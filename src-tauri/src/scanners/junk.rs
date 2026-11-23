use crate::fsutil::{summarize, unix_secs};
use crate::model::{Action, Category, Finding, NameFilter, Risk};
use crate::scoring::{format_bytes, score};
use crate::sys::SystemOps;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub struct KnownPaths {
    pub temp: PathBuf,
    pub windows: PathBuf,
    pub local_app_data: PathBuf,
    pub app_data: PathBuf,
    pub program_data: PathBuf,
    pub home: PathBuf,
    /// Tool cache-location overrides (CARGO_HOME, GOCACHE, …) that are set on this machine.
    pub env: HashMap<String, PathBuf>,
}

/// Environment variables developer tools use to move their caches.
const OVERRIDE_VARS: [&str; 8] = [
    "CARGO_HOME",
    "RUSTUP_HOME",
    "npm_config_cache",
    "GOCACHE",
    "PIP_CACHE_DIR",
    "UV_CACHE_DIR",
    "GRADLE_USER_HOME",
    "YARN_CACHE_FOLDER",
];

impl KnownPaths {
    pub fn from_env() -> Option<Self> {
        let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        Some(Self {
            temp: std::env::temp_dir(),
            windows: env("SystemRoot")?,
            local_app_data: env("LOCALAPPDATA")?,
            app_data: env("APPDATA")?,
            program_data: env("ProgramData")?,
            home: env("USERPROFILE")?,
            env: OVERRIDE_VARS.iter().filter_map(|k| env(k).map(|v| (k.to_string(), v))).collect(),
        })
    }

    /// `env[var]` if the tool's override is set, otherwise `default`.
    fn tool_dir(&self, var: &str, default: PathBuf) -> PathBuf {
        self.env.get(var).cloned().unwrap_or(default)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct JunkLocation {
    pub title: String,
    pub root: PathBuf,
    pub filter: NameFilter,
    pub min_age_hours: u64,
    pub risk: Risk,
    pub requires_admin: bool,
    pub explanation: &'static str,
}

const TEMP_WHY: &str = "Apps leave files here and rarely clean up. Only files older than 24 hours are removed, so running programs aren't affected.";
const DUMP_WHY: &str = "Memory dumps written when an app crashed. They're only useful to a developer debugging that crash.";
const WER_WHY: &str = "Copies of crash reports Windows already sent or no longer needs.";
const THUMB_WHY: &str = "Explorer rebuilds thumbnails when it needs them. Files Explorer has open are skipped.";
const WU_WHY: &str = "Installer files for updates. Windows downloads anything it still needs again. Files from the last 3 days are kept in case an update is in progress.";
const DO_WHY: &str = "Update pieces Windows keeps to share with other PCs. Safe to clear; Windows refills it if needed.";
const BROWSER_WHY: &str = "Your browser rebuilds this as you browse. Close the browser first; files in use are skipped. Cookies, passwords and history are not touched.";

fn loc(title: impl Into<String>, root: PathBuf, min_age_hours: u64, risk: Risk, requires_admin: bool, explanation: &'static str) -> JunkLocation {
    JunkLocation { title: title.into(), root, filter: NameFilter::any(), min_age_hours, risk, requires_admin, explanation }
}

/// Child folders of `base` that contain `sub`, as (child name, base/child/sub), sorted by name.
fn profiles(base: &Path, sub: &str) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(base) else { return Vec::new() };
    let mut out: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path().join(sub)))
        .filter(|(_, p)| p.is_dir())
        .collect();
    out.sort();
    out
}

