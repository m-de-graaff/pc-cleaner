use crate::fsutil::{candidates, old_dirs, unix_secs};
use crate::journal::{Journal, JournalEntry};
use crate::model::{Action, DockerTarget, Finding, WslTask};
use crate::protect::ProtectedSet;
use crate::sys::SystemOps;
use serde::Serialize;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemResult {
    pub finding_id: String,
    pub freed_bytes: u64,
    pub deleted_count: u64,
    pub failed_count: u64,
    pub errors: Vec<String>,
    pub skipped: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecReport {
    pub items: Vec<ItemResult>,
    pub total_freed: u64,
    pub dry_run: bool,
}

pub struct Executor<'a> {
    pub protect: &'a ProtectedSet,
    pub journal: &'a Journal,
    pub sys: &'a dyn SystemOps,
    pub elevated: bool,
    pub now: SystemTime,
}

const MAX_ERRORS: usize = 5;

pub fn compact_script_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("pc-cleaner-compact-{}.txt", std::process::id()))
}

/// diskpart script that compacts a virtual disk. Attaching read-only is enough for compaction
/// and guarantees diskpart never writes the filesystem inside it.
pub fn compact_script(disk: &std::path::Path) -> String {
    format!("select vdisk file=\"{}\"\r\nattach vdisk readonly\r\ncompact vdisk\r\ndetach vdisk\r\n", disk.display())
}

/// "Total reclaimed space: 1.5GB" from docker prune output.
fn reclaimed_from_docker(output: &str) -> Option<u64> {
    output
        .lines()
        .find_map(|l| l.trim().strip_prefix("Total reclaimed space:"))
        .and_then(|s| crate::scanners::devtools::parse_docker_size(s.trim()))
}

fn path_size(path: &std::path::Path) -> u64 {
    match std::fs::symlink_metadata(path) {
        Ok(md) if md.is_dir() => {
            crate::fsutil::summarize(path, &crate::model::NameFilter::any(), Duration::ZERO, SystemTime::now()).bytes
        }
        Ok(md) => md.len(),
        Err(_) => 0,
    }
}


