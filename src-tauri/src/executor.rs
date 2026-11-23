use crate::fsutil::{candidates, old_dirs, unix_secs};
use crate::journal::{Journal, JournalEntry};
use crate::model::{Action, Finding};
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


impl Executor<'_> {
    pub fn run(&self, findings: &[Finding], dry_run: bool) -> ExecReport {
        let items: Vec<ItemResult> = findings
            .iter()
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
        }
        r
    }

    fn record(&self, f: &Finding, r: &ItemResult, dry_run: bool) {
        let action_kind = match f.action {
            Action::DeleteContents { .. } => "deleteContents",
            Action::EmptyRecycleBin => "emptyRecycleBin",
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
    use crate::model::{Category, NameFilter, Risk};
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct FakeSys {
        bytes: u64,
        emptied: AtomicBool,
    }
    impl SystemOps for FakeSys {
        fn recycle_bin_info(&self) -> (u64, u64) { (self.bytes, 1) }
        fn empty_recycle_bin(&self) -> Result<(), String> {
            self.emptied.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    fn fake(bytes: u64) -> FakeSys { FakeSys { bytes, emptied: AtomicBool::new(false) } }

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
        assert!(!sys.emptied.load(Ordering::SeqCst));
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
        assert!(sys.emptied.load(Ordering::SeqCst));
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