pub fn junk_locations(p: &KnownPaths) -> Vec<JunkLocation> {
    let wer = p.program_data.join("Microsoft\\Windows\\WER");
    let mut v = vec![
        loc("Your temporary files", p.temp.clone(), 24, Risk::Safe, false, TEMP_WHY),
        loc("Windows temporary files", p.windows.join("Temp"), 24, Risk::Safe, true, TEMP_WHY),
        loc("Crash dumps", p.local_app_data.join("CrashDumps"), 0, Risk::Safe, false, DUMP_WHY),
        loc("Windows error reports (archived)", wer.join("ReportArchive"), 0, Risk::Safe, true, WER_WHY),
        loc("Windows error reports (queued)", wer.join("ReportQueue"), 0, Risk::Safe, true, WER_WHY),
        JunkLocation {
            filter: NameFilter { prefix: Some("thumbcache_".into()), suffix: Some(".db".into()) },
            ..loc("Thumbnail cache", p.local_app_data.join("Microsoft\\Windows\\Explorer"), 0, Risk::Safe, false, THUMB_WHY)
        },
        loc("Windows Update downloads", p.windows.join("SoftwareDistribution\\Download"), 72, Risk::Review, true, WU_WHY),
        loc(
            "Delivery Optimization cache",
            p.windows.join("ServiceProfiles\\NetworkService\\AppData\\Local\\Microsoft\\Windows\\DeliveryOptimization\\Cache"),
            0,
            Risk::Safe,
            true,
            DO_WHY,
        ),
    ];

    let chromium = [
        ("Chrome", "Google\\Chrome\\User Data"),
        ("Edge", "Microsoft\\Edge\\User Data"),
        ("Brave", "BraveSoftware\\Brave-Browser\\User Data"),
    ];
    for (browser, rel) in chromium {
        let base = p.local_app_data.join(rel);
        for (profile, root) in profiles(&base, "Cache") {
            v.push(loc(format!("{browser} cache ({profile})"), root, 0, Risk::Safe, false, BROWSER_WHY));
        }
        for (profile, root) in profiles(&base, "Code Cache") {
            v.push(loc(format!("{browser} code cache ({profile})"), root, 0, Risk::Safe, false, BROWSER_WHY));
        }
    }
    for (profile, root) in profiles(&p.local_app_data.join("Mozilla\\Firefox\\Profiles"), "cache2") {
        v.push(loc(format!("Firefox cache ({profile})"), root, 0, Risk::Safe, false, BROWSER_WHY));
    }
    v.extend(dev_locations(p));
    v.extend(app_locations(p));
    v
}

const PKG_WHY: &str = "Downloaded packages kept so installs are faster. Your projects keep working; the next install downloads what it needs again. Close running installs and builds first.";
const PNPM_WHY: &str = "pnpm's shared package store. Packages your projects still use stay on disk through hard links, so the space freed can be less than shown. Running `pnpm store prune` instead removes only unused packages.";
const BUILD_WHY: &str = "Build outputs the tool recreates on the next build, which will take longer once. Close running builds first.";
const BROWSERS_WHY: &str = "Browser engines downloaded for automated testing. Your test runner downloads them again the next time it needs them.";
const APP_CACHE_WHY: &str = "The app rebuilds this cache when it needs it. Close the app first; files in use are skipped.";
const SHADER_WHY: &str = "Compiled graphics shaders. Games and apps rebuild them, so the first launch afterwards may be a little slower.";

