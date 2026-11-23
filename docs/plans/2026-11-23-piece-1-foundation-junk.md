# Piece 1 — Foundation + Junk & Temp Files: Implementation Plan

> **To execute:** use the `executing-plans` skill. Steps use `- [ ]` for tracking.
> This plan is a hypothesis about work not yet done. Where reality disagrees, for example a
> scaffold flag or a crate API that changed, fix the step, note the deviation in the commit
> message, and carry on.

**Goal:** A Tauri desktop app that scans the Windows junk locations, shows ranked
recommendations with reasons, cleans the selected items through a guarded executor, and
records every action in a History screen.

**Architecture:**
- Rust backend:
  - Scanners produce `Finding`s.
  - The backend keeps the most recent scan in state.
  - The frontend sends back finding IDs, never paths.
  - An `Executor` re-checks every path against `ProtectedSet`, deletes files, and appends to a JSON Lines journal.
  - System calls (Recycle Bin, disk usage, elevation) sit behind a `SystemOps` trait, so the executor is testable.
- React frontend: talks to the backend only through `src/lib/api.ts`.

**Tech stack:** Tauri 2, Rust 1.97 (edition 2021), React 19, TypeScript, Vite, Tailwind CSS v4
(`@tailwindcss/vite`), lucide-react, Vitest. Rust crates: serde, serde_json, walkdir 2, dunce 1,
is_elevated 0.1, windows-sys 0.59. Dev-dependencies: tempfile 3, filetime 0.2.

Spec: [`docs/design/2026-11-23-pc-cleaner.md`](../design/2026-11-23-pc-cleaner.md)

## Global Constraints

- Windows 10/11 x64 only. No `cfg(not(windows))` code paths.
- The app makes no network calls.
- Only `executor.rs` deletes files or empties the Recycle Bin. Scanners are read-only.
- Every path the executor touches passes `ProtectedSet::check` first. A failed check means skip, never delete.
- The frontend never sends paths to the backend, only finding IDs from the latest scan.
- All Rust structs sent to the frontend use `#[serde(rename_all = "camelCase")]`. Enums use lowercase or camelCase tags exactly as written in Task 2.
- Junk is deleted permanently. Moving it to the Recycle Bin would free no space.
- App identifier `com.rosdal.pccleaner`, product name `PC Cleaner`, Rust lib name `pc_cleaner_lib`.
- Rust tests: `cargo test --manifest-path src-tauri/Cargo.toml`. Frontend tests: `npm test`.
- UI copy is plain English, second person, and has no exclamation marks.

## File map

| File | Responsibility |
|---|---|
| `src-tauri/src/model.rs` | `Finding`, `Risk`, `Category`, `Action`, `NameFilter`. Data only. |
| `src-tauri/src/scoring.rs` | `score(risk, size)` and `format_bytes` |
| `src-tauri/src/fsutil.rs` | `candidates()`, `summarize()`, `old_dirs()`, `unix_secs()`: shared file walking |
| `src-tauri/src/protect.rs` | `ProtectedSet` deny-list guard |
| `src-tauri/src/journal.rs` | `Journal` (append-only JSON Lines) and `JournalEntry` |
| `src-tauri/src/sys.rs` | `SystemOps` trait, `WindowsSys`, `drive_usage`, `relaunch_as_admin` |
| `src-tauri/src/executor.rs` | `Executor::run`: the only code that deletes |
| `src-tauri/src/scanners/mod.rs` | `pub mod junk;` |
| `src-tauri/src/scanners/junk.rs` | `KnownPaths`, `junk_locations`, `scan_location`, `recycle_bin_finding`, `scan_junk` |
| `src-tauri/src/commands.rs` | Tauri commands, `AppState`, `select_findings` |
| `src-tauri/src/lib.rs` | Module declarations and the Tauri builder |
| `src/lib/types.ts` | TypeScript mirrors of the Rust types |
| `src/lib/api.ts` | Typed `invoke` wrappers |
| `src/lib/format.ts` | `formatBytes` (same output as Rust) |
| `src/lib/selection.ts` | Selection and grouping logic for the review list |
| `src/lib/useJunkScan.ts` | Scan state hook |
| `src/components/*.tsx` | `Sidebar`, `AdminBanner`, `FindingRow`, `ConfirmDialog` |
| `src/pages/*.tsx` | `Dashboard`, `JunkPage`, `HistoryPage` |
| `src/App.tsx`, `src/main.tsx`, `src/index.css` | Shell |

---

### Task 1: Scaffold the Tauri app

**Files:**
- Create: the whole scaffold (`package.json`, `vite.config.ts`, `src/`, `src-tauri/`), and `.gitignore` (from the scaffold)
- Modify: `src-tauri/tauri.conf.json`, `vite.config.ts`, `src/main.tsx`, `src/App.tsx`
- Create: `src/index.css`

**Interfaces:**
- Produces: a buildable Tauri 2 + React-TS app whose Rust lib is named `pc_cleaner_lib`, with Tailwind v4 working.

- [ ] **Step 1: Scaffold.** Pass `--yes` so the scaffold doesn't prompt. **Never pass `--force` into a folder that has content.** In create-tauri-app 4.7.5, `--force` empties the target folder first, so it deletes `docs/`.

Run: `npm create tauri-app@latest . -- --template react-ts --manager npm --identifier com.rosdal.pccleaner --yes`

Run it in an empty folder, or scaffold into a sibling folder and move the files in. Commit `docs/` before scaffolding.
Expect: the files `package.json`, `src/App.tsx` and `src-tauri/Cargo.toml` exist.

- [ ] **Step 2: Install and check the lib name**

Run: `npm install`
Then open `src-tauri/Cargo.toml` and confirm `[lib] name = "pc_cleaner_lib"`, and that `src-tauri/src/main.rs` calls `pc_cleaner_lib::run()`. If the name differs, rename both to `pc_cleaner_lib`.

- [ ] **Step 3: Add the frontend dependencies**

Run: `npm install lucide-react`
Run: `npm install -D tailwindcss @tailwindcss/vite vitest`

Add `"test": "vitest run"` to `"scripts"` in `package.json`.

- [ ] **Step 4: Wire Tailwind.** In `vite.config.ts`, add the import and the plugin, and keep the scaffold's other settings unchanged:

```ts
import tailwindcss from "@tailwindcss/vite";
// ...
plugins: [react(), tailwindcss()],
```

Create `src/index.css`:

```css
@import "tailwindcss";

html, body, #root { height: 100%; }
body { margin: 0; font-family: "Segoe UI Variable", "Segoe UI", system-ui, sans-serif; }
```

Replace `src/main.tsx`:

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

Replace `src/App.tsx` with a temporary shell. Task 11 replaces it again.

```tsx
export default function App() {
  return <div className="flex h-screen items-center justify-center bg-zinc-950 text-zinc-100">PC Cleaner</div>;
}
```

Run: `Remove-Item src/App.css, src/assets -Recurse -Force`

- [ ] **Step 5: Configure the window.** In `src-tauri/tauri.conf.json`:
  - Set `"productName": "PC Cleaner"`.
  - Replace the window entry under `app.windows` with:

```json
{ "title": "PC Cleaner", "width": 1100, "height": 720, "minWidth": 900, "minHeight": 600 }
```

- [ ] **Step 6: Verify it builds and runs**

Run: `npm run build`. Expect: exit 0.
Run: `cargo check --manifest-path src-tauri/Cargo.toml`. Expect: exit 0.
Run: `npm run tauri dev`. Expect: a window titled "PC Cleaner" showing centred text on a dark background. Close it.

- [ ] **Step 7: Commit**

```powershell
git init
git add -A
git commit -m "Scaffold Tauri 2 + React app with Tailwind v4"
```

---

### Task 2: Data model and scoring

**Files:**
- Create: `src-tauri/src/model.rs`, `src-tauri/src/scoring.rs`
- Modify: `src-tauri/src/lib.rs`. Add `pub mod model;` and `pub mod scoring;` at the top.
- Test: inline `#[cfg(test)]` modules in both files

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `model::{Category, Risk, NameFilter, Action, Finding}`
  - `NameFilter::matches(&self, &str) -> bool`
  - `scoring::score(Risk, u64) -> u8`
  - `scoring::format_bytes(u64) -> String`

- [ ] **Step 1: Write the failing tests.** Create `src-tauri/src/scoring.rs` with only the tests and stub signatures:

```rust
use crate::model::Risk;

pub fn score(_risk: Risk, _size_bytes: u64) -> u8 { unimplemented!() }
pub fn format_bytes(_bytes: u64) -> String { unimplemented!() }

#[cfg(test)]
mod tests {
    use super::*;
    const MIB: u64 = 1024 * 1024;

    #[test]
    fn base_score_follows_risk() {
        assert_eq!(score(Risk::Safe, 0), 70);
        assert_eq!(score(Risk::Review, 0), 40);
        assert_eq!(score(Risk::Caution, 0), 10);
    }

    #[test]
    fn size_adds_up_to_thirty() {
        assert_eq!(score(Risk::Review, 10 * MIB), 50);
        assert_eq!(score(Risk::Safe, 1024 * MIB), 100);
        assert_eq!(score(Risk::Safe, 100_000 * MIB), 100);
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1024 * MIB), "1.0 GB");
    }
}
```

Create `src-tauri/src/model.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Junk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Safe,
    Review,
    Caution,
}

/// Case-insensitive file-name filter. `None` means "any".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NameFilter {
    pub prefix: Option<String>,
    pub suffix: Option<String>,
}

impl NameFilter {
    pub fn any() -> Self {
        Self::default()
    }

    pub fn matches(&self, file_name: &str) -> bool {
        let name = file_name.to_ascii_lowercase();
        self.prefix.as_ref().map_or(true, |p| name.starts_with(&p.to_ascii_lowercase()))
            && self.suffix.as_ref().map_or(true, |s| name.ends_with(&s.to_ascii_lowercase()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Action {
    /// Delete files under `root` (never `root` itself) that match `filter`
    /// and were last modified at least `min_age_hours` ago.
    #[serde(rename_all = "camelCase")]
    DeleteContents { root: PathBuf, filter: NameFilter, min_age_hours: u64 },
    EmptyRecycleBin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub category: Category,
    pub title: String,
    pub size_bytes: u64,
    pub item_count: u64,
    /// Unix seconds of the newest matching file, if known.
    pub last_modified: Option<i64>,
    pub risk: Risk,
    pub score: u8,
    pub reason: String,
    pub requires_admin: bool,
    pub reversible: bool,
    pub action: Action,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_filter_is_case_insensitive() {
        let f = NameFilter { prefix: Some("thumbcache_".into()), suffix: Some(".db".into()) };
        assert!(f.matches("THUMBCACHE_256.DB"));
        assert!(!f.matches("iconcache_256.db"));
        assert!(!f.matches("thumbcache_256.tmp"));
        assert!(NameFilter::any().matches("anything"));
    }

    #[test]
    fn action_serializes_with_camel_case_tag() {
        let a = Action::DeleteContents { root: "C:\\x".into(), filter: NameFilter::any(), min_age_hours: 24 };
        let json = serde_json::to_value(&a).unwrap();
        assert_eq!(json["kind"], "deleteContents");
        assert_eq!(json["minAgeHours"], 24);
        assert_eq!(serde_json::to_value(Action::EmptyRecycleBin).unwrap()["kind"], "emptyRecycleBin");
    }
}
```

- [ ] **Step 2: Run the tests and confirm the scoring tests fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml scoring`
Expect: the three scoring tests FAIL (panic: "not implemented"). The model tests pass.

- [ ] **Step 3: Implement scoring.** Replace the two stubs:

```rust
pub fn score(risk: Risk, size_bytes: u64) -> u8 {
    let base = match risk {
        Risk::Safe => 70.0,
        Risk::Review => 40.0,
        Risk::Caution => 10.0,
    };
    let mib = size_bytes as f64 / (1024.0 * 1024.0);
    // 10 MiB -> +10, 100 MiB -> +20, 1 GiB+ -> +30
    let bonus = if mib < 1.0 { 0.0 } else { (mib.log10() * 10.0).min(30.0) };
    (base + bonus).round() as u8
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml model scoring`
Expect: 5 passed.

- [ ] **Step 5: Commit** with `git commit -am "Add finding model and scoring"`. Run `git add` first for the new files.

---

### Task 3: File walking helpers

**Files:**
- Create: `src-tauri/src/fsutil.rs`
- Modify: `src-tauri/src/lib.rs`. Add `pub mod fsutil;`.
- Test: inline in `fsutil.rs`

**Interfaces:**
- Consumes: `model::NameFilter`.
- Produces:
  - `fsutil::Candidate { path: PathBuf, size: u64, modified: SystemTime }`
  - `fsutil::candidates(root: &Path, filter: &NameFilter, min_age: Duration, now: SystemTime) -> Vec<Candidate>`
  - `fsutil::Stats { bytes: u64, files: u64, newest: Option<SystemTime> }`
  - `fsutil::summarize(root, filter, min_age, now) -> Stats`
  - `fsutil::old_dirs(root, min_age, now) -> Vec<PathBuf>`: deepest first, `root` excluded
  - `fsutil::unix_secs(SystemTime) -> i64`

- [ ] **Step 1: Add the dependencies**

Run: `cargo add --manifest-path src-tauri/Cargo.toml walkdir@2`
Run: `cargo add --manifest-path src-tauri/Cargo.toml --dev tempfile@3 filetime@0.2`

- [ ] **Step 2: Write the failing tests.** Create `src-tauri/src/fsutil.rs`:

```rust
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

pub fn candidates(_root: &Path, _filter: &NameFilter, _min_age: Duration, _now: SystemTime) -> Vec<Candidate> { unimplemented!() }
pub fn summarize(_root: &Path, _filter: &NameFilter, _min_age: Duration, _now: SystemTime) -> Stats { unimplemented!() }
pub fn old_dirs(_root: &Path, _min_age: Duration, _now: SystemTime) -> Vec<PathBuf> { unimplemented!() }
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
```

- [ ] **Step 3: Run the tests and confirm they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml fsutil`
Expect: 5 FAIL with "not implemented".

- [ ] **Step 4: Implement.** Replace the three stubs:

```rust
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

pub fn summarize(root: &Path, filter: &NameFilter, min_age: Duration, now: SystemTime) -> Stats {
    candidates(root, filter, min_age, now).into_iter().fold(Stats::default(), |mut s, c| {
        s.bytes += c.size;
        s.files += 1;
        s.newest = Some(s.newest.map_or(c.modified, |n| n.max(c.modified)));
        s
    })
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
```

- [ ] **Step 5: Run the tests and confirm they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml fsutil`
Expect: 5 passed.

- [ ] **Step 6: Commit** with `"Add file walking helpers"`.

---

### Task 4: ProtectedSet safety guard

**Files:**
- Create: `src-tauri/src/protect.rs`
- Modify: `src-tauri/src/lib.rs`. Add `pub mod protect;`.
- Test: inline

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `protect::ProtectedSet::new(exact: Vec<PathBuf>, tree: Vec<PathBuf>) -> ProtectedSet`
  - `ProtectedSet::from_system() -> ProtectedSet`
  - `ProtectedSet::check(&self, &Path) -> Result<(), Denied>`
  - `protect::Denied { NotAbsolute, Unresolvable, Protected(String) }`, which implements `Display`

Semantics:
- `exact` roots may not be deleted, and neither may any of their ancestors. Paths *inside* them are allowed.
- `tree` roots may not have anything at or below them touched.
- The comparison is case-insensitive, made after canonicalising with `dunce`.

- [ ] **Step 1: Add the dependency**

Run: `cargo add --manifest-path src-tauri/Cargo.toml dunce@1`

- [ ] **Step 2: Write the failing tests.** Create `src-tauri/src/protect.rs`:

```rust
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

impl ProtectedSet {
    pub fn new(_exact: Vec<PathBuf>, _tree: Vec<PathBuf>) -> Self { unimplemented!() }
    pub fn from_system() -> Self { unimplemented!() }
    pub fn check(&self, _target: &Path) -> Result<(), Denied> { unimplemented!() }
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
```

- [ ] **Step 3: Run the tests and confirm they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml protect`
Expect: 8 FAIL with "not implemented".

- [ ] **Step 4: Implement.** Replace the `impl ProtectedSet` block and add the helpers:

```rust
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
```

- [ ] **Step 5: Run the tests and confirm they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml protect`
Expect: 8 passed.

- [ ] **Step 6: Commit** with `"Add ProtectedSet deny-list guard"`.

---

### Task 5: Journal

**Files:**
- Create: `src-tauri/src/journal.rs`
- Modify: `src-tauri/src/lib.rs`. Add `pub mod journal;`.
- Test: inline

**Interfaces:**
- Consumes: `model::Category`.
- Produces:
  - `journal::JournalEntry { timestamp: i64, finding_id: String, title: String, category: Category, action_kind: String, freed_bytes: u64, deleted_count: u64, failed_count: u64, dry_run: bool, reversible: bool }`
  - `journal::Journal::new(PathBuf) -> Journal`
  - `Journal::append(&self, &JournalEntry) -> std::io::Result<()>`
  - `Journal::read_all(&self) -> std::io::Result<Vec<JournalEntry>>`: oldest first, corrupt lines skipped

- [ ] **Step 1: Write the failing tests.** Create `src-tauri/src/journal.rs`:

```rust
use crate::model::Category;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntry {
    pub timestamp: i64,
    pub finding_id: String,
    pub title: String,
    pub category: Category,
    pub action_kind: String,
    pub freed_bytes: u64,
    pub deleted_count: u64,
    pub failed_count: u64,
    pub dry_run: bool,
    pub reversible: bool,
}

pub struct Journal {
    path: PathBuf,
}

impl Journal {
    pub fn new(path: PathBuf) -> Self { Self { path } }
    pub fn append(&self, _entry: &JournalEntry) -> std::io::Result<()> { unimplemented!() }
    pub fn read_all(&self) -> std::io::Result<Vec<JournalEntry>> { unimplemented!() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> JournalEntry {
        JournalEntry {
            timestamp: 1_700_000_000,
            finding_id: id.into(),
            title: "Your temporary files".into(),
            category: Category::Junk,
            action_kind: "deleteContents".into(),
            freed_bytes: 42,
            deleted_count: 3,
            failed_count: 1,
            dry_run: false,
            reversible: false,
        }
    }

    #[test]
    fn appends_and_reads_back_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let j = Journal::new(dir.path().join("nested/journal.jsonl"));
        j.append(&entry("a")).unwrap();
        j.append(&entry("b")).unwrap();
        let all = j.read_all().unwrap();
        assert_eq!(all, vec![entry("a"), entry("b")]);
    }

    #[test]
    fn missing_file_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Journal::new(dir.path().join("none.jsonl")).read_all().unwrap().is_empty());
    }

    #[test]
    fn corrupt_lines_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("j.jsonl");
        let j = Journal::new(path.clone());
        j.append(&entry("a")).unwrap();
        std::fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b"{not json\n").unwrap();
        j.append(&entry("b")).unwrap();
        assert_eq!(j.read_all().unwrap().len(), 2);
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml journal`
Expect: `appends_and_reads_back_in_order`, `missing_file_reads_as_empty` and `corrupt_lines_are_skipped` FAIL with "not implemented".

- [ ] **Step 3: Implement**

```rust
    pub fn append(&self, entry: &JournalEntry) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
        writeln!(f, "{line}")
    }

    pub fn read_all(&self) -> std::io::Result<Vec<JournalEntry>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let text = std::fs::read_to_string(&self.path)?;
        Ok(text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect())
    }
