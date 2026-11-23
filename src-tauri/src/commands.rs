use crate::executor::{ExecReport, Executor};
use crate::journal::{Journal, JournalEntry};
use crate::model::{Category, Finding};
use crate::protect::ProtectedSet;
use crate::scanners::devtools::{scan_devtools as run_devtools_scan, DevScan};
use crate::scanners::junk::{scan_junk_with_progress, KnownPaths};
use crate::sys::{self, DriveUsage, WindowsSys};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::SystemTime;
use tauri::{AppHandle, Emitter, Manager};

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
pub fn select_findings(scan: &HashMap<String, Finding>, ids: &[String]) -> Result<Vec<Finding>, String> {
    ids.iter()
        .map(|id| scan.get(id).cloned().ok_or_else(|| "Some items are no longer in the scan results. Scan again.".to_string()))
        .collect()
}

#[tauri::command]
pub async fn scan_junk(app: AppHandle) -> Result<Vec<Finding>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = KnownPaths::from_env().ok_or("Windows environment folders are missing.")?;
        let findings = scan_junk_with_progress(&paths, &WindowsSys, SystemTime::now(), &|progress| {
            let _ = app.emit("scan-progress", progress); // progress is cosmetic; a dropped event is harmless
        });
        remember(&app, Category::Junk, &findings)?;
        Ok::<_, String>(findings)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn scan_devtools(app: AppHandle) -> Result<DevScan, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = KnownPaths::from_env().ok_or("Windows environment folders are missing.")?;
        let scan = run_devtools_scan(&paths, &WindowsSys, SystemTime::now());
        remember(&app, Category::DevTools, &scan.findings)?;
        Ok::<_, String>(scan)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn remember(app: &AppHandle, category: Category, findings: &[Finding]) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut scan = state.last_scan.lock().map_err(|_| "Scan state is unavailable.")?;
    replace_category(&mut scan, category, findings);
    Ok(())
}

/// A new scan of one page replaces only that page's findings, so the other page can still clean.
pub fn replace_category(scan: &mut HashMap<String, Finding>, category: Category, findings: &[Finding]) {
    scan.retain(|_, f| f.category != category);
    scan.extend(findings.iter().map(|f| (f.id.clone(), f.clone())));
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
    fn rescanning_one_page_keeps_the_other_pages_findings() {
        let junk = recycle_bin_finding(10, 1).unwrap();
        let dev = Finding { id: "devtools:x".into(), category: Category::DevTools, ..junk.clone() };
        let mut scan = HashMap::new();
        replace_category(&mut scan, Category::Junk, &[junk.clone()]);
        replace_category(&mut scan, Category::DevTools, &[dev.clone()]);
        replace_category(&mut scan, Category::DevTools, &[]);
        assert!(scan.contains_key(&junk.id));
        assert!(!scan.contains_key(&dev.id));
    }

    #[test]
    fn rejects_unknown_ids() {
        let err = select_findings(&HashMap::new(), &["junk:nope".into()]).unwrap_err();
        assert!(err.contains("Scan again"));
    }
}