/// Package-manager, toolchain and build caches. All regenerable, so permanent deletion is fine,
/// but re-downloading costs time and bandwidth, so they need review rather than being pre-checked.
fn dev_locations(p: &KnownPaths) -> Vec<JunkLocation> {
    let local = &p.local_app_data;
    let home = &p.home;
    let cargo = p.tool_dir("CARGO_HOME", home.join(".cargo"));
    let rustup = p.tool_dir("RUSTUP_HOME", home.join(".rustup"));
    let gradle = p.tool_dir("GRADLE_USER_HOME", home.join(".gradle"));
    let review = |title: &str, root: PathBuf, why: &'static str| loc(title, root, 0, Risk::Review, false, why);
    vec![
        review("npm cache", p.tool_dir("npm_config_cache", local.join("npm-cache")), PKG_WHY),
        review("pnpm store", local.join("pnpm\\store"), PNPM_WHY),
        review("Yarn cache", p.tool_dir("YARN_CACHE_FOLDER", local.join("Yarn\\Cache")), PKG_WHY),
        review("Yarn Berry cache", local.join("Yarn\\Berry\\cache"), PKG_WHY),
        review("Bun cache", home.join(".bun\\install\\cache"), PKG_WHY),
        review("Deno cache", local.join("deno"), PKG_WHY),
        review("node-gyp headers", local.join("node-gyp\\Cache"), PKG_WHY),
        review("Electron downloads", local.join("electron\\Cache"), PKG_WHY),
        review("electron-builder downloads", local.join("electron-builder\\Cache"), PKG_WHY),
        review("Playwright browsers", local.join("ms-playwright"), BROWSERS_WHY),
        review("Puppeteer browsers", home.join(".cache\\puppeteer"), BROWSERS_WHY),
        review("Cargo downloaded crates", cargo.join("registry\\cache"), PKG_WHY),
        review("Cargo extracted sources", cargo.join("registry\\src"), PKG_WHY),
        review("Cargo git checkouts", cargo.join("git\\checkouts"), PKG_WHY),
        review("Rustup downloads", rustup.join("downloads"), PKG_WHY),
        review("Rustup temporary files", rustup.join("tmp"), PKG_WHY),
        review("pip cache", p.tool_dir("PIP_CACHE_DIR", local.join("pip\\Cache")), PKG_WHY),
        review("uv cache", p.tool_dir("UV_CACHE_DIR", local.join("uv\\cache")), PKG_WHY),
        review("NuGet HTTP cache", local.join("NuGet\\v3-cache"), PKG_WHY),
        review("Gradle caches", gradle.join("caches"), PKG_WHY),
        review("Maven repository", home.join(".m2\\repository"), PKG_WHY),
        review("Go build cache", p.tool_dir("GOCACHE", local.join("go-build")), BUILD_WHY),
        review("TypeScript server cache", local.join("Microsoft\\TypeScript"), BUILD_WHY),
    ]
}

/// Caches of everyday desktop apps and the graphics stack.
fn app_locations(p: &KnownPaths) -> Vec<JunkLocation> {
    let safe = |title: String, root: PathBuf, why: &'static str| loc(title, root, 0, Risk::Safe, false, why);
    let mut v = Vec::new();
    for (app, rel) in [("VS Code", "Code"), ("Discord", "discord"), ("Slack", "Slack"), ("Microsoft Teams (classic)", "Microsoft\\Teams")] {
        for sub in ["Cache", "CachedData", "Code Cache", "GPUCache"] {
            v.push(safe(format!("{app} cache ({sub})"), p.app_data.join(rel).join(sub), APP_CACHE_WHY));
        }
    }
    v.push(safe("VS Code extension downloads".into(), p.app_data.join("Code\\CachedExtensionVSIXs"), APP_CACHE_WHY));
    v.push(safe("Spotify cache".into(), p.local_app_data.join("Spotify\\Data"), APP_CACHE_WHY));
    v.push(safe("Internet cache".into(), p.local_app_data.join("Microsoft\\Windows\\INetCache"), APP_CACHE_WHY));
    v.push(safe("DirectX shader cache".into(), p.local_app_data.join("D3DSCache"), SHADER_WHY));
    v.push(safe("NVIDIA DirectX shader cache".into(), p.local_app_data.join("NVIDIA\\DXCache"), SHADER_WHY));
    v.push(safe("NVIDIA OpenGL shader cache".into(), p.local_app_data.join("NVIDIA\\GLCache"), SHADER_WHY));
    v
}

pub fn scan_location(l: &JunkLocation, now: SystemTime) -> Option<Finding> {
    if !l.root.is_dir() {
        return None;
    }
    let stats = summarize(&l.root, &l.filter, Duration::from_secs(l.min_age_hours * 3600), now);
    if stats.files == 0 {
        return None;
    }
    Some(Finding {
        id: format!("junk:{}", l.root.to_string_lossy().to_lowercase()),
        category: Category::Junk,
        title: l.title.clone(),
        size_bytes: stats.bytes,
        item_count: stats.files,
        last_modified: stats.newest.map(unix_secs),
        risk: l.risk,
        score: score(l.risk, stats.bytes),
        reason: format!("{} in {} files. {}", format_bytes(stats.bytes), stats.files, l.explanation),
        requires_admin: l.requires_admin,
        reversible: false,
        upper_bound: false,
        action: Action::DeleteContents { root: l.root.clone(), filter: l.filter.clone(), min_age_hours: l.min_age_hours },
    })
}

