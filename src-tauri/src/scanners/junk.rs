use crate::fsutil::{summarize, unix_secs};
use crate::model::{Action, Category, Finding, NameFilter, Risk};
use crate::scoring::{format_bytes, score};
use crate::sys::SystemOps;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub struct KnownPaths {
    pub temp: PathBuf,
    pub windows: PathBuf,
    pub local_app_data: PathBuf,
    pub program_data: PathBuf,
}

impl KnownPaths {
    pub fn from_env() -> Option<Self> {
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
        Some(Self {
            temp: std::env::temp_dir(),
            windows: env("SystemRoot")?,
            local_app_data: env("LOCALAPPDATA")?,
            program_data: env("ProgramData")?,
        })
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
        action: Action::EmptyRecycleBin,
    })
}

pub fn scan_junk(p: &KnownPaths, sys: &dyn SystemOps, now: SystemTime) -> Vec<Finding> {
    let mut found: Vec<Finding> = junk_locations(p).iter().filter_map(|l| scan_location(l, now)).collect();
    let (bytes, items) = sys.recycle_bin_info();
    found.extend(recycle_bin_finding(bytes, items));
    found.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsutil::test_support::file;

    struct FakeSys(u64, u64);
    impl SystemOps for FakeSys {
        fn recycle_bin_info(&self) -> (u64, u64) { (self.0, self.1) }
        fn empty_recycle_bin(&self) -> Result<(), String> { Ok(()) }
    }

    fn fake_paths(dir: &Path) -> KnownPaths {
        KnownPaths {
            temp: dir.join("Temp"),
            windows: dir.join("Windows"),
            local_app_data: dir.join("Local"),
            program_data: dir.join("ProgramData"),
        }
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

        let found = scan_junk(&p, &FakeSys(0, 0), SystemTime::now());
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
        assert!(scan_junk(&p, &FakeSys(0, 0), SystemTime::now()).is_empty());
    }

    #[test]
    fn system_locations_require_admin() {
        let dir = tempfile::tempdir().unwrap();
        let p = fake_paths(dir.path());
        file(&p.windows.join("Temp/old.log"), 10, 48);
        file(&p.windows.join("SoftwareDistribution/Download/kb.cab"), 10, 100);

        let found = scan_junk(&p, &FakeSys(0, 0), SystemTime::now());
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

        let found = scan_junk(&p, &FakeSys(0, 0), SystemTime::now());
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

        let found = titles(&scan_junk(&p, &FakeSys(0, 0), SystemTime::now()));
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
        let found = scan_junk(&p, &FakeSys(5 * 1024 * 1024, 1), SystemTime::now());
        let scores: Vec<u8> = found.iter().map(|f| f.score).collect();
        let mut sorted = scores.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(scores, sorted);
        assert_eq!(found[0].title, "Your temporary files");
    }
}
