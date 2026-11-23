use crate::model::NameFilter;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub path: PathBuf,
    pub size: u64,
    pub modified: SystemTime,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stats {
    pub bytes: u64,
    pub files: u64,
    pub newest: Option<SystemTime>,
}

fn is_old(modified: SystemTime, min_age: Duration, now: SystemTime) -> bool {
    now.duration_since(modified).unwrap_or(Duration::ZERO) >= min_age
}

/// Files under `root` (recursive, symlinks and junctions not followed) that match `filter`
/// and are at least `min_age` old. Unreadable entries are skipped.
pub fn candidates(root: &Path, filter: &NameFilter, min_age: Duration, now: SystemTime) -> Vec<Candidate> {
    WalkDir::new(root)
        .min_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| filter.matches(&e.file_name().to_string_lossy()))
        .filter_map(|e| {
            let md = e.metadata().ok()?;
            let modified = md.modified().ok()?;
            is_old(modified, min_age, now).then(|| Candidate { path: e.into_path(), size: md.len(), modified })
        })
        .collect()
}

/// Same selection as `candidates`, but only totals it, walking subfolders in parallel.
/// Caches like uv or the pnpm store hold hundreds of thousands of files, so a single-threaded
/// walk of one of them dominates the whole scan.
pub fn summarize(root: &Path, filter: &NameFilter, min_age: Duration, now: SystemTime) -> Stats {
    use rayon::prelude::*;

    let Ok(entries) = std::fs::read_dir(root) else { return Stats::default() };
    let mut here = Stats::default();
    let mut subdirs = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let Ok(file_type) = entry.file_type() else { continue };
        if file_type.is_symlink() {
            continue; // symlinks and junctions are never followed or counted
        }
        if file_type.is_dir() {
            subdirs.push(entry.path());
            continue;
        }
        if !file_type.is_file() || !filter.matches(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let Ok(md) = entry.metadata() else { continue };
        let Ok(modified) = md.modified() else { continue };
        if is_old(modified, min_age, now) {
            here = merge(here, Stats { bytes: md.len(), files: 1, newest: Some(modified) });
        }
    }
    let below = subdirs.par_iter().map(|d| summarize(d, filter, min_age, now)).reduce(Stats::default, merge);
    merge(here, below)
}

fn merge(a: Stats, b: Stats) -> Stats {
    Stats {
        bytes: a.bytes + b.bytes,
        files: a.files + b.files,
        newest: match (a.newest, b.newest) {
            (Some(x), Some(y)) => Some(x.max(y)),
            (x, y) => x.or(y),
        },
    }
}

/// Directories under `root` (excluding `root`) last modified at least `min_age` ago,
/// children before parents, so removing them in order works bottom-up.
pub fn old_dirs(root: &Path, min_age: Duration, now: SystemTime) -> Vec<PathBuf> {
    WalkDir::new(root)
        .min_depth(1)
        .follow_links(false)
        .contents_first(true)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_dir())
        .filter(|e| e.metadata().ok().and_then(|m| m.modified().ok()).is_some_and(|m| is_old(m, min_age, now)))
        .map(|e| e.into_path())
        .collect()
}
pub fn unix_secs(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

#[cfg(test)]
pub(crate) mod test_support {
    use filetime::{set_file_mtime, FileTime};
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    /// Write `bytes` bytes to `path` (creating parents) and set its mtime `hours_ago`.
    pub fn file(path: &Path, bytes: usize, hours_ago: u64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![b'x'; bytes]).unwrap();
        age(path, hours_ago);
    }

    pub fn age(path: &Path, hours_ago: u64) {
        let t = SystemTime::now() - Duration::from_secs(hours_ago * 3600);
        set_file_mtime(path, FileTime::from_system_time(t)).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{age, file};
    use super::*;

    const DAY: Duration = Duration::from_secs(24 * 3600);

    #[test]
    fn candidates_respect_min_age_and_recurse() {
        let dir = tempfile::tempdir().unwrap();
        file(&dir.path().join("old.tmp"), 10, 48);
        file(&dir.path().join("sub/deep/old2.tmp"), 20, 72);
        file(&dir.path().join("new.tmp"), 30, 1);

        let mut found = candidates(dir.path(), &NameFilter::any(), DAY, SystemTime::now());
        found.sort_by(|a, b| a.path.cmp(&b.path));
        let names: Vec<_> = found.iter().map(|c| c.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["old.tmp", "old2.tmp"]);
    }

    #[test]
    fn candidates_apply_name_filter() {
        let dir = tempfile::tempdir().unwrap();
        file(&dir.path().join("thumbcache_32.db"), 5, 0);
        file(&dir.path().join("iconcache_32.db"), 5, 0);
        let filter = NameFilter { prefix: Some("thumbcache_".into()), suffix: Some(".db".into()) };
        let found = candidates(dir.path(), &filter, Duration::ZERO, SystemTime::now());
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn summarize_totals_bytes_and_newest() {
        let dir = tempfile::tempdir().unwrap();
        file(&dir.path().join("a"), 100, 48);
        file(&dir.path().join("b"), 50, 30);
        let s = summarize(dir.path(), &NameFilter::any(), DAY, SystemTime::now());
        assert_eq!(s.bytes, 150);
        assert_eq!(s.files, 2);
        let newest_age = SystemTime::now().duration_since(s.newest.unwrap()).unwrap();
        assert!(newest_age < Duration::from_secs(31 * 3600) && newest_age > Duration::from_secs(29 * 3600));
    }

    #[test]
    fn summarize_matches_candidates_on_nested_filtered_tree() {
        let dir = tempfile::tempdir().unwrap();
        for (i, path) in ["a/x.db", "a/b/c/thumbcache_1.db", "thumbcache_2.db", "a/b/thumbcache_3.tmp", "d/e/f/g/thumbcache_4.db"]
            .iter()
            .enumerate()
        {
            file(&dir.path().join(path), 10 * (i + 1), 48);
        }
        file(&dir.path().join("d/thumbcache_new.db"), 999, 1);
        let filter = NameFilter { prefix: Some("thumbcache_".into()), suffix: Some(".db".into()) };
        let now = SystemTime::now();

        let s = summarize(dir.path(), &filter, DAY, now);
        let c = candidates(dir.path(), &filter, DAY, now);
        assert_eq!(s.files, c.len() as u64);
        assert_eq!(s.bytes, c.iter().map(|c| c.size).sum::<u64>());
        assert_eq!(s.files, 3);
    }

    #[test]
    fn summarize_missing_root_is_empty() {
        let s = summarize(Path::new("C:\\definitely\\not\\here"), &NameFilter::any(), DAY, SystemTime::now());
        assert_eq!(s, Stats::default());
    }

    #[test]
    fn old_dirs_lists_deepest_first_and_skips_new_and_root() {
        let dir = tempfile::tempdir().unwrap();
        let old_parent = dir.path().join("p");
        let old_child = old_parent.join("c");
        let new_dir = dir.path().join("fresh");
        std::fs::create_dir_all(&old_child).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();
        age(&old_child, 48);
        age(&old_parent, 48);

        let dirs = old_dirs(dir.path(), DAY, SystemTime::now());
        assert_eq!(dirs, vec![old_child, old_parent]);
    }
}