impl Executor<'_> {
    pub fn run(&self, findings: &[Finding], dry_run: bool) -> ExecReport {
        // Compaction goes last so it returns the space freed by every other cleanup in the run,
        // including Docker prunes and cleanups inside Linux.
        let mut ordered: Vec<&Finding> = findings.iter().collect();
        ordered.sort_by_key(|f| matches!(f.action, Action::CompactDisk { .. }));
        let items: Vec<ItemResult> = ordered
            .into_iter()
            .map(|f| {
                let result = self.run_one(f, dry_run);
                self.record(f, &result, dry_run);
                result
            })
            .collect();
        let total_freed = items.iter().map(|i| i.freed_bytes).sum();
        ExecReport { items, total_freed, dry_run }
    }

    fn run_one(&self, f: &Finding, dry_run: bool) -> ItemResult {
        let mut r = ItemResult { finding_id: f.id.clone(), ..Default::default() };
        if f.requires_admin && !self.elevated {
            r.skipped = Some("Needs administrator rights".into());
            return r;
        }
        match &f.action {
            Action::DeleteContents { root, filter, min_age_hours } => {
                if let Err(d) = self.protect.check(root) {
                    r.skipped = Some(format!("Blocked by safety guard: {d}"));
                    return r;
                }
                let min_age = Duration::from_secs(min_age_hours * 3600);
                let dirs = old_dirs(root, min_age, self.now);
                for c in candidates(root, filter, min_age, self.now) {
                    if self.protect.check(&c.path).is_err() {
                        r.failed_count += 1;
                        continue;
                    }
                    if dry_run {
                        r.freed_bytes += c.size;
                        r.deleted_count += 1;
                        continue;
                    }
                    match std::fs::remove_file(&c.path) {
                        Ok(()) => {
                            r.freed_bytes += c.size;
                            r.deleted_count += 1;
                        }
                        Err(e) => {
                            r.failed_count += 1;
                            if r.errors.len() < MAX_ERRORS {
                                r.errors.push(format!("{}: {e}", c.path.display()));
                            }
                        }
                    }
                }
                if !dry_run {
                    for d in dirs {
                        let _ = std::fs::remove_dir(&d); // only succeeds when empty
                    }
                }
            }
            Action::EmptyRecycleBin => {
                let (bytes, _) = self.sys.recycle_bin_info();
                if dry_run {
                    r.freed_bytes = bytes;
                    r.deleted_count = 1;
                } else {
                    match self.sys.empty_recycle_bin() {
                        Ok(()) => {
                            r.freed_bytes = bytes;
                            r.deleted_count = 1;
                        }
                        Err(e) => {
                            r.failed_count = 1;
                            r.errors.push(e);
                        }
                    }
                }
            }
            Action::DockerPrune { target } => {
                let args: &[&str] = match target {
                    DockerTarget::BuildCache => &["builder", "prune", "-f"],
                    DockerTarget::Images => &["image", "prune", "-a", "-f"],
                    DockerTarget::Containers => &["container", "prune", "-f"],
                };
                self.run_command(&mut r, f, dry_run, "docker", args);
            }
            Action::DockerRemoveVolume { name } => self.run_command(&mut r, f, dry_run, "docker", &["volume", "rm", name]),
            Action::RemoveToolchain { name } => {
                self.run_command(&mut r, f, dry_run, "rustup", &["toolchain", "uninstall", name])
            }
            Action::WslClean { distro, task } => {
                let script = match task {
                    WslTask::AptCache => "apt-get clean",
                    WslTask::UserCaches => "find /root/.cache /home/*/.cache -mindepth 1 -delete 2>/dev/null; true",
                    WslTask::Journal => "journalctl --vacuum-time=7d",
                };
                self.run_command(&mut r, f, dry_run, "wsl", &["-d", distro, "-u", "root", "--", "sh", "-c", script]);
            }
            Action::CompactDisk { path } => {
                if !path.exists() {
                    r.skipped = Some("The disk no longer exists".into());
                    return r;
                }
                if dry_run {
                    r.deleted_count = 1; // the size is only a maximum, so nothing is counted as freed
                    return r;
                }
                let before = path_size(path);
                if let Err(e) = self.compact(path) {
                    r.failed_count = 1;
                    r.errors.push(e);
                    return r;
                }
                r.freed_bytes = before.saturating_sub(path_size(path));
                r.deleted_count = 1;
            }
            Action::DeletePaths { paths } => {
                for path in paths {
                    if let Err(d) = self.protect.check(path) {
                        r.failed_count += 1;
                        r.errors.push(format!("{}: blocked by safety guard: {d}", path.display()));
                        continue;
                    }
                    let size = path_size(path);
                    let result = if dry_run {
                        Ok(())
                    } else if path.is_dir() {
                        std::fs::remove_dir_all(path)
                    } else {
                        std::fs::remove_file(path)
                    };
                    match result {
                        Ok(()) => {
                            r.freed_bytes += size;
                            r.deleted_count += 1;
                        }
                        Err(e) => {
                            r.failed_count += 1;
                            r.errors.push(format!("{}: {e}", path.display()));
                        }
                    }
                }
            }
        }
        r
    }

    /// Run a tool for a finding. Freed space comes from docker's own report when present,
    /// otherwise from the scan's size.
    fn run_command(&self, r: &mut ItemResult, f: &Finding, dry_run: bool, program: &str, args: &[&str]) {
        let expected = if f.upper_bound { 0 } else { f.size_bytes };
        if dry_run {
            r.freed_bytes = expected;
            r.deleted_count = 1;
            return;
        }
        match self.sys.run(program, args) {
            Ok(out) => {
                r.freed_bytes = reclaimed_from_docker(&out).unwrap_or(expected);
                r.deleted_count = 1;
            }
            Err(e) => {
                r.failed_count = 1;
                r.errors.push(e);
            }
        }
    }

    fn compact(&self, disk: &std::path::Path) -> Result<(), String> {
        self.protect.check(disk).map_err(|d| format!("Blocked by safety guard: {d}"))?;
        self.sys.run("wsl", &["--shutdown"])?;
        let script = compact_script_path();
        std::fs::write(&script, compact_script(disk)).map_err(|e| e.to_string())?;
        let result = self.sys.run("diskpart", &["/s", &script.to_string_lossy()]);
        let _ = std::fs::remove_file(&script);
        result.map(|_| ())
    }

    fn record(&self, f: &Finding, r: &ItemResult, dry_run: bool) {
        let action_kind = match f.action {
            Action::DeleteContents { .. } => "deleteContents",
            Action::EmptyRecycleBin => "emptyRecycleBin",
            Action::DockerPrune { .. } => "dockerPrune",
            Action::DockerRemoveVolume { .. } => "dockerRemoveVolume",
            Action::CompactDisk { .. } => "compactDisk",
            Action::RemoveToolchain { .. } => "removeToolchain",
            Action::WslClean { .. } => "wslClean",
            Action::DeletePaths { .. } => "deletePaths",
        };
        let entry = JournalEntry {
            timestamp: unix_secs(self.now),
            finding_id: f.id.clone(),
            title: f.title.clone(),
            category: f.category,
            action_kind: action_kind.into(),
            freed_bytes: r.freed_bytes,
            deleted_count: r.deleted_count,
            failed_count: r.failed_count,
            dry_run,
            reversible: f.reversible,
        };
        if let Err(e) = self.journal.append(&entry) {
            eprintln!("journal write failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsutil::test_support::{age, file};
    use crate::model::{Category, DockerTarget, NameFilter, Risk, WslTask};
    use crate::sys::fake::FakeSys;
    use std::path::Path;

    fn fake(bytes: u64) -> FakeSys { FakeSys::with_bin(bytes, 1) }

    fn junk(root: &Path, min_age_hours: u64, requires_admin: bool) -> Finding {
        Finding {
            id: format!("junk:{}", root.display()),
            category: Category::Junk,
            title: "Test junk".into(),
            size_bytes: 0,
            item_count: 0,
            last_modified: None,
            risk: Risk::Safe,
            score: 70,
            reason: String::new(),
            requires_admin,
            reversible: false,
            upper_bound: false,
            action: Action::DeleteContents { root: root.to_path_buf(), filter: NameFilter::any(), min_age_hours },
        }
    }

    fn recycle() -> Finding {
        Finding { id: "junk:recycle-bin".into(), action: Action::EmptyRecycleBin, ..junk(Path::new("C:\\"), 0, false) }
    }

    struct Env {
        dir: tempfile::TempDir,
        journal: Journal,
        open: ProtectedSet,
    }
    fn env() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::new(dir.path().join("journal.jsonl"));
        Env { dir, journal, open: ProtectedSet::new(vec![], vec![]) }
    }
    fn exec<'a>(e: &'a Env, protect: &'a ProtectedSet, sys: &'a FakeSys, elevated: bool) -> Executor<'a> {
        Executor { protect, journal: &e.journal, sys, elevated, now: SystemTime::now() }
    }

    #[test]
    fn deletes_old_files_keeps_new_files_and_root() {
        let e = env();
        let root = e.dir.path().join("temp");
        file(&root.join("old.tmp"), 100, 48);
        file(&root.join("sub/old2.tmp"), 50, 48);
        file(&root.join("new.tmp"), 10, 1);
        age(&root.join("sub"), 48);
        let sys = fake(0);

        let report = exec(&e, &e.open, &sys, false).run(&[junk(&root, 24, false)], false);

        assert_eq!(report.total_freed, 150);
        assert_eq!(report.items[0].deleted_count, 2);
        assert!(!root.join("old.tmp").exists());
        assert!(!root.join("sub").exists(), "emptied old subfolder is removed");
        assert!(root.join("new.tmp").exists());
        assert!(root.exists());
    }

    fn dev(id: &str, size: u64, requires_admin: bool, action: Action) -> Finding {
        Finding { id: id.into(), category: Category::DevTools, size_bytes: size, requires_admin, action, ..junk(Path::new("C:\\"), 0, false) }
    }

    #[test]
    fn docker_prune_runs_docker_and_reads_reclaimed_space() {
        let e = env();
        let sys = fake(0).respond("docker builder prune -f", Ok("Deleted build cache\nTotal reclaimed space: 1.5GB\n"));
        let f = dev("d", 9, false, Action::DockerPrune { target: DockerTarget::BuildCache });

        let report = exec(&e, &e.open, &sys, false).run(&[f], false);

        assert_eq!(sys.calls(), vec!["docker builder prune -f"]);
        assert_eq!(report.total_freed, 1_500_000_000);
    }

    #[test]
    fn failed_command_is_reported_not_counted() {
        let e = env();
        let sys = fake(0).respond("docker image prune -a -f", Err("daemon not running"));
        let f = dev("d", 9, false, Action::DockerPrune { target: DockerTarget::Images });

        let report = exec(&e, &e.open, &sys, false).run(&[f], false);

        assert_eq!((report.items[0].failed_count, report.total_freed), (1, 0));
        assert_eq!(report.items[0].errors, vec!["daemon not running"]);
    }

    #[test]
    fn volume_and_toolchain_removal_run_their_commands() {
        let e = env();
        let sys = fake(0)
            .respond("docker volume rm pgdata", Ok("pgdata\n"))
            .respond("rustup toolchain uninstall 1.85-x86_64-pc-windows-msvc", Ok(""));
        let findings = [
            dev("v", 100, false, Action::DockerRemoveVolume { name: "pgdata".into() }),
            dev("t", 200, false, Action::RemoveToolchain { name: "1.85-x86_64-pc-windows-msvc".into() }),
        ];

        let report = exec(&e, &e.open, &sys, false).run(&findings, false);

        assert_eq!(report.total_freed, 300);
        assert_eq!(sys.calls(), vec!["docker volume rm pgdata", "rustup toolchain uninstall 1.85-x86_64-pc-windows-msvc"]);
    }

    #[test]
    fn compaction_shuts_down_wsl_then_runs_diskpart_last() {
        let e = env();
        let disk = e.dir.path().join("ext4.vhdx");
        std::fs::write(&disk, b"vhdx").unwrap();
        let script = compact_script_path();
        let diskpart = format!("diskpart /s {}", script.display());
        let sys = fake(0)
            .respond("wsl --shutdown", Ok(""))
            .respond(&diskpart, Ok("DiskPart successfully compacted the virtual disk file."))
            .respond("docker builder prune -f", Ok(""));
        let findings = [
            dev("c", 4, true, Action::CompactDisk { path: disk.clone() }),
            dev("d", 1, false, Action::DockerPrune { target: DockerTarget::BuildCache }),
        ];

        let report = exec(&e, &e.open, &sys, true).run(&findings, false);

        assert_eq!(sys.calls(), vec!["docker builder prune -f".to_string(), "wsl --shutdown".into(), diskpart]);
        assert_eq!(report.items[1].finding_id, "c");
        assert_eq!(report.items[1].failed_count, 0);
        assert!(!script.exists(), "temporary diskpart script is removed");
    }

    #[test]
    fn linux_cleanups_run_before_compaction() {
        let e = env();
        let disk = e.dir.path().join("ext4.vhdx");
        std::fs::write(&disk, b"vhdx").unwrap();
        let diskpart = format!("diskpart /s {}", compact_script_path().display());
        let apt = "wsl -d Ubuntu -u root -- sh -c apt-get clean";
        let caches = "wsl -d Ubuntu -u root -- sh -c find /root/.cache /home/*/.cache -mindepth 1 -delete 2>/dev/null; true";
        let journal = "wsl -d Ubuntu -u root -- sh -c journalctl --vacuum-time=7d";
        let sys = fake(0)
            .respond(apt, Ok(""))
            .respond(caches, Ok(""))
            .respond(journal, Ok(""))
            .respond("wsl --shutdown", Ok(""))
            .respond(&diskpart, Ok(""));
        let findings = [
            dev("c", 4, true, Action::CompactDisk { path: disk }),
            dev("a", 10, false, Action::WslClean { distro: "Ubuntu".into(), task: WslTask::AptCache }),
            dev("h", 20, false, Action::WslClean { distro: "Ubuntu".into(), task: WslTask::UserCaches }),
            Finding { upper_bound: true, ..dev("j", 30, false, Action::WslClean { distro: "Ubuntu".into(), task: WslTask::Journal }) },
        ];

        let report = exec(&e, &e.open, &sys, true).run(&findings, false);

        assert_eq!(sys.calls(), vec![apt, caches, journal, "wsl --shutdown", diskpart.as_str()]);
        assert_eq!(report.total_freed, 30, "journal size is only a maximum, so it isn't counted");
    }

    #[test]
    fn compaction_of_a_removed_disk_is_skipped() {
        let e = env();
        let sys = fake(0);
        let f = dev("c", 4, true, Action::CompactDisk { path: e.dir.path().join("gone.vhdx") });

        let report = exec(&e, &e.open, &sys, true).run(&[f], false);

        assert_eq!(report.items[0].skipped.as_deref(), Some("The disk no longer exists"));
        assert!(sys.calls().is_empty());
    }

    #[test]
    fn compact_script_selects_attaches_compacts_and_detaches() {
        let s = compact_script(Path::new("C:\\Users\\me\\ext4.vhdx"));
        assert_eq!(
            s,
            "select vdisk file=\"C:\\Users\\me\\ext4.vhdx\"\r\nattach vdisk readonly\r\ncompact vdisk\r\ndetach vdisk\r\n"
        );
    }

    #[test]
    fn dry_run_runs_no_commands() {
        let e = env();
        let sys = fake(0);
        let disk = e.dir.path().join("ext4.vhdx");
        std::fs::write(&disk, b"vhdx").unwrap();
        let findings = [
            dev("d", 10, false, Action::DockerPrune { target: DockerTarget::Images }),
            dev("v", 20, false, Action::DockerRemoveVolume { name: "x".into() }),
            dev("t", 30, false, Action::RemoveToolchain { name: "x".into() }),
            dev("c", 40, true, Action::CompactDisk { path: disk }),
        ];

        let report = exec(&e, &e.open, &sys, true).run(&findings, true);

        assert!(sys.calls().is_empty());
        assert_eq!(report.total_freed, 60, "compaction's size is only a maximum, so a dry run counts 0 for it");
    }

    #[test]
    fn delete_paths_removes_folder_and_file_but_respects_the_guard() {
        let e = env();
        let avd = e.dir.path().join("avd");
        file(&avd.join("Pixel.avd/userdata.img"), 70, 0);
        file(&avd.join("Pixel.ini"), 5, 0);
        file(&e.dir.path().join("Documents/keep.txt"), 1, 0);
        let guard = ProtectedSet::new(vec![], vec![e.dir.path().join("Documents")]);
        let f = dev("a", 75, false, Action::DeletePaths {
            paths: vec![avd.join("Pixel.avd"), avd.join("Pixel.ini"), e.dir.path().join("Documents/keep.txt")],
        });
        let sys = fake(0);

        let report = exec(&e, &guard, &sys, false).run(&[f], false);

        assert!(!avd.join("Pixel.avd").exists() && !avd.join("Pixel.ini").exists());
        assert!(e.dir.path().join("Documents/keep.txt").exists());
        assert_eq!((report.total_freed, report.items[0].deleted_count, report.items[0].failed_count), (75, 2, 1));
    }

    #[test]
    fn read_only_cache_files_are_deleted() {
        let e = env();
        let root = e.dir.path().join("cache");
        let f = root.join("pkg/readonly.js");
        file(&f, 40, 48);
        let mut perms = std::fs::metadata(&f).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&f, perms).unwrap();
        let sys = fake(0);

        let report = exec(&e, &e.open, &sys, false).run(&[junk(&root, 0, false)], false);

        assert_eq!(report.items[0].failed_count, 0, "{:?}", report.items[0].errors);
        assert_eq!(report.total_freed, 40);
        assert!(!f.exists());
    }

    #[test]
    fn dry_run_reports_but_deletes_nothing() {
        let e = env();
        let root = e.dir.path().join("temp");
        file(&root.join("old.tmp"), 100, 48);
        let sys = fake(500);

        let report = exec(&e, &e.open, &sys, false).run(&[junk(&root, 24, false), recycle()], true);

        assert!(report.dry_run);
        assert_eq!(report.total_freed, 600);
        assert!(root.join("old.tmp").exists());
        assert!(!sys.was_emptied());
    }

    #[test]
    fn admin_findings_are_skipped_when_not_elevated() {
        let e = env();
        let root = e.dir.path().join("wintemp");
        file(&root.join("old.tmp"), 100, 48);
        let sys = fake(0);

        let report = exec(&e, &e.open, &sys, false).run(&[junk(&root, 24, true)], false);

        assert_eq!(report.items[0].skipped.as_deref(), Some("Needs administrator rights"));
        assert!(root.join("old.tmp").exists());
    }

    #[test]
    fn protected_root_is_skipped_and_untouched() {
        let e = env();
        let root = e.dir.path().join("Documents");
        file(&root.join("thesis.docx"), 100, 48);
        let guard = ProtectedSet::new(vec![], vec![root.clone()]);
        let sys = fake(0);

        let report = exec(&e, &guard, &sys, false).run(&[junk(&root, 0, false)], false);

        assert!(report.items[0].skipped.as_deref().unwrap().starts_with("Blocked by safety guard"));
        assert!(root.join("thesis.docx").exists());
    }

    #[test]
    fn empties_recycle_bin_through_system_ops() {
        let e = env();
        let sys = fake(1234);
        let report = exec(&e, &e.open, &sys, false).run(&[recycle()], false);
        assert!(sys.was_emptied());
        assert_eq!(report.total_freed, 1234);
    }

    #[test]
    fn writes_one_journal_entry_per_finding() {
        let e = env();
        let root = e.dir.path().join("temp");
        file(&root.join("old.tmp"), 100, 48);
        let sys = fake(0);
        exec(&e, &e.open, &sys, false).run(&[junk(&root, 24, false), recycle()], true);

        let entries = e.journal.read_all().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].action_kind, "deleteContents");
        assert_eq!(entries[0].freed_bytes, 100);
        assert!(entries[0].dry_run);
        assert_eq!(entries[1].action_kind, "emptyRecycleBin");
    }
}
