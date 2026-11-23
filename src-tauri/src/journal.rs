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