```

- [ ] **Step 4: Run the tests and confirm they pass.** Expect: 3 passed.

- [ ] **Step 5: Commit** with `"Add action journal"`.

---

### Task 6: SystemOps trait and Executor

**Files:**
- Create: `src-tauri/src/sys.rs` (trait only. Task 8 adds the Windows implementation.)
- Create: `src-tauri/src/executor.rs`
- Modify: `src-tauri/src/lib.rs`. Add `pub mod sys;` and `pub mod executor;`.
- Test: inline in `executor.rs`

**Interfaces:**
- Consumes:
  - `model::{Finding, Action, Category, Risk, NameFilter}`
  - `fsutil::{candidates, old_dirs, unix_secs}` and `fsutil::test_support::file`
  - `protect::ProtectedSet`
  - `journal::{Journal, JournalEntry}`
- Produces:
  - `sys::SystemOps` trait:
    - `fn recycle_bin_info(&self) -> (u64, u64)`, returning (bytes, items)
    - `fn empty_recycle_bin(&self) -> Result<(), String>`
  - `executor::ItemResult { finding_id: String, freed_bytes: u64, deleted_count: u64, failed_count: u64, errors: Vec<String>, skipped: Option<String> }`
  - `executor::ExecReport { items: Vec<ItemResult>, total_freed: u64, dry_run: bool }`
  - `executor::Executor<'a> { protect: &'a ProtectedSet, journal: &'a Journal, sys: &'a dyn SystemOps, elevated: bool, now: SystemTime }`
  - `Executor::run(&self, &[Finding], dry_run: bool) -> ExecReport`

- [ ] **Step 1: Create the trait.** Create `src-tauri/src/sys.rs`:

```rust
/// Everything the executor and scanners need from Windows, behind a trait so tests can fake it.
pub trait SystemOps: Send + Sync {
    /// (total bytes, item count) across all drives' Recycle Bins.
    fn recycle_bin_info(&self) -> (u64, u64);
    fn empty_recycle_bin(&self) -> Result<(), String>;
}
```

- [ ] **Step 2: Write the failing tests.** Create `src-tauri/src/executor.rs`:

```rust
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

impl Executor<'_> {
    pub fn run(&self, _findings: &[Finding], _dry_run: bool) -> ExecReport { unimplemented!() }
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
```

- [ ] **Step 3: Run the tests and confirm they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml executor`
Expect: 6 FAIL with "not implemented".

- [ ] **Step 4: Implement.** Replace `impl Executor<'_>`:

```rust
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
```

- [ ] **Step 5: Run the tests and confirm they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml executor`
Expect: 6 passed.

- [ ] **Step 6: Commit** with `"Add guarded executor and SystemOps trait"`.

---

### Task 7: Junk scanner

**Files:**
- Create: `src-tauri/src/scanners/mod.rs`, `src-tauri/src/scanners/junk.rs`
- Modify: `src-tauri/src/lib.rs`. Add `pub mod scanners;`.
- Test: inline in `junk.rs`

**Interfaces:**
- Consumes:
  - `model::*`
  - `scoring::{score, format_bytes}`
  - `fsutil::{summarize, unix_secs}` and `fsutil::test_support::file`
  - `sys::SystemOps`
- Produces:
  - `junk::KnownPaths { temp, windows, local_app_data, program_data: PathBuf }`
  - `KnownPaths::from_env() -> Option<KnownPaths>`
  - `junk::JunkLocation { title: String, root: PathBuf, filter: NameFilter, min_age_hours: u64, risk: Risk, requires_admin: bool, explanation: &'static str }`
  - `junk::junk_locations(&KnownPaths) -> Vec<JunkLocation>`
  - `junk::scan_location(&JunkLocation, SystemTime) -> Option<Finding>`
  - `junk::recycle_bin_finding(bytes: u64, items: u64) -> Option<Finding>`
  - `junk::scan_junk(&KnownPaths, &dyn SystemOps, SystemTime) -> Vec<Finding>`: sorted by score, highest first, then by title

- [ ] **Step 1: Write the failing tests.** Create `src-tauri/src/scanners/mod.rs` containing `pub mod junk;`. Then create `src-tauri/src/scanners/junk.rs`:

```rust
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

pub fn junk_locations(_p: &KnownPaths) -> Vec<JunkLocation> { unimplemented!() }
pub fn scan_location(_loc: &JunkLocation, _now: SystemTime) -> Option<Finding> { unimplemented!() }
pub fn recycle_bin_finding(_bytes: u64, _items: u64) -> Option<Finding> { unimplemented!() }
pub fn scan_junk(_p: &KnownPaths, _sys: &dyn SystemOps, _now: SystemTime) -> Vec<Finding> { unimplemented!() }

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
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml junk`
Expect: 7 FAIL with "not implemented".

- [ ] **Step 3: Implement.** Replace the four stubs:

```rust
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
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml junk`
Expect: 7 passed.

- [ ] **Step 5: Commit** with `"Add junk and temp file scanner"`.

---

### Task 8: Windows system access

**Files:**
- Modify: `src-tauri/src/sys.rs`. Add `WindowsSys`, `DriveUsage`, `drive_usage`, `is_elevated` and `relaunch_as_admin`.
- Test: inline. These run against the real machine and are read-only.

**Interfaces:**
- Consumes: the `sys::SystemOps` trait from Task 6.
- Produces:
  - `sys::WindowsSys` (a unit struct) that implements `SystemOps`
  - `sys::DriveUsage { root: String, total_bytes: u64, free_bytes: u64 }`, which implements `Serialize` (camelCase)
  - `sys::drive_usage(root: &str) -> Option<DriveUsage>`
  - `sys::is_elevated() -> bool`
  - `sys::relaunch_as_admin() -> Result<(), String>`

- [ ] **Step 1: Add the dependencies**

Run: `cargo add --manifest-path src-tauri/Cargo.toml windows-sys@0.59 --features Win32_Foundation,Win32_UI_Shell,Win32_Storage_FileSystem`
Run: `cargo add --manifest-path src-tauri/Cargo.toml is_elevated@0.1`

- [ ] **Step 2: Write the failing tests.** Append to `src-tauri/src/sys.rs`:

```rust
use serde::Serialize;

pub struct WindowsSys;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveUsage {
    pub root: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

impl SystemOps for WindowsSys {
    fn recycle_bin_info(&self) -> (u64, u64) { unimplemented!() }
    fn empty_recycle_bin(&self) -> Result<(), String> { unimplemented!() }
}

pub fn drive_usage(_root: &str) -> Option<DriveUsage> { unimplemented!() }
pub fn is_elevated() -> bool { unimplemented!() }
pub fn relaunch_as_admin() -> Result<(), String> { unimplemented!() }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_drive_usage_is_plausible() {
        let d = drive_usage("C:\\").expect("C: exists");
        assert!(d.total_bytes > 0);
        assert!(d.free_bytes <= d.total_bytes);
    }

    #[test]
    fn missing_drive_returns_none() {
        assert!(drive_usage("Q:\\definitely-not-a-drive\\").is_none());
    }

    #[test]
    fn recycle_bin_query_does_not_panic() {
        let (_bytes, _items) = WindowsSys.recycle_bin_info();
    }

    #[test]
    fn elevation_check_returns() {
        let _ = is_elevated();
    }
}
```

No test calls `empty_recycle_bin` or `relaunch_as_admin`, because both act on the real system. Task 14 verifies them manually.

- [ ] **Step 3: Run the tests and confirm they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml sys::`
Expect: 4 FAIL with "not implemented".

- [ ] **Step 4: Implement.** Replace the stubs:

```rust
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows_sys::Win32::UI::Shell::{
    SHEmptyRecycleBinW, SHQueryRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHQUERYRBINFO,
};

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

impl SystemOps for WindowsSys {
    fn recycle_bin_info(&self) -> (u64, u64) {
        let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, i64Size: 0, i64NumItems: 0 };
        // Null root = all drives.
        let hr = unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) };
        if hr < 0 {
            return (0, 0);
        }
        (info.i64Size.max(0) as u64, info.i64NumItems.max(0) as u64)
    }

    fn empty_recycle_bin(&self) -> Result<(), String> {
        let flags = SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND;
        let hr = unsafe { SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), flags) };
        if hr < 0 {
            Err(format!("Windows could not empty the Recycle Bin (HRESULT 0x{:08X})", hr as u32))
        } else {
            Ok(())
        }
    }
}

