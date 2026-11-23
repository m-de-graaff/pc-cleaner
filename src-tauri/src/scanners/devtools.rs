//! Developer tooling that hides space outside plain cache folders: Docker data, WSL virtual
//! disks, old Rust toolchains and Android emulators. Read-only: actions run in the executor.

use super::junk::KnownPaths;
use crate::fsutil::{summarize, unix_secs};
use crate::model::{Action, Category, DockerTarget, Finding, NameFilter, Risk, WslTask};
use crate::scoring::{format_bytes, score};
use crate::sys::SystemOps;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevScan {
    pub findings: Vec<Finding>,
    /// Things the user should know that aren't findings, e.g. "Docker isn't running".
    pub notices: Vec<String>,
}

const DOCKER_CACHE_WHY: &str = "Layers Docker keeps to speed up later builds. The next build that needs them takes longer once.";
const DOCKER_IMAGES_WHY: &str = "Images no container uses. Pulling or building them again takes time and bandwidth.";
const DOCKER_CONTAINERS_WHY: &str = "Containers that have stopped. Anything written inside them, outside volumes, is lost.";
const DOCKER_VOLUME_WHY: &str = "A volume no container uses. Volumes usually hold database or app data; deleting it is permanent. Only remove it if you know what it held.";
const WSL_WHY: &str = "WSL and Docker disks grow but never give freed space back to Windows on their own; compacting does. How much it returns depends on what was deleted inside, so this is a maximum. It shuts down WSL and Docker Desktop while it runs; quit Docker Desktop first.";
const TOOLCHAIN_WHY: &str = "An installed Rust toolchain that isn't your default. If a project pins it in rust-toolchain.toml, rustup downloads it again when you build that project.";

/// Docker prints sizes with decimal units ("1.5kB", "2.3GB (45%)").
pub fn parse_docker_size(s: &str) -> Option<u64> {
    let s = s.split_whitespace().next()?;
    let split = s.find(|c: char| c.is_ascii_alphabetic())?;
    let (number, unit) = s.split_at(split);
    let factor: f64 = match unit {
        "B" => 1.0,
        "kB" | "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        _ => return None,
    };
    Some((number.parse::<f64>().ok()? * factor).round() as u64)
}

fn dev_finding(id: String, title: String, size: u64, risk: Risk, reason: String, action: Action) -> Finding {
    Finding {
        id,
        category: Category::DevTools,
        title,
        size_bytes: size,
        item_count: 1,
        last_modified: None,
        risk,
        score: score(risk, size),
        reason,
        requires_admin: false,
        reversible: false,
        upper_bound: false,
        action,
    }
}

pub fn docker_findings(sys: &dyn SystemOps) -> (Vec<Finding>, Vec<String>) {
    let Ok(df) = sys.run("docker", &["system", "df", "--format", "{{json .}}"]) else {
        return (Vec::new(), vec!["Docker isn't running, so its images, build cache and volumes weren't checked. Start Docker Desktop and scan again.".into()]);
    };
    let mut found = Vec::new();
    for line in df.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(row) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let size = row["Reclaimable"].as_str().and_then(parse_docker_size).unwrap_or(0);
        let (target, title, risk, why) = match row["Type"].as_str() {
            Some("Build Cache") => (DockerTarget::BuildCache, "Docker build cache", Risk::Safe, DOCKER_CACHE_WHY),
            Some("Images") => (DockerTarget::Images, "Unused Docker images", Risk::Review, DOCKER_IMAGES_WHY),
            Some("Containers") => (DockerTarget::Containers, "Stopped Docker containers", Risk::Review, DOCKER_CONTAINERS_WHY),
            _ => continue, // volumes are listed one by one below
        };
        if size > 0 {
            let id = format!("devtools:docker:{}", serde_json::to_value(target).unwrap().as_str().unwrap());
            found.push(dev_finding(id, title.into(), size, risk, format!("{}. {why}", format_bytes(size)), Action::DockerPrune { target }));
        }
    }

    let volume_sizes: std::collections::HashMap<String, u64> = sys
        .run("docker", &["system", "df", "-v", "--format", "{{json .}}"])
        .ok()
        .and_then(|out| serde_json::from_str::<serde_json::Value>(&out).ok())
        .and_then(|v| v["Volumes"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|v| Some((v["Name"].as_str()?.to_string(), v["Size"].as_str().and_then(parse_docker_size).unwrap_or(0))))
        .collect();
    let dangling = sys.run("docker", &["volume", "ls", "-q", "--filter", "dangling=true"]).unwrap_or_default();
    for name in dangling.lines().map(str::trim).filter(|n| !n.is_empty()) {
        let size = volume_sizes.get(name).copied().unwrap_or(0);
        let size_text = if size > 0 { format_bytes(size) } else { "Size unknown".into() };
        found.push(dev_finding(
            format!("devtools:docker:volume:{name}"),
            format!("Unused Docker volume: {name}"),
            size,
            Risk::Caution,
            format!("{size_text}. {DOCKER_VOLUME_WHY}"),
            Action::DockerRemoveVolume { name: name.into() },
        ));
    }
    (found, Vec::new())
}