pub fn recycle_bin_finding(bytes: u64, items: u64) -> Option<Finding> {
    if items == 0 {
        return None;
    }
    Some(Finding {
        id: "junk:recycle-bin".into(),
        category: Category::Junk,
        title: "Recycle Bin".into(),
        size_bytes: bytes,
        item_count: items,
        last_modified: None,
        risk: Risk::Review,
        score: score(Risk::Review, bytes),
        reason: format!(
            "{} in {items} items you already deleted. Emptying it is permanent, so check it holds nothing you want back.",
            format_bytes(bytes)
        ),
        requires_admin: false,
        reversible: false,
        upper_bound: false,
        action: Action::EmptyRecycleBin,
    })
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    /// Locations finished so far, including this one.
    pub done: usize,
    pub total: usize,
    /// Title of the location that just finished.
    pub title: String,
}

/// Scans every location in parallel, calling `on_progress` (from worker threads) as each finishes.
pub fn scan_junk_with_progress(
    p: &KnownPaths,
    sys: &dyn SystemOps,
    now: SystemTime,
    on_progress: &(dyn Fn(ScanProgress) + Sync),
) -> Vec<Finding> {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let locations = junk_locations(p);
    let total = locations.len() + 1; // + Recycle Bin
    let done = AtomicUsize::new(0);
    let tick = |title: &str| {
        let done = done.fetch_add(1, Ordering::SeqCst) + 1;
        on_progress(ScanProgress { done, total, title: title.to_string() });
    };

    let mut found: Vec<Finding> = locations
        .par_iter()
        .filter_map(|l| {
            let f = scan_location(l, now);
            tick(&l.title);
            f
        })
        .collect();
    let (bytes, items) = sys.recycle_bin_info();
    tick("Recycle Bin");
    found.extend(recycle_bin_finding(bytes, items));
    found.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
    found
}