pub fn drive_usage(root: &str) -> Option<DriveUsage> {
    let path = wide(root);
    let (mut available, mut total, mut free) = (0u64, 0u64, 0u64);
    let ok = unsafe { GetDiskFreeSpaceExW(path.as_ptr(), &mut available, &mut total, &mut free) };
    (ok != 0).then(|| DriveUsage { root: root.to_string(), total_bytes: total, free_bytes: free })
}

pub fn is_elevated() -> bool {
    is_elevated::is_elevated()
}

/// Starts a new elevated copy of this exe through UAC. The caller exits the current process on Ok.
pub fn relaunch_as_admin() -> Result<(), String> {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.to_string_lossy().replace('\'', "''");
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &format!("Start-Process -FilePath '{exe}' -Verb RunAs")])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("Administrator access was not granted.".into())
    }
}
```

If the compiler reports a type mismatch for the `SHERB_*` flags or the HWND argument, check the signature in the installed windows-sys source with `cargo doc --open -p windows-sys` and adjust the cast. The intent is: null window handle, null root (all drives), and no confirmation, progress UI or sound.

- [ ] **Step 5: Run the tests and confirm they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml sys::`
Expect: 4 passed.

- [ ] **Step 6: Commit** with `"Add Windows system access"`.

---

### Task 9: Tauri commands and app state

**Files:**
- Create: `src-tauri/src/commands.rs`
- Modify: replace `src-tauri/src/lib.rs` entirely. This drops the scaffold's `greet`.
- Test: inline in `commands.rs` for `select_findings`

**Interfaces:**
- Consumes:
  - `junk::{KnownPaths, scan_junk}`
  - `executor::{Executor, ExecReport}`
  - `journal::{Journal, JournalEntry}`
  - `protect::ProtectedSet`
  - `sys::{WindowsSys, DriveUsage, drive_usage, is_elevated, relaunch_as_admin}`
  - `model::{Finding, Category}`
- Produces these Tauri commands. They are the frontend's contract, and arguments are camelCase in JS.
  - `scan_junk() -> Finding[]`
  - `clean({ ids: string[], dryRun: boolean }) -> ExecReport`
  - `history() -> JournalEntry[]`, newest first
  - `system_info() -> { elevated: boolean, drive: DriveUsage | null }`
  - `relaunch_as_admin() -> void`, which exits the app on success
- Also produces:
  - `commands::AppState { last_scan: Mutex<HashMap<String, Finding>>, journal: Journal, protect: ProtectedSet }`
  - `commands::select_findings(&HashMap<String, Finding>, &[String]) -> Result<Vec<Finding>, String>`

- [ ] **Step 1: Write the failing test.** Create `src-tauri/src/commands.rs`:

```rust
use crate::executor::{ExecReport, Executor};
use crate::journal::{Journal, JournalEntry};
use crate::model::{Category, Finding};
use crate::protect::ProtectedSet;
use crate::scanners::junk::{scan_junk as run_junk_scan, KnownPaths};
use crate::sys::{self, DriveUsage, WindowsSys};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::SystemTime;
use tauri::{AppHandle, Manager};

pub struct AppState {
    pub last_scan: Mutex<HashMap<String, Finding>>,
    pub journal: Journal,
    pub protect: ProtectedSet,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub elevated: bool,
    pub drive: Option<DriveUsage>,
}

/// Resolve IDs against the latest scan. Fails if any ID is unknown, so a stale UI can never act.
pub fn select_findings(_scan: &HashMap<String, Finding>, _ids: &[String]) -> Result<Vec<Finding>, String> { unimplemented!() }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanners::junk::recycle_bin_finding;

    #[test]
    fn selects_known_ids_in_request_order() {
        let f = recycle_bin_finding(10, 1).unwrap();
        let scan = HashMap::from([(f.id.clone(), f.clone())]);
        assert_eq!(select_findings(&scan, &[f.id.clone()]).unwrap(), vec![f]);
    }

    #[test]
    fn rejects_unknown_ids() {
        let err = select_findings(&HashMap::new(), &["junk:nope".into()]).unwrap_err();
        assert!(err.contains("Scan again"));
    }
}
```

Add `pub mod commands;` to `lib.rs` temporarily so the test compiles.

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml commands`
Expect: 2 FAIL with "not implemented".

- [ ] **Step 3: Implement `select_findings` and the commands.** Replace the stub and append:

```rust
pub fn select_findings(scan: &HashMap<String, Finding>, ids: &[String]) -> Result<Vec<Finding>, String> {
    ids.iter()
        .map(|id| scan.get(id).cloned().ok_or_else(|| "Some items are no longer in the scan results. Scan again.".to_string()))
        .collect()
}

#[tauri::command]
pub async fn scan_junk(app: AppHandle) -> Result<Vec<Finding>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = KnownPaths::from_env().ok_or("Windows environment folders are missing.")?;
        let findings = run_junk_scan(&paths, &WindowsSys, SystemTime::now());
        let state = app.state::<AppState>();
        let mut scan = state.last_scan.lock().map_err(|_| "Scan state is unavailable.")?;
        scan.retain(|_, f| f.category != Category::Junk);
        scan.extend(findings.iter().map(|f| (f.id.clone(), f.clone())));
        Ok::<_, String>(findings)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn clean(app: AppHandle, ids: Vec<String>, dry_run: bool) -> Result<ExecReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let findings = {
            let scan = state.last_scan.lock().map_err(|_| "Scan state is unavailable.")?;
            select_findings(&scan, &ids)?
        };
        let executor = Executor {
            protect: &state.protect,
            journal: &state.journal,
            sys: &WindowsSys,
            elevated: sys::is_elevated(),
            now: SystemTime::now(),
        };
        Ok::<_, String>(executor.run(&findings, dry_run))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn history(app: AppHandle) -> Result<Vec<JournalEntry>, String> {
    let mut entries = app.state::<AppState>().journal.read_all().map_err(|e| e.to_string())?;
    entries.reverse();
    Ok(entries)
}