/// Disks smaller than this aren't worth shutting WSL down for.
const MIN_COMPACT_BYTES: u64 = 512 * 1024 * 1024;

fn vhdx_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("vhdx")))
        .collect();
    v.sort();
    v
}

fn subdirs(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect();
    v.sort();
    v
}

pub fn wsl_disk_findings(p: &KnownPaths) -> Vec<Finding> {
    let local = &p.local_app_data;
    let mut disks: Vec<(String, PathBuf)> = Vec::new();
    for (package, dir) in subdirs(&local.join("Packages")) {
        // "CanonicalGroupLimited.Ubuntu_79rhkp1fndgsc" -> "Ubuntu"
        let product = package.split('_').next().unwrap_or(&package);
        let name = product.split_once('.').map_or(product, |(_, rest)| rest).to_string();
        disks.extend(vhdx_in(&dir.join("LocalState")).into_iter().map(|d| (name.clone(), d)));
    }
    for (sub, dir) in subdirs(&local.join("Docker\\wsl")) {
        let name = match sub.as_str() {
            "disk" => "Docker Desktop data".to_string(),
            "main" => "Docker Desktop system".to_string(),
            other => format!("Docker Desktop ({other})"),
        };
        disks.extend(vhdx_in(&dir).into_iter().map(|d| (name.clone(), d)));
    }
    for (sub, dir) in subdirs(&local.join("wsl")) {
        disks.extend(vhdx_in(&dir).into_iter().map(|d| (sub.clone(), d)));
    }

    disks
        .into_iter()
        .filter_map(|(name, path)| {
            let size = std::fs::metadata(&path).ok()?.len();
            (size >= MIN_COMPACT_BYTES).then(|| Finding {
                requires_admin: true,
                upper_bound: true,
                ..dev_finding(
                    format!("devtools:wsl:{}", path.to_string_lossy().to_lowercase()),
                    format!("Compact WSL disk: {name}"),
                    size,
                    Risk::Review,
                    format!("The disk file is {}. {WSL_WHY}", format_bytes(size)),
                    Action::CompactDisk { path },
                )
            })
        })
        .collect()
}

pub fn toolchain_findings(p: &KnownPaths, sys: &dyn SystemOps) -> Vec<Finding> {
    let Ok(list) = sys.run("rustup", &["toolchain", "list"]) else { return Vec::new() };
    let toolchains = p.tool_dir("RUSTUP_HOME", p.home.join(".rustup")).join("toolchains");
    list.lines()
        .filter(|l| !l.contains("default") && !l.contains("active")) // "(default)", "(active)", "(active, default)"
        .filter_map(|l| l.split_whitespace().next())
        .filter_map(|name| {
            let stats = summarize(&toolchains.join(name), &NameFilter::any(), Duration::ZERO, SystemTime::now());
            (stats.bytes > 0).then(|| {
                dev_finding(
                    format!("devtools:rust:{name}"),
                    format!("Rust toolchain {name}"),
                    stats.bytes,
                    Risk::Review,
                    format!("{}. {TOOLCHAIN_WHY}", format_bytes(stats.bytes)),
                    Action::RemoveToolchain { name: name.into() },
                )
            })
        })
        .collect()
}

