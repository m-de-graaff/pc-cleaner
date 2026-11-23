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
