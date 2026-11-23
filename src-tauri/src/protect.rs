use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Denied {
    NotAbsolute,
    Unresolvable,
    Protected(String),
}

impl fmt::Display for Denied {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Denied::NotAbsolute => write!(f, "path is not absolute"),
            Denied::Unresolvable => write!(f, "path could not be resolved"),
            Denied::Protected(p) => write!(f, "protected location {p}"),
        }
    }
}

pub struct ProtectedSet {
    exact: Vec<String>,
    tree: Vec<String>,
}

fn norm(p: &Path) -> String {
    let resolved = dunce::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    resolved.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// true when `child` equals `parent` or lies inside it.
fn is_within(child: &str, parent: &str) -> bool {
    child == parent || child.starts_with(&format!("{parent}\\"))
}

impl ProtectedSet {
    pub fn new(exact: Vec<PathBuf>, tree: Vec<PathBuf>) -> Self {
        Self {
            exact: exact.iter().map(|p| norm(p)).collect(),
            tree: tree.iter().map(|p| norm(p)).collect(),
        }
    }

    pub fn from_system() -> Self {
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
        let windows = env("SystemRoot").unwrap_or_else(|| PathBuf::from("C:\\Windows"));
        let drive = env("SystemDrive").map(|d| d.join("\\")).unwrap_or_else(|| PathBuf::from("C:\\"));
        let profile = env("USERPROFILE");

        let mut exact = vec![drive.clone(), windows.clone(), drive.join("Users")];
        exact.extend(["ProgramFiles", "ProgramFiles(x86)", "ProgramData", "LOCALAPPDATA", "APPDATA"].iter().filter_map(|k| env(k)));
        exact.extend(profile.clone());

        let mut tree: Vec<PathBuf> = ["System32", "SysWOW64", "WinSxS", "servicing", "Boot", "Fonts"]
            .iter()
            .map(|s| windows.join(s))
            .collect();
        if let Some(p) = &profile {
            tree.extend(["Documents", "Desktop", "Pictures", "Videos", "Music", "OneDrive"].iter().map(|s| p.join(s)));
        }
        Self::new(exact, tree)
    }

    pub fn check(&self, target: &Path) -> Result<(), Denied> {
        if !target.is_absolute() {
            return Err(Denied::NotAbsolute);
        }
        let resolved = dunce::canonicalize(target).map_err(|_| Denied::Unresolvable)?;
        let t = resolved.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
        if let Some(e) = self.exact.iter().find(|e| is_within(e, &t)) {
            return Err(Denied::Protected(e.clone()));
        }
        if let Some(r) = self.tree.iter().find(|r| is_within(&t, r)) {
            return Err(Denied::Protected(r.clone()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, ProtectedSet) {
        let dir = tempfile::tempdir().unwrap();
        for sub in ["exact/inner", "tree/inner", "free"] {
            std::fs::create_dir_all(dir.path().join(sub)).unwrap();
        }
        std::fs::write(dir.path().join("exact/inner/f.txt"), "x").unwrap();
        std::fs::write(dir.path().join("tree/inner/f.txt"), "x").unwrap();
        std::fs::write(dir.path().join("free/f.txt"), "x").unwrap();
        let set = ProtectedSet::new(vec![dir.path().join("exact")], vec![dir.path().join("tree")]);
        (dir, set)
    }

    #[test]
    fn exact_root_and_its_ancestors_are_denied() {
        let (dir, set) = setup();
        assert!(matches!(set.check(&dir.path().join("exact")), Err(Denied::Protected(_))));
        assert!(matches!(set.check(dir.path()), Err(Denied::Protected(_))));
    }

    #[test]
    fn inside_exact_root_is_allowed() {
        let (dir, set) = setup();
        assert_eq!(set.check(&dir.path().join("exact/inner/f.txt")), Ok(()));
    }

    #[test]
    fn anything_in_tree_root_is_denied() {
        let (dir, set) = setup();
        assert!(matches!(set.check(&dir.path().join("tree")), Err(Denied::Protected(_))));
        assert!(matches!(set.check(&dir.path().join("tree/inner/f.txt")), Err(Denied::Protected(_))));
    }

    #[test]
    fn unrelated_paths_are_allowed() {
        let (dir, set) = setup();
        assert_eq!(set.check(&dir.path().join("free/f.txt")), Ok(()));
    }

    #[test]
    fn sibling_with_shared_prefix_is_not_confused() {
        let (dir, set) = setup();
        std::fs::create_dir_all(dir.path().join("treehouse")).unwrap();
        assert_eq!(set.check(&dir.path().join("treehouse")), Ok(()));
    }

    #[test]
    fn comparison_ignores_case() {
        let (dir, _) = setup();
        let upper = PathBuf::from(dir.path().join("tree").to_string_lossy().to_uppercase());
        let set = ProtectedSet::new(vec![], vec![upper]);
        assert!(matches!(set.check(&dir.path().join("tree/inner/f.txt")), Err(Denied::Protected(_))));
    }

    #[test]
    fn relative_and_missing_paths_are_denied() {
        let (_dir, set) = setup();
        assert_eq!(set.check(Path::new("relative\\x")), Err(Denied::NotAbsolute));
        assert_eq!(set.check(Path::new("C:\\no\\such\\path\\here")), Err(Denied::Unresolvable));
    }

    #[test]
    fn system_set_protects_windows_and_documents_but_not_temp() {
        let set = ProtectedSet::from_system();
        let win = std::env::var("SystemRoot").unwrap();
        assert!(set.check(Path::new(&win)).is_err());
        assert!(set.check(&Path::new(&win).join("System32\\kernel32.dll")).is_err());
        assert!(set.check(Path::new("C:\\")).is_err());
        let docs = PathBuf::from(std::env::var("USERPROFILE").unwrap()).join("Documents");
        if docs.exists() {
            assert!(set.check(&docs).is_err());
        }
        assert_eq!(set.check(&std::env::temp_dir()), Ok(()));
    }
}