pub fn avd_findings(p: &KnownPaths, now: SystemTime) -> Vec<Finding> {
    let home = p.tool_dir("ANDROID_AVD_HOME", p.home.join(".android\\avd"));
    subdirs(&home)
        .into_iter()
        .filter_map(|(dir_name, dir)| {
            let name = dir_name.strip_suffix(".avd")?.to_string();
            let stats = summarize(&dir, &NameFilter::any(), Duration::ZERO, now);
            let days = stats.newest.and_then(|t| now.duration_since(t).ok()).map_or(0, |d| d.as_secs() / 86_400);
            let reason = format!(
                "{}, last used {days} days ago. Deleting removes the emulator and everything installed on it. You can recreate it in Android Studio's Device Manager.",
                format_bytes(stats.bytes)
            );
            Some(Finding {
                last_modified: stats.newest.map(unix_secs),
                ..dev_finding(
                    format!("devtools:avd:{}", name.to_lowercase()),
                    format!("Android emulator: {name}"),
                    stats.bytes,
                    Risk::Caution,
                    reason,
                    Action::DeletePaths { paths: vec![dir.clone(), home.join(format!("{name}.ini"))] },
                )
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Distro {
    pub name: String,
    pub vhdx: PathBuf,
}

/// Distros from `reg query HKCU\...\Lxss /s`: one block per distro GUID with
/// DistributionName, BasePath and VhdFileName, separated by four spaces.
pub fn parse_lxss(reg_output: &str) -> Vec<Distro> {
    let mut distros = Vec::new();
    let mut block: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut flush = |block: &mut std::collections::HashMap<String, String>| {
        if let (Some(name), Some(base)) = (block.get("DistributionName"), block.get("BasePath")) {
            let base = base.strip_prefix("\\\\?\\").unwrap_or(base);
            let file = block.get("VhdFileName").map_or("ext4.vhdx", String::as_str);
            distros.push(Distro { name: name.clone(), vhdx: Path::new(base).join(file) });
        }
        block.clear();
    };
    for line in reg_output.lines() {
        if line.starts_with("HKEY_") {
            flush(&mut block);
            continue;
        }
        let parts: Vec<&str> = line.trim().splitn(3, "    ").collect();
        if let [key, _kind, value] = parts[..] {
            block.insert(key.to_string(), value.trim().to_string());
        }
    }
    flush(&mut block);
    distros
}

const INSIDE_WHY: &str = "This frees space inside Linux. Compact the distro's disk afterwards to give it back to Windows.";
const LINUX_SIZES: &str = "du -sb /var/cache/apt/archives /var/log/journal /root/.cache /home/*/.cache 2>/dev/null; true";

fn linux_findings(sys: &dyn SystemOps, distro: &str) -> Vec<Finding> {
    let Ok(out) = sys.run("wsl", &["-d", distro, "-u", "root", "--", "sh", "-c", LINUX_SIZES]) else { return Vec::new() };
    let (mut apt, mut journal, mut caches) = (0u64, 0u64, 0u64);
    for line in out.lines() {
        let Some((bytes, path)) = line.split_once('\t') else { continue };
        let Ok(bytes) = bytes.trim().parse::<u64>() else { continue };
        match path.trim() {
            "/var/cache/apt/archives" => apt += bytes,
            "/var/log/journal" => journal += bytes,
            p if p.ends_with("/.cache") => caches += bytes,
            _ => {}
        }
    }
    let make = |task: WslTask, title: &str, size: u64, risk: Risk, why: &str| {
        dev_finding(
            format!("devtools:wsl-clean:{}:{}", distro.to_lowercase(), serde_json::to_value(task).unwrap().as_str().unwrap()),
            format!("{title} ({distro})"),
            size,
            risk,
            format!("{}. {why} {INSIDE_WHY}", format_bytes(size)),
            Action::WslClean { distro: distro.into(), task },
        )
    };
    let mut v = Vec::new();
    if apt > 0 {
        v.push(make(WslTask::AptCache, "apt package cache", apt, Risk::Safe, "Downloaded .deb packages that are already installed."));
    }
    if caches > 0 {
        v.push(make(
            WslTask::UserCaches,
            "User caches in ~/.cache",
            caches,
            Risk::Review,
            "Caches of Linux tools (pip, npm, browsers for tests and more). They rebuild when needed, but a few tools keep state here.",
        ));
    }
    if journal > 0 {
        v.push(Finding {
            upper_bound: true,
            ..make(WslTask::Journal, "System logs older than 7 days", journal, Risk::Review, "The systemd journal. This is its total size; only entries older than 7 days are removed.")
        });
    }
    v
}

/// Cleanups inside each WSL distro, except Docker Desktop's own, which Docker manages.
pub fn distro_findings(sys: &dyn SystemOps) -> Vec<Finding> {
    let Ok(reg) = sys.run("reg", &["query", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Lxss", "/s"]) else {
        return Vec::new();
    };
    parse_lxss(&reg)
        .into_iter()
        .filter(|d| !d.name.starts_with("docker-desktop"))
        .flat_map(|d| linux_findings(sys, &d.name))
        .collect()
}

pub fn scan_devtools(p: &KnownPaths, sys: &dyn SystemOps, now: SystemTime) -> DevScan {
    let (mut findings, notices) = docker_findings(sys);
    findings.extend(wsl_disk_findings(p));
    findings.extend(toolchain_findings(p, sys));
    findings.extend(avd_findings(p, now));
    findings.extend(distro_findings(sys));
    findings.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
    DevScan { findings, notices }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsutil::test_support::file;
    use crate::sys::fake::FakeSys;
    use std::collections::HashMap;

    const DF: &str = "docker system df --format {{json .}}";
    const DF_V: &str = "docker system df -v --format {{json .}}";
    const DANGLING: &str = "docker volume ls -q --filter dangling=true";
    const DF_OUT: &str = r#"{"Active":"3","Reclaimable":"2.1GB (45%)","Size":"4.7GB","TotalCount":"10","Type":"Images"}
{"Active":"1","Reclaimable":"0B (0%)","Size":"12kB","TotalCount":"4","Type":"Containers"}
{"Active":"1","Reclaimable":"1.5GB (90%)","Size":"1.6GB","TotalCount":"3","Type":"Local Volumes"}
{"Active":"0","Reclaimable":"3.4GB","Size":"3.4GB","TotalCount":"80","Type":"Build Cache"}
"#;
    const DF_V_OUT: &str = r#"{"Images":[],"Containers":[],"Volumes":[{"Name":"pgdata","Links":"0","Size":"1.2GB"},{"Name":"inuse","Links":"1","Size":"9GB"}],"BuildCache":[]}"#;

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
        let titles: Vec<_> = found.iter().map(|f| f.title.as_str()).collect();
        found.iter().find(|f| f.title == title).unwrap_or_else(|| panic!("no {title:?} in {titles:?}"))
    }

    #[test]
    fn parses_docker_sizes() {
        assert_eq!(parse_docker_size("0B"), Some(0));
        assert_eq!(parse_docker_size("1.5kB"), Some(1_500));
        assert_eq!(parse_docker_size("512MB"), Some(512_000_000));
        assert_eq!(parse_docker_size("2.3GB (45%)"), Some(2_300_000_000));
        assert_eq!(parse_docker_size("1.25TB"), Some(1_250_000_000_000));
        assert_eq!(parse_docker_size("lots"), None);
    }

    #[test]
    fn docker_reclaimable_space_becomes_findings() {
        let sys = FakeSys::default().respond(DF, Ok(DF_OUT)).respond(DF_V, Ok(DF_V_OUT)).respond(DANGLING, Ok("pgdata\n"));

        let (found, notices) = docker_findings(&sys);

        assert!(notices.is_empty());
        let cache = find(&found, "Docker build cache");
        assert_eq!((cache.size_bytes, cache.risk), (3_400_000_000, Risk::Safe));
        assert_eq!(cache.action, Action::DockerPrune { target: DockerTarget::BuildCache });
        let images = find(&found, "Unused Docker images");
        assert_eq!((images.size_bytes, images.risk), (2_100_000_000, Risk::Review));
        assert!(found.iter().all(|f| f.title != "Stopped Docker containers"), "0 B is skipped");
        let vol = find(&found, "Unused Docker volume: pgdata");
        assert_eq!((vol.size_bytes, vol.risk), (1_200_000_000, Risk::Caution));
        assert_eq!(vol.action, Action::DockerRemoveVolume { name: "pgdata".into() });
        assert!(found.iter().all(|f| f.category == Category::DevTools));
    }

    #[test]
    fn docker_not_running_gives_a_notice_only() {
        let sys = FakeSys::default().respond(DF, Err("failed to connect to the docker API"));
        let (found, notices) = docker_findings(&sys);
        assert!(found.is_empty());
        assert_eq!(notices.len(), 1);
        assert!(notices[0].contains("Docker"));
    }

    #[test]
    fn wsl_disks_are_found_and_named() {
        let dir = tempfile::tempdir().unwrap();
        let p = fake_paths(dir.path());
        let gb = 1024 * 1024 * 1024;
        let big = |path: PathBuf| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::File::create(&path).unwrap().set_len(gb).unwrap(); // sparse, no real disk use
        };
        big(p.local_app_data.join("Packages/CanonicalGroupLimited.Ubuntu_79rhkp1fndgsc/LocalState/ext4.vhdx"));
        big(p.local_app_data.join("Docker/wsl/disk/docker_data.vhdx"));
        file(&p.local_app_data.join("Docker/wsl/main/ext4.vhdx"), 10, 0); // too small to bother

        let found = wsl_disk_findings(&p);

        assert_eq!(found.len(), 2);
        let ubuntu = find(&found, "Compact WSL disk: Ubuntu");
        assert!(ubuntu.upper_bound && ubuntu.requires_admin);
        assert_eq!(ubuntu.size_bytes, gb);
        assert!(matches!(&ubuntu.action, Action::CompactDisk { path } if path.ends_with("ext4.vhdx")));
        find(&found, "Compact WSL disk: Docker Desktop data");
    }

    #[test]
    fn rust_toolchains_except_default_and_active_are_found() {
        let dir = tempfile::tempdir().unwrap();
        let p = fake_paths(dir.path());
        let tc = p.home.join(".rustup/toolchains");
        file(&tc.join("1.85-x86_64-pc-windows-msvc/bin/rustc.exe"), 300, 0);
        file(&tc.join("stable-x86_64-pc-windows-msvc/bin/rustc.exe"), 300, 0);
        file(&tc.join("nightly-2026-10-01-x86_64-pc-windows-msvc/bin/rustc.exe"), 100, 0);
        let list = "stable-x86_64-pc-windows-msvc (active, default)\n1.85-x86_64-pc-windows-msvc\nnightly-2026-10-01-x86_64-pc-windows-msvc (active)\n";
        let sys = FakeSys::default().respond("rustup toolchain list", Ok(list));

        let found = toolchain_findings(&p, &sys);

        assert_eq!(found.len(), 1);
        let f = find(&found, "Rust toolchain 1.85-x86_64-pc-windows-msvc");
        assert_eq!((f.size_bytes, f.risk), (300, Risk::Review));
        assert_eq!(f.action, Action::RemoveToolchain { name: "1.85-x86_64-pc-windows-msvc".into() });
    }

    #[test]
    fn no_rustup_means_no_toolchain_findings() {
        let dir = tempfile::tempdir().unwrap();
        assert!(toolchain_findings(&fake_paths(dir.path()), &FakeSys::default()).is_empty());
    }

    #[test]
    fn android_emulators_are_found_with_their_ini() {
        let dir = tempfile::tempdir().unwrap();
        let p = fake_paths(dir.path());
        let avd = p.home.join(".android/avd");
        file(&avd.join("Pixel_8.avd/userdata-qemu.img"), 500, 72);
        file(&avd.join("Pixel_8.ini"), 5, 72);

        let found = avd_findings(&p, SystemTime::now());

        let f = find(&found, "Android emulator: Pixel_8");
        assert_eq!((f.size_bytes, f.risk), (500, Risk::Caution));
        assert_eq!(f.action, Action::DeletePaths { paths: vec![avd.join("Pixel_8.avd"), avd.join("Pixel_8.ini")] });
        assert!(f.reason.contains("3 days ago"), "{}", f.reason);
    }

    #[test]
    fn scan_combines_everything_and_sorts_by_score() {
        let dir = tempfile::tempdir().unwrap();
        let p = fake_paths(dir.path());
        file(&p.home.join(".android/avd/a.avd/x.img"), 10, 0);
        let sys = FakeSys::default().respond(DF, Ok(DF_OUT)).respond(DF_V, Ok(DF_V_OUT)).respond(DANGLING, Ok(""));

        let scan = scan_devtools(&p, &sys, SystemTime::now());

        assert!(scan.notices.is_empty());
        assert_eq!(scan.findings[0].title, "Docker build cache");
        let scores: Vec<u8> = scan.findings.iter().map(|f| f.score).collect();
        let mut sorted = scores.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(scores, sorted);
        find(&scan.findings, "Android emulator: a");
    }

    const LXSS: &str = "reg query HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Lxss /s";
    const LXSS_OUT: &str = "\r
HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Lxss\r
    DefaultDistribution    REG_SZ    {cf6264aa}\r
\r
HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Lxss\\{c3091a87}\r
    DistributionName    REG_SZ    docker-desktop\r
    BasePath    REG_SZ    \\\\?\\C:\\Users\\me\\AppData\\Local\\Docker\\wsl\\main\r
    VhdFileName    REG_SZ    ext4.vhdx\r
\r
HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Lxss\\{cf6264aa}\r
    DistributionName    REG_SZ    Ubuntu\r
    BasePath    REG_SZ    C:\\Users\\me\\My Distros\\Ubuntu\r
    VhdFileName    REG_SZ    ext4.vhdx\r
";
    const DU_UBUNTU: &str = "wsl -d Ubuntu -u root -- sh -c du -sb /var/cache/apt/archives /var/log/journal /root/.cache /home/*/.cache 2>/dev/null; true";
    const DU_OUT: &str = "122947714\t/var/cache/apt/archives\n838860800\t/var/log/journal\n0\t/root/.cache\n19771815841\t/home/mark/.cache\n";

    #[test]
    fn parses_wsl_distros_from_the_registry() {
        assert_eq!(
            parse_lxss(LXSS_OUT),
            vec![
                Distro { name: "docker-desktop".into(), vhdx: PathBuf::from("C:\\Users\\me\\AppData\\Local\\Docker\\wsl\\main\\ext4.vhdx") },
                Distro { name: "Ubuntu".into(), vhdx: PathBuf::from("C:\\Users\\me\\My Distros\\Ubuntu\\ext4.vhdx") },
            ]
        );
    }

    #[test]
    fn user_distros_are_cleaned_inside_but_never_removed() {
        let sys = FakeSys::default().respond(LXSS, Ok(LXSS_OUT)).respond(DU_UBUNTU, Ok(DU_OUT));

        let found = distro_findings(&sys);

        assert_eq!(found.len(), 3);
        assert!(found.iter().all(|f| matches!(f.action, Action::WslClean { .. })));
        assert!(!sys.calls().iter().any(|c| c.contains("docker-desktop")), "Docker's own distros are never touched");

        let apt = find(&found, "apt package cache (Ubuntu)");
        assert_eq!((apt.size_bytes, apt.risk), (122_947_714, Risk::Safe));
        assert_eq!(apt.action, Action::WslClean { distro: "Ubuntu".into(), task: WslTask::AptCache });
        let caches = find(&found, "User caches in ~/.cache (Ubuntu)");
        assert_eq!((caches.size_bytes, caches.risk), (19_771_815_841, Risk::Review));
        let logs = find(&found, "System logs older than 7 days (Ubuntu)");
        assert!(logs.upper_bound);
        assert!(apt.reason.contains("Compact the distro's disk"), "{}", apt.reason);
    }

    #[test]
    fn distro_that_fails_to_start_is_skipped() {
        let sys = FakeSys::default().respond(LXSS, Ok(LXSS_OUT)).respond(DU_UBUNTU, Err("distro failed to start"));
        assert!(distro_findings(&sys).is_empty());
    }

    #[test]
    fn no_wsl_means_no_distro_findings() {
        assert!(distro_findings(&FakeSys::default()).is_empty());
    }

    /// Read-only: prints what the scan finds on this machine. Cleans nothing.
    /// Run with: cargo test --lib devtools_this_machine -- --ignored --nocapture
    #[test]
    #[ignore]
    fn devtools_this_machine() {
        let p = KnownPaths::from_env().unwrap();
        let scan = scan_devtools(&p, &crate::sys::WindowsSys, SystemTime::now());
        for n in &scan.notices {
            println!("notice: {n}");
        }
        for f in &scan.findings {
            println!("{:>10}  {:?}  {}", format_bytes(f.size_bytes), f.risk, f.title);
        }
    }
}