#[tauri::command]
pub fn system_info() -> SystemInfo {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    SystemInfo { elevated: sys::is_elevated(), drive: sys::drive_usage(&format!("{drive}\\")) }
}

#[tauri::command]
pub fn relaunch_as_admin(app: AppHandle) -> Result<(), String> {
    sys::relaunch_as_admin()?;
    app.exit(0);
    Ok(())
}
```

- [ ] **Step 4: Replace `src-tauri/src/lib.rs`**

```rust
pub mod commands;
pub mod executor;
pub mod fsutil;
pub mod journal;
pub mod model;
pub mod protect;
pub mod scanners;
pub mod scoring;
pub mod sys;

use commands::AppState;
use protect::ProtectedSet;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_local_data_dir()?;
            app.manage(AppState {
                last_scan: Default::default(),
                journal: journal::Journal::new(data_dir.join("journal.jsonl")),
                protect: ProtectedSet::from_system(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan_junk,
            commands::clean,
            commands::history,
            commands::system_info,
            commands::relaunch_as_admin
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

If the scaffold has no `tauri_plugin_opener` dependency, delete the `.plugin(...)` line.

- [ ] **Step 5: Run all Rust tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expect: every test from Tasks 2–9 passes (40 tests), with no warnings about unused imports.

- [ ] **Step 6: Commit** with `"Expose scan, clean, history and system commands"`.

---

### Task 10: Frontend types, API and selection logic

**Files:**
- Create: `src/lib/types.ts`, `src/lib/api.ts`, `src/lib/format.ts`, `src/lib/selection.ts`, `src/lib/useJunkScan.ts`
- Test: `src/lib/format.test.ts`, `src/lib/selection.test.ts`

**Interfaces:**
- Consumes: the Tauri commands from Task 9.
- Produces:
  - Types: `Risk`, `Category`, `Action`, `Finding`, `ItemResult`, `ExecReport`, `JournalEntry`, `DriveUsage`, `SystemInfo`
  - The `api` object: `scanJunk()`, `clean(ids, dryRun)`, `history()`, `systemInfo()`, `relaunchAsAdmin()`
  - `formatBytes(n: number): string`
  - Selection helpers: `isSelectable(f, elevated)`, `defaultSelection(findings, elevated): Set<string>`, `selectedTotal(findings, selected): number`, `groupByRisk(findings): Record<Risk, Finding[]>`, `RISK_ORDER`
  - The `useJunkScan(): JunkScan` hook, where `JunkScan` is `{ findings, status: ScanStatus, error, scan }`

- [ ] **Step 1: Create the types.** Create `src/lib/types.ts`:

```ts
export type Risk = "safe" | "review" | "caution";
export type Category = "junk";

export interface NameFilter { prefix: string | null; suffix: string | null }

export type Action =
  | { kind: "deleteContents"; root: string; filter: NameFilter; minAgeHours: number }
  | { kind: "emptyRecycleBin" };

export interface Finding {
  id: string;
  category: Category;
  title: string;
  sizeBytes: number;
  itemCount: number;
  lastModified: number | null;
  risk: Risk;
  score: number;
  reason: string;
  requiresAdmin: boolean;
  reversible: boolean;
  action: Action;
}

export interface ItemResult {
  findingId: string;
  freedBytes: number;
  deletedCount: number;
  failedCount: number;
  errors: string[];
  skipped: string | null;
}

export interface ExecReport { items: ItemResult[]; totalFreed: number; dryRun: boolean }

export interface JournalEntry {
  timestamp: number;
  findingId: string;
  title: string;
  category: Category;
  actionKind: string;
  freedBytes: number;
  deletedCount: number;
  failedCount: number;
  dryRun: boolean;
  reversible: boolean;
}

export interface DriveUsage { root: string; totalBytes: number; freeBytes: number }
export interface SystemInfo { elevated: boolean; drive: DriveUsage | null }
```

Create `src/lib/api.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import type { ExecReport, Finding, JournalEntry, SystemInfo } from "./types";

export const api = {
  scanJunk: () => invoke<Finding[]>("scan_junk"),
  clean: (ids: string[], dryRun: boolean) => invoke<ExecReport>("clean", { ids, dryRun }),
  history: () => invoke<JournalEntry[]>("history"),
  systemInfo: () => invoke<SystemInfo>("system_info"),
  relaunchAsAdmin: () => invoke<void>("relaunch_as_admin"),
};
```

- [ ] **Step 2: Write the failing tests.** Create `src/lib/format.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { formatBytes } from "./format";

describe("formatBytes", () => {
  it("matches the Rust formatter", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(1024 ** 3)).toBe("1.0 GB");
  });
});
```

Create `src/lib/selection.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { defaultSelection, groupByRisk, isSelectable, selectedTotal } from "./selection";
import type { Finding, Risk } from "./types";

const f = (id: string, risk: Risk, sizeBytes: number, requiresAdmin = false): Finding => ({
  id, risk, sizeBytes, requiresAdmin,
  category: "junk", title: id, itemCount: 1, lastModified: null, score: 50, reason: "", reversible: false,
  action: { kind: "emptyRecycleBin" },
});

const items = [f("a", "safe", 100), f("b", "review", 200), f("c", "safe", 300, true), f("d", "caution", 1)];

describe("selection", () => {
  it("pre-selects only safe items the user can run", () => {
    expect([...defaultSelection(items, false)]).toEqual(["a"]);
    expect([...defaultSelection(items, true)].sort()).toEqual(["a", "c"]);
  });

  it("admin items are not selectable without elevation", () => {
    expect(isSelectable(items[2], false)).toBe(false);
    expect(isSelectable(items[2], true)).toBe(true);
  });

  it("totals selected sizes", () => {
    expect(selectedTotal(items, new Set(["a", "b"]))).toBe(300);
    expect(selectedTotal(items, new Set())).toBe(0);
  });

  it("groups by risk in a fixed order", () => {
    const g = groupByRisk(items);
    expect(Object.keys(g)).toEqual(["safe", "review", "caution"]);
    expect(g.safe.map((x) => x.id)).toEqual(["a", "c"]);
  });
});
```

- [ ] **Step 3: Run the tests and confirm they fail**

Run: `npm test`
Expect: FAIL, "Failed to resolve import ./format" and "./selection".

- [ ] **Step 4: Implement.** Create `src/lib/format.ts`:

```ts
const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toFixed(1)} ${UNITS[unit]}`;
}
```

Create `src/lib/selection.ts`:

```ts
import type { Finding, Risk } from "./types";

export const RISK_ORDER: Risk[] = ["safe", "review", "caution"];

export const isSelectable = (f: Finding, elevated: boolean) => !f.requiresAdmin || elevated;

export function defaultSelection(findings: Finding[], elevated: boolean): Set<string> {
  return new Set(findings.filter((f) => f.risk === "safe" && isSelectable(f, elevated)).map((f) => f.id));
}

export function selectedTotal(findings: Finding[], selected: Set<string>): number {
  return findings.reduce((sum, f) => (selected.has(f.id) ? sum + f.sizeBytes : sum), 0);
}

export function groupByRisk(findings: Finding[]): Record<Risk, Finding[]> {
  const groups: Record<Risk, Finding[]> = { safe: [], review: [], caution: [] };
  for (const f of findings) groups[f.risk].push(f);
  return groups;
}
```

Create `src/lib/useJunkScan.ts`:

```ts
import { useCallback, useState } from "react";
import { api } from "./api";
import type { Finding } from "./types";

export type ScanStatus = "idle" | "scanning" | "done" | "error";

export interface JunkScan {
  findings: Finding[];
  status: ScanStatus;
  error: string | null;
  scan: () => Promise<void>;
}

export function useJunkScan(): JunkScan {
  const [findings, setFindings] = useState<Finding[]>([]);
  const [status, setStatus] = useState<ScanStatus>("idle");
  const [error, setError] = useState<string | null>(null);

  const scan = useCallback(async () => {
    setStatus("scanning");
    setError(null);
    try {
      setFindings(await api.scanJunk());
      setStatus("done");
    } catch (e) {
      setError(String(e));
      setStatus("error");
    }
  }, []);

  return { findings, status, error, scan };
}
```

- [ ] **Step 5: Run the tests and confirm they pass**

Run: `npm test`
Expect: 5 passed. Also run `npx tsc --noEmit` and expect exit 0.

- [ ] **Step 6: Commit** with `"Add frontend types, API client and selection logic"`.

---

### Task 11: App shell, sidebar, admin banner and Overview

**Files:**
- Create: `src/components/Sidebar.tsx`, `src/components/AdminBanner.tsx`, `src/pages/Dashboard.tsx`
- Create (temporary stubs, replaced in Tasks 12–13): `src/pages/JunkPage.tsx`, `src/pages/HistoryPage.tsx`
- Modify: replace `src/App.tsx`

**Interfaces:**
- Consumes: `api`, `formatBytes`, `useJunkScan`/`JunkScan`, `SystemInfo`.
- Produces:
  - `Sidebar({ page, onNavigate })` and `type Page = "dashboard" | "junk" | "history"`
  - `AdminBanner()`
  - `Dashboard({ info, scan, onReview })`
  - `JunkPage({ scan, elevated, onCleaned })`
  - `HistoryPage()`

- [ ] **Step 1: Create `src/components/Sidebar.tsx`**

```tsx
import { History, LayoutDashboard, Trash2 } from "lucide-react";

export type Page = "dashboard" | "junk" | "history";

const ITEMS = [
  { id: "dashboard", label: "Overview", icon: LayoutDashboard },
  { id: "junk", label: "Junk & temp files", icon: Trash2 },
  { id: "history", label: "History", icon: History },
] as const;

export function Sidebar({ page, onNavigate }: { page: Page; onNavigate: (p: Page) => void }) {
  return (
    <nav aria-label="Main" className="flex w-60 shrink-0 flex-col gap-1 border-r border-zinc-800 bg-zinc-900/60 p-4">
      <div className="mb-6 px-3 text-lg font-semibold tracking-tight">PC Cleaner</div>
      {ITEMS.map(({ id, label, icon: Icon }) => (
        <button
          key={id}
          type="button"
          onClick={() => onNavigate(id)}
          aria-current={page === id ? "page" : undefined}
          className={`flex items-center gap-3 rounded-lg px-3 py-2 text-left text-sm transition-colors focus-visible:outline-2 focus-visible:outline-sky-400 ${
            page === id ? "bg-zinc-800 text-white" : "text-zinc-400 hover:bg-zinc-800/60 hover:text-zinc-100"
          }`}
        >
          <Icon size={18} aria-hidden="true" />
          {label}
        </button>
      ))}
    </nav>
  );
}
```

- [ ] **Step 2: Create `src/components/AdminBanner.tsx`**

```tsx
import { ShieldAlert } from "lucide-react";
import { useState } from "react";
import { api } from "../lib/api";

export function AdminBanner() {
  const [error, setError] = useState<string | null>(null);
  return (
    <div className="flex items-center gap-3 border-b border-amber-900/50 bg-amber-950/40 px-8 py-2 text-sm text-amber-200">
      <ShieldAlert size={16} aria-hidden="true" />
      <span className="flex-1">
        Some system locations need administrator rights.
        {error && <span role="alert"> {error}</span>}
      </span>
      <button
        type="button"
        onClick={() => api.relaunchAsAdmin().catch((e) => setError(String(e)))}
        className="rounded-md bg-amber-500/20 px-3 py-1 font-medium hover:bg-amber-500/30 focus-visible:outline-2 focus-visible:outline-amber-300"
      >
        Restart as admin
      </button>
    </div>
  );
}
```

- [ ] **Step 3: Create `src/pages/Dashboard.tsx`**

```tsx
import { HardDrive, Loader2, Sparkles } from "lucide-react";
import { formatBytes } from "../lib/format";
import type { SystemInfo } from "../lib/types";
import type { JunkScan } from "../lib/useJunkScan";

const primary =
  "inline-flex items-center gap-2 rounded-lg bg-sky-500 px-4 py-2 font-medium text-zinc-950 hover:bg-sky-400 disabled:opacity-60 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-sky-300";
const secondary =
  "inline-flex items-center gap-2 rounded-lg border border-zinc-700 px-4 py-2 text-zinc-200 hover:bg-zinc-800 disabled:opacity-60 focus-visible:outline-2 focus-visible:outline-sky-400";

export function Dashboard({ info, scan, onReview }: { info: SystemInfo | null; scan: JunkScan; onReview: () => void }) {
  const drive = info?.drive;
  const usedPct = drive ? Math.round(((drive.totalBytes - drive.freeBytes) / drive.totalBytes) * 100) : 0;
  const total = scan.findings.reduce((s, f) => s + f.sizeBytes, 0);
  const safe = scan.findings.filter((f) => f.risk === "safe").reduce((s, f) => s + f.sizeBytes, 0);
  const scanning = scan.status === "scanning";

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold tracking-tight">Overview</h1>

      {drive && (
        <section aria-labelledby="drive-heading" className="rounded-xl border border-zinc-800 bg-zinc-900 p-5">
          <div className="mb-3 flex items-center gap-2">
            <HardDrive size={18} aria-hidden="true" />
            <h2 id="drive-heading" className="font-medium">Drive {drive.root}</h2>
            <span className="ml-auto text-sm tabular-nums text-zinc-400">
              {formatBytes(drive.freeBytes)} free of {formatBytes(drive.totalBytes)}
            </span>
          </div>
          <div
            role="progressbar"
            aria-label="Disk space used"
            aria-valuenow={usedPct}
            aria-valuemin={0}
            aria-valuemax={100}
            className="h-3 overflow-hidden rounded-full bg-zinc-800"
          >
            <div
              className={`h-full rounded-full ${usedPct > 90 ? "bg-red-500" : usedPct > 75 ? "bg-amber-500" : "bg-sky-500"}`}
              style={{ width: `${usedPct}%` }}
            />
          </div>
        </section>
      )}

      <section aria-labelledby="junk-heading" className="rounded-xl border border-zinc-800 bg-zinc-900 p-5">
        <h2 id="junk-heading" className="sr-only">Junk files</h2>
        {scan.status === "done" ? (
          <div className="flex items-center gap-4">
            <div className="flex-1">
              <p className="text-3xl font-semibold tabular-nums">{formatBytes(total)}</p>
              <p className="text-sm text-zinc-400">
                can be freed. {formatBytes(safe)} of it is safe to remove right away.
              </p>
            </div>
            <button type="button" className={secondary} onClick={scan.scan}>Scan again</button>
            <button type="button" className={primary} onClick={onReview}>Review</button>
          </div>
        ) : (
          <div className="flex items-center gap-4">
            <p className="flex-1 text-zinc-300">Find temporary files, caches and other junk that's safe to remove.</p>
            <button type="button" className={primary} disabled={scanning} onClick={scan.scan}>
              {scanning ? <Loader2 size={16} className="animate-spin" aria-hidden="true" /> : <Sparkles size={16} aria-hidden="true" />}
              {scanning ? "Scanning…" : "Scan now"}
            </button>
          </div>
        )}
        {scan.error && <p role="alert" className="mt-3 text-sm text-red-400">The scan failed: {scan.error}</p>}
      </section>
    </div>
  );
}
```

- [ ] **Step 4: Create temporary page stubs** so the shell compiles. Tasks 12 and 13 replace both files.

`src/pages/JunkPage.tsx`:

```tsx
import type { JunkScan } from "../lib/useJunkScan";

export function JunkPage(_props: { scan: JunkScan; elevated: boolean; onCleaned: () => void }) {
  return <h1 className="text-2xl font-semibold">Junk & temp files</h1>;
}
```

`src/pages/HistoryPage.tsx`:

```tsx
export function HistoryPage() {
  return <h1 className="text-2xl font-semibold">History</h1>;
}
```

- [ ] **Step 5: Replace `src/App.tsx`**

```tsx
import { useCallback, useEffect, useState } from "react";
import { AdminBanner } from "./components/AdminBanner";
import { Sidebar, type Page } from "./components/Sidebar";
import { api } from "./lib/api";
import type { SystemInfo } from "./lib/types";
import { useJunkScan } from "./lib/useJunkScan";
import { Dashboard } from "./pages/Dashboard";
import { HistoryPage } from "./pages/HistoryPage";
import { JunkPage } from "./pages/JunkPage";

export default function App() {
  const [page, setPage] = useState<Page>("dashboard");
  const [info, setInfo] = useState<SystemInfo | null>(null);
  const scan = useJunkScan();

  const refreshInfo = useCallback(() => {
    api.systemInfo().then(setInfo).catch(() => setInfo(null));
  }, []);
  useEffect(refreshInfo, [refreshInfo]);

  const elevated = info?.elevated ?? false;

  return (
    <div className="flex h-screen bg-zinc-950 text-zinc-100">
      <Sidebar page={page} onNavigate={setPage} />
      <main className="flex-1 overflow-y-auto">
        {info && !elevated && <AdminBanner />}
        <div className="mx-auto max-w-4xl p-8">
          {page === "dashboard" && <Dashboard info={info} scan={scan} onReview={() => setPage("junk")} />}
          {page === "junk" && <JunkPage scan={scan} elevated={elevated} onCleaned={refreshInfo} />}
          {page === "history" && <HistoryPage />}
        </div>
      </main>
    </div>
  );
}
```

- [ ] **Step 6: Verify**

Run: `npx tsc --noEmit`. Expect: exit 0.
Run: `npm run tauri dev`. Expect:
- The sidebar shows three entries, and the Overview shows drive C: with a usage bar.
- The amber admin banner is visible.
- Clicking "Scan now" shows "Scanning…" and then a total with "Review" and "Scan again" buttons.
- Tabbing through the controls shows a visible focus ring on each.

- [ ] **Step 7: Commit** with `"Add app shell, sidebar and overview"`.

---

### Task 12: Junk review page and clean flow

**Files:**
- Create: `src/components/FindingRow.tsx`, `src/components/ConfirmDialog.tsx`
- Modify: replace the `src/pages/JunkPage.tsx` stub

**Interfaces:**
- Consumes:
  - `api.clean`
  - `formatBytes`
  - `defaultSelection`, `groupByRisk`, `isSelectable`, `selectedTotal`, `RISK_ORDER`
  - `JunkScan`
  - `ExecReport`, `Finding`, `Risk`
- Produces:
  - `FindingRow({ finding, checked, disabled, onToggle })`
  - `ConfirmDialog({ open, title, children, confirmLabel, onConfirm, onCancel })`
  - `JunkPage({ scan, elevated, onCleaned })`

- [ ] **Step 1: Create `src/components/FindingRow.tsx`**

```tsx
import { AlertTriangle, ShieldAlert } from "lucide-react";
import { useId } from "react";
import { formatBytes } from "../lib/format";
import type { Finding } from "../lib/types";

interface Props { finding: Finding; checked: boolean; disabled: boolean; onToggle: () => void }

export function FindingRow({ finding: f, checked, disabled, onToggle }: Props) {
  const id = useId();
  return (
    <li className={`flex gap-4 rounded-lg border border-zinc-800 bg-zinc-900 p-4 ${disabled ? "opacity-60" : ""}`}>
      <input
        id={id}
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={onToggle}
        aria-describedby={`${id}-reason`}
        className="mt-1 size-4 accent-sky-500"
      />
      <label htmlFor={id} className="flex-1 cursor-pointer">
        <span className="flex items-center gap-2">
          <span className="font-medium">{f.title}</span>
          {f.requiresAdmin && (
            <span className="inline-flex items-center gap-1 rounded bg-amber-500/15 px-1.5 py-0.5 text-xs text-amber-300">
              <ShieldAlert size={12} aria-hidden="true" /> Admin
            </span>
          )}
          {!f.reversible && f.risk !== "safe" && (
            <span className="inline-flex items-center gap-1 rounded bg-red-500/15 px-1.5 py-0.5 text-xs text-red-300">
              <AlertTriangle size={12} aria-hidden="true" /> Can't be undone
            </span>
          )}
          <span className="ml-auto tabular-nums text-zinc-300">{formatBytes(f.sizeBytes)}</span>
        </span>
        <span id={`${id}-reason`} className="mt-1 block text-sm text-zinc-400">{f.reason}</span>
      </label>
    </li>
  );
}
```

- [ ] **Step 2: Create `src/components/ConfirmDialog.tsx`**

```tsx
import { useEffect, useRef, type ReactNode } from "react";

interface Props {
  open: boolean;
  title: string;
  children: ReactNode;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({ open, title, children, confirmLabel, onConfirm, onCancel }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) d.showModal();
    if (!open && d.open) d.close();
  }, [open]);

  return (
    <dialog
      ref={ref}
      aria-labelledby="confirm-title"
      onCancel={(e) => { e.preventDefault(); onCancel(); }}
      className="m-auto max-w-md rounded-xl border border-zinc-700 bg-zinc-900 p-6 text-zinc-100 backdrop:bg-black/60"
    >
      <h2 id="confirm-title" className="text-lg font-semibold">{title}</h2>
      <div className="mt-2 text-sm text-zinc-300">{children}</div>
      <div className="mt-6 flex justify-end gap-2">
        <button type="button" autoFocus onClick={onCancel}
          className="rounded-lg border border-zinc-700 px-4 py-2 hover:bg-zinc-800 focus-visible:outline-2 focus-visible:outline-sky-400">
          Cancel
        </button>
        <button type="button" onClick={onConfirm}
          className="rounded-lg bg-red-500 px-4 py-2 font-medium text-white hover:bg-red-400 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-red-300">
          {confirmLabel}
        </button>
      </div>
    </dialog>
  );
}
```

- [ ] **Step 3: Replace `src/pages/JunkPage.tsx`**

```tsx
import { Loader2, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { FindingRow } from "../components/FindingRow";
import { api } from "../lib/api";
import { formatBytes } from "../lib/format";
import { defaultSelection, groupByRisk, isSelectable, RISK_ORDER, selectedTotal } from "../lib/selection";
import type { ExecReport, Risk } from "../lib/types";
import type { JunkScan } from "../lib/useJunkScan";

const GROUP_COPY: Record<Risk, { heading: string; hint: string }> = {
  safe: { heading: "Safe to remove", hint: "Windows and your apps recreate these automatically." },
  review: { heading: "Review first", hint: "Usually fine to remove, but take a look before you do." },
  caution: { heading: "Be careful", hint: "Only remove these if you're sure." },
};

interface Props { scan: JunkScan; elevated: boolean; onCleaned: () => void }
interface Result { report: ExecReport; titles: Record<string, string> }

export function JunkPage({ scan, elevated, onCleaned }: Props) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [dryRun, setDryRun] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<Result | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (scan.status === "idle") void scan.scan();
  }, [scan.status, scan.scan]);
  useEffect(() => {
    setSelected(defaultSelection(scan.findings, elevated));
  }, [scan.findings, elevated]);

  const groups = groupByRisk(scan.findings);
  const total = selectedTotal(scan.findings, selected);
  const toggle = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  async function runClean() {
    setConfirming(false);
    setBusy(true);
    setError(null);
    const titles = Object.fromEntries(scan.findings.map((f) => [f.id, f.title]));
    try {
      const report = await api.clean([...selected], dryRun);
      setResult({ report, titles });
      if (!dryRun) {
        await scan.scan();
        onCleaned();
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const problems = result?.report.items.filter((i) => i.skipped || i.failedCount > 0) ?? [];

  return (
    <div className="space-y-6 pb-24">
      <div className="flex items-center gap-3">
        <h1 className="flex-1 text-2xl font-semibold tracking-tight">Junk & temp files</h1>
        <button type="button" onClick={scan.scan} disabled={scan.status === "scanning" || busy}
          className="inline-flex items-center gap-2 rounded-lg border border-zinc-700 px-3 py-1.5 text-sm hover:bg-zinc-800 disabled:opacity-60 focus-visible:outline-2 focus-visible:outline-sky-400">
          <RefreshCw size={14} aria-hidden="true" /> Scan again
        </button>
      </div>

      {scan.status === "scanning" && (
        <p role="status" className="flex items-center gap-2 text-zinc-400">
          <Loader2 size={16} className="animate-spin" aria-hidden="true" /> Scanning…
        </p>
      )}
      {scan.error && <p role="alert" className="text-red-400">The scan failed: {scan.error}</p>}
      {error && <p role="alert" className="text-red-400">Cleaning failed: {error}</p>}

      {result && (
        <section role="status" className="rounded-xl border border-emerald-900/60 bg-emerald-950/30 p-4">
          <p className="font-medium text-emerald-200">
            {result.report.dryRun ? "Dry run: this would free " : "Freed "}
            <span className="tabular-nums">{formatBytes(result.report.totalFreed)}</span>.
          </p>
          {problems.length > 0 && (
            <ul className="mt-2 space-y-1 text-sm text-zinc-300">
              {problems.map((i) => (
                <li key={i.findingId}>
                  {result.titles[i.findingId] ?? i.findingId}:{" "}
                  {i.skipped ?? `${i.failedCount} files were in use and were skipped.`}
                </li>
              ))}
            </ul>
          )}
        </section>
      )}

      {scan.status === "done" && scan.findings.length === 0 && (
        <p className="text-zinc-400">Nothing to clean. Your PC has no junk in the locations this app checks.</p>
      )}

      {RISK_ORDER.filter((r) => groups[r].length > 0).map((risk) => (
        <section key={risk} aria-labelledby={`group-${risk}`} className="space-y-2">
          <h2 id={`group-${risk}`} className="font-medium">{GROUP_COPY[risk].heading}</h2>
          <p className="text-sm text-zinc-400">{GROUP_COPY[risk].hint}</p>
          <ul className="space-y-2">
            {groups[risk].map((f) => (
              <FindingRow key={f.id} finding={f} checked={selected.has(f.id)}
                disabled={!isSelectable(f, elevated) || busy} onToggle={() => toggle(f.id)} />
            ))}
          </ul>
        </section>
      ))}

      <footer className="fixed bottom-0 left-60 right-0 flex items-center gap-4 border-t border-zinc-800 bg-zinc-950/95 px-8 py-4 backdrop-blur">
        <label className="flex items-center gap-2 text-sm text-zinc-300">
          <input type="checkbox" checked={dryRun} onChange={(e) => setDryRun(e.target.checked)} className="size-4 accent-sky-500" />
          Dry run (show what would be removed)
        </label>
        <span className="ml-auto text-sm tabular-nums text-zinc-300">
          {selected.size} selected · {formatBytes(total)}
        </span>
        <button type="button" disabled={selected.size === 0 || busy} onClick={() => setConfirming(true)}
          className="inline-flex items-center gap-2 rounded-lg bg-sky-500 px-4 py-2 font-medium text-zinc-950 hover:bg-sky-400 disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-sky-300">
          {busy && <Loader2 size={16} className="animate-spin" aria-hidden="true" />}
          {dryRun ? "Run dry run" : "Clean selected"}
        </button>
      </footer>

      <ConfirmDialog
        open={confirming}
        title={dryRun ? "Run a dry run?" : `Remove ${formatBytes(total)}?`}
        confirmLabel={dryRun ? "Run dry run" : "Remove permanently"}
        onConfirm={runClean}
        onCancel={() => setConfirming(false)}
      >
        {dryRun
          ? "Nothing will be deleted. You'll see how much space would be freed."
          : `${selected.size} items will be deleted permanently. Junk files don't go to the Recycle Bin, because that would free no space.`}
      </ConfirmDialog>
    </div>
  );
}
```

- [ ] **Step 4: Verify**

Run: `npx tsc --noEmit` and `npm test`. Expect: both pass.
Run: `npm run tauri dev`, open "Junk & temp files", and expect:
- Findings are grouped under "Safe to remove" and "Review first", with "Safe" items checked.
- Admin items are disabled while the app isn't elevated.
- Turn on Dry run, then Run dry run, then confirm. The green panel says "Dry run: this would free …", and no files change. Check `%TEMP%` before and after.
- Pressing Escape in the dialog cancels it, and focus starts on Cancel.

- [ ] **Step 5: Commit** with `"Add junk review page and clean flow"`.

---

### Task 13: History page

**Files:**
- Modify: replace the `src/pages/HistoryPage.tsx` stub

**Interfaces:**
- Consumes: `api.history`, `formatBytes`, `JournalEntry`.
- Produces: `HistoryPage()`.

- [ ] **Step 1: Replace `src/pages/HistoryPage.tsx`**

```tsx
import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { formatBytes } from "../lib/format";
import type { JournalEntry } from "../lib/types";

function outcome(e: JournalEntry): string {
  if (e.dryRun) return `Dry run: ${e.deletedCount} items`;
  const skipped = e.failedCount > 0 ? `, ${e.failedCount} skipped` : "";
  return `${e.deletedCount} removed${skipped}`;
}

export function HistoryPage() {
  const [entries, setEntries] = useState<JournalEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.history().then(setEntries).catch((e) => setError(String(e)));
  }, []);

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold tracking-tight">History</h1>
      {error && <p role="alert" className="text-red-400">Couldn't load history: {error}</p>}
      {!entries && !error && <p className="text-zinc-400">Loading…</p>}
      {entries?.length === 0 && <p className="text-zinc-400">Nothing has been cleaned yet.</p>}
      {entries && entries.length > 0 && (
        <table className="w-full text-left text-sm">
          <thead className="border-b border-zinc-800 text-zinc-400">
            <tr>
              <th scope="col" className="py-2 font-medium">When</th>
              <th scope="col" className="py-2 font-medium">Item</th>
              <th scope="col" className="py-2 font-medium">Result</th>
              <th scope="col" className="py-2 text-right font-medium">Freed</th>
            </tr>
          </thead>
          <tbody>
            {entries.map((e, i) => (
              <tr key={`${e.timestamp}-${e.findingId}-${i}`} className="border-b border-zinc-900">
                <td className="py-2 text-zinc-400">{new Date(e.timestamp * 1000).toLocaleString()}</td>
                <td className="py-2">{e.title}</td>
                <td className={`py-2 ${e.dryRun ? "text-zinc-500" : "text-zinc-300"}`}>{outcome(e)}</td>
                <td className="py-2 text-right tabular-nums">{formatBytes(e.freedBytes)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Verify.** Run `npx tsc --noEmit`. Then, in `npm run tauri dev`, open History and expect the dry run from Task 12 listed with "Dry run: N items".

- [ ] **Step 3: Commit** with `"Add history page"`.

---

### Task 14: End-to-end verification and installer

**Files:** none, unless a bug is found. If one is, use `systematic-debugging` and add a regression test to the relevant module's tests.

**Interfaces:**
- Consumes: the whole app.
- Produces: a verified build, and an installer at `src-tauri/target/release/bundle/nsis/*.exe`.

- [ ] **Step 1: Run the full test suites**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`. Expect: all pass.
Run: `npm test`. Expect: all pass.

- [ ] **Step 2: Test a real clean as a normal user** (`npm run tauri dev`)
  1. Create a test file: `New-Item "$env:TEMP\pc-cleaner-e2e.tmp" -Value "x" | ForEach-Object { $_.LastWriteTime = (Get-Date).AddDays(-2) }`
  2. Scan. "Your temporary files" is listed and checked.
  3. Clean selected, then Remove permanently.
  4. Expect: the green panel reports bytes freed; `Test-Path "$env:TEMP\pc-cleaner-e2e.tmp"` returns `False`; the History screen shows the entry; and the Overview free space went up or stayed the same.
  5. Expect: admin-only findings were disabled, so they are absent from History.

- [ ] **Step 3: Test elevated mode.** Click "Restart as admin" and accept the UAC prompt.
  - Expect: the app reopens without the banner, and the admin findings can now be checked.
  - Run a **dry run** on "Windows temporary files" and check it reports a size.
  - Declining UAC from a non-elevated window instead shows "Administrator access was not granted."

- [ ] **Step 4: Test the Recycle Bin.**
  1. Delete a scratch file to the Recycle Bin from Explorer, then scan.
  2. The Recycle Bin appears under "Review first", unchecked, with a "Can't be undone" badge.
  3. Check it and clean it. Expect: the Recycle Bin is empty in Explorer.

- [ ] **Step 5: Check the safety guard on a real system.** Run `cargo test --manifest-path src-tauri/Cargo.toml system_set_protects`. Expect: it passes, which shows the real `%SystemRoot%`, System32, `C:\` and Documents are denied and `%TEMP%` is allowed.

- [ ] **Step 6: Build the installer**

Run: `npm run tauri build`
Expect: exit 0, and an installer under `src-tauri/target/release/bundle/nsis/`. Install it, launch "PC Cleaner" from the Start menu, and run one scan.

- [ ] **Step 7: Commit** any fixes, then tag: `git tag piece-1`.

---

## Spec coverage (self-review)

| Spec requirement (piece 1) | Task |
|---|---|
| Finding model with risk, score, reason, admin and reversible flags | 2 |
| Scoring by risk plus size | 2 |
| Protected-path guard on the root and every file | 4, 6 |
| Executor as the only deleter; IDs, not paths, from the frontend | 6, 9 |
| Continue past in-use files and report them | 6, 12 |
| Journal plus History screen | 5, 6, 13 |
| `SystemOps` trait, Recycle Bin, drive usage | 6, 8 |
| Elevation flagging and relaunch as admin | 8, 9, 11, 12 |
| All ten junk locations in the spec table | 7 |
| Junk deleted permanently; only files older than the minimum age | 6, 7, 12 |
| Only safe, runnable findings pre-checked | 10, 12 |
| Dry-run mode | 6, 12 |
| Confirmation before delete | 12 |
| Overview dashboard with drive usage | 11 |
| No network calls | Global constraint; no HTTP dependency is added |