pub fn scan_junk(p: &KnownPaths, sys: &dyn SystemOps, now: SystemTime) -> Vec<Finding> {
    scan_junk_with_progress(p, sys, now, &|_| {})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsutil::test_support::file;

    use crate::sys::fake::FakeSys;

    fn fake_paths(dir: &Path) -> KnownPaths {
        KnownPaths {
            temp: dir.join("Temp"),
            windows: dir.join("Windows"),
            local_app_data: dir.join("Local"),
            app_data: dir.join("Roaming"),
            program_data: dir.join("ProgramData"),
            home: dir.join("Home"),
            env: HashMap::new(),
        }
    }

    fn find<'a>(found: &'a [Finding], title: &str) -> &'a Finding {
        found.iter().find(|f| f.title == title).unwrap_or_else(|| panic!("no finding titled {title:?} in {:?}", titles(found)))
    }

    /// Read-only timing of a scan of this machine's real junk locations.
    /// Run with: cargo test --lib scan_this_machine -- --ignored --nocapture
    #[test]
    #[ignore]
    fn scan_this_machine() {
        let p = KnownPaths::from_env().unwrap();
        let now = SystemTime::now();
        let t = std::time::Instant::now();
        let found = scan_junk(&p, &crate::sys::WindowsSys, now);
        println!("parallel scan: {:.2}s, {} findings", t.elapsed().as_secs_f64(), found.len());
        let now = SystemTime::now();
        for l in junk_locations(&p) {
            let t = std::time::Instant::now();
            if let Some(f) = scan_location(&l, now) {
                println!("{:>8.2}s  {:>10}  {:>9} files  {}", t.elapsed().as_secs_f64(), format_bytes(f.size_bytes), f.item_count, f.title);
            }
        }
        println!("total: {:.2}s", now.elapsed().unwrap().as_secs_f64());
    }

    #[test]
    fn progress_is_reported_once_per_location_up_to_total() {
        let dir = tempfile::tempdir().unwrap();
        let p = fake_paths(dir.path());
        file(&p.temp.join("old.tmp"), 10, 48);
        let seen = std::sync::Mutex::new(Vec::new());

        let found = scan_junk_with_progress(&p, &FakeSys::with_bin(0, 0), SystemTime::now(), &|pr| seen.lock().unwrap().push(pr));

        let seen = seen.into_inner().unwrap();
        let total = junk_locations(&p).len() + 1; // + Recycle Bin
        assert_eq!(seen.len(), total);
        assert!(seen.iter().all(|s| s.total == total));
        let mut done: Vec<usize> = seen.iter().map(|s| s.done).collect();
        done.sort();
        assert_eq!(done, (1..=total).collect::<Vec<_>>());
        assert!(seen.iter().any(|s| s.title == "Your temporary files"));
        assert_eq!(titles(&found), titles(&scan_junk(&p, &FakeSys::with_bin(0, 0), SystemTime::now())));
    }

    #[test]
    fn developer_caches_are_found_and_need_review() {
        let dir = tempfile::tempdir().unwrap();
        let p = fake_paths(dir.path());
        file(&p.local_app_data.join("npm-cache/_cacache/content-v2/sha512/ab"), 10, 0);
        file(&p.local_app_data.join("pnpm/store/v10/files/00/abc"), 10, 0);
        file(&p.home.join(".cargo/registry/cache/index.crates.io-1/serde-1.0.0.crate"), 10, 0);
        file(&p.home.join(".cargo/registry/src/index.crates.io-1/serde-1.0.0/lib.rs"), 10, 0);
        file(&p.home.join(".gradle/caches/modules-2/x.jar"), 10, 0);

        let found = scan_junk(&p, &FakeSys::with_bin(0, 0), SystemTime::now());
        for title in ["npm cache", "pnpm store", "Cargo downloaded crates", "Cargo extracted sources", "Gradle caches"] {
            let f = find(&found, title);
            assert_eq!(f.risk, Risk::Review, "{title}");
            assert!(!f.requires_admin, "{title}");
        }
    }

    #[test]
    fn app_caches_are_safe() {
        let dir = tempfile::tempdir().unwrap();
        let p = fake_paths(dir.path());
        file(&p.app_data.join("Code/CachedData/abc/x.bin"), 10, 0);
        file(&p.local_app_data.join("D3DSCache/x"), 10, 0);

        let found = scan_junk(&p, &FakeSys::with_bin(0, 0), SystemTime::now());
        assert_eq!(find(&found, "VS Code cache (CachedData)").risk, Risk::Safe);
        assert_eq!(find(&found, "DirectX shader cache").risk, Risk::Safe);
    }

    #[test]
    fn env_overrides_move_tool_caches() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = fake_paths(dir.path());
        let custom = dir.path().join("custom-cargo");
        p.env.insert("CARGO_HOME".into(), custom.clone());
        file(&custom.join("registry/cache/idx/a.crate"), 30, 0);
        file(&p.home.join(".cargo/registry/cache/idx/b.crate"), 99, 0);

        let found = scan_junk(&p, &FakeSys::with_bin(0, 0), SystemTime::now());
        let cargo = find(&found, "Cargo downloaded crates");
        assert_eq!(cargo.size_bytes, 30);
        assert_eq!(found.iter().filter(|f| f.title == "Cargo downloaded crates").count(), 1);
    }

    fn titles(findings: &[Finding]) -> Vec<String> {
        findings.iter().map(|f| f.title.clone()).collect()
    }

    #[test]
    fn finds_user_temp_older_than_a_day_only() {
        let dir = tempfile::tempdir().unwrap();
        let p = fake_paths(dir.path());
        file(&p.temp.join("old.tmp"), 2048, 48);
        file(&p.temp.join("new.tmp"), 4096, 1);

        let found = scan_junk(&p, &FakeSys::with_bin(0, 0), SystemTime::now());
        let temp = found.iter().find(|f| f.title == "Your temporary files").unwrap();
        assert_eq!(temp.size_bytes, 2048);
        assert_eq!(temp.item_count, 1);
        assert_eq!(temp.risk, Risk::Safe);
        assert!(!temp.requires_admin);
        assert!(temp.reason.starts_with("2.0 KB in 1 files."));
    }

    #[test]
    fn missing_or_empty_locations_produce_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let p = fake_paths(dir.path());
        std::fs::create_dir_all(p.local_app_data.join("CrashDumps")).unwrap();
        assert!(scan_junk(&p, &FakeSys::with_bin(0, 0), SystemTime::now()).is_empty());
    }

    #[test]
    fn system_locations_require_admin() {
        let dir = tempfile::tempdir().unwrap();
        let p = fake_paths(dir.path());
        file(&p.windows.join("Temp/old.log"), 10, 48);
        file(&p.windows.join("SoftwareDistribution/Download/kb.cab"), 10, 100);

        let found = scan_junk(&p, &FakeSys::with_bin(0, 0), SystemTime::now());
        let win = found.iter().find(|f| f.title == "Windows temporary files").unwrap();
        assert!(win.requires_admin);
        let wu = found.iter().find(|f| f.title == "Windows Update downloads").unwrap();
        assert_eq!(wu.risk, Risk::Review);
    }

    #[test]
    fn thumbnail_cache_counts_only_thumbcache_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = fake_paths(dir.path());
        let explorer = p.local_app_data.join("Microsoft/Windows/Explorer");
        file(&explorer.join("thumbcache_256.db"), 100, 0);
        file(&explorer.join("iconcache_256.db"), 999, 0);

        let found = scan_junk(&p, &FakeSys::with_bin(0, 0), SystemTime::now());
        let thumbs = found.iter().find(|f| f.title == "Thumbnail cache").unwrap();
        assert_eq!(thumbs.size_bytes, 100);
    }

    #[test]
    fn browser_caches_are_found_per_profile() {
        let dir = tempfile::tempdir().unwrap();
        let p = fake_paths(dir.path());
        let chrome = p.local_app_data.join("Google/Chrome/User Data");
        file(&chrome.join("Default/Cache/Cache_Data/f_0001"), 10, 0);
        file(&chrome.join("Profile 1/Code Cache/js/abc"), 10, 0);
        file(&p.local_app_data.join("Mozilla/Firefox/Profiles/x1.default/cache2/entries/E1"), 10, 0);

        let found = titles(&scan_junk(&p, &FakeSys::with_bin(0, 0), SystemTime::now()));
        assert!(found.contains(&"Chrome cache (Default)".to_string()));
        assert!(found.contains(&"Chrome code cache (Profile 1)".to_string()));
        assert!(found.contains(&"Firefox cache (x1.default)".to_string()));
    }

    #[test]
    fn recycle_bin_finding_only_when_not_empty() {
        assert!(recycle_bin_finding(0, 0).is_none());
        let f = recycle_bin_finding(1024, 2).unwrap();
        assert_eq!(f.action, Action::EmptyRecycleBin);
        assert_eq!(f.risk, Risk::Review);
        assert_eq!(f.item_count, 2);
    }

    #[test]
    fn results_are_sorted_by_score() {
        let dir = tempfile::tempdir().unwrap();
        let p = fake_paths(dir.path());
        file(&p.temp.join("big.tmp"), 20 * 1024 * 1024, 48);
        file(&p.local_app_data.join("CrashDumps/a.dmp"), 10, 0);
        let found = scan_junk(&p, &FakeSys::with_bin(5 * 1024 * 1024, 1), SystemTime::now());
        let scores: Vec<u8> = found.iter().map(|f| f.score).collect();
        let mut sorted = scores.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(scores, sorted);
        assert_eq!(found[0].title, "Your temporary files");
    }
}
