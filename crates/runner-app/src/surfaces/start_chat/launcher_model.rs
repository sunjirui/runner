use std::{collections::BTreeMap, fs, path::Path};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum EntryKey {
    Agent(String),
    Role(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RecentEntry {
    pub(crate) key: EntryKey,
    pub(crate) used_at: Option<i64>,
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct RecentStore {
    computers: BTreeMap<String, Vec<RecentEntry>>,
}

const FILE: &str = "launcher-recent.json";

impl RecentStore {
    pub(crate) fn load(directory: &Path) -> Self {
        if let Ok(text) = fs::read_to_string(directory.join(FILE)) {
            return serde_json::from_str(&text).unwrap_or_default();
        }
        let mut store = Self::default();
        if let Ok(text) = fs::read_to_string(directory.join("start-chat-recent-runtimes")) {
            let entries = store.computers.entry("local".into()).or_default();
            for runtime in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
                let key = EntryKey::Agent(runtime.into());
                if !entries.iter().any(|entry| entry.key == key) {
                    entries.push(RecentEntry { key, used_at: None });
                }
            }
            let _ = store.save(directory);
        }
        store
    }

    pub(crate) fn entries(&self, computer: &str) -> &[RecentEntry] {
        self.computers
            .get(computer)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub(crate) fn record(&mut self, computer: &str, key: EntryKey, now: i64) {
        let entries = self.computers.entry(computer.into()).or_default();
        entries.retain(|entry| entry.key != key);
        entries.insert(
            0,
            RecentEntry {
                key,
                used_at: Some(now),
            },
        );
    }

    pub(crate) fn save(&self, directory: &Path) -> std::io::Result<()> {
        fs::write(directory.join(FILE), serde_json::to_vec(self)?)
    }
}

pub(crate) struct SearchEntry {
    pub(crate) key: EntryKey,
    pub(crate) name: String,
    pub(crate) runtime: String,
    pub(crate) handle: String,
}

pub(crate) fn match_rank(entry: &SearchEntry, query: &str) -> Option<u8> {
    let query = query.to_lowercase();
    let name = entry.name.to_lowercase();
    let handle = entry.handle.to_lowercase();
    if name.starts_with(&query) || handle.starts_with(&query) {
        Some(0)
    } else if [&name, &handle].iter().any(|text| {
        text.split(|c: char| !c.is_alphanumeric())
            .any(|word| word.starts_with(&query))
    }) {
        Some(1)
    } else if name.contains(&query)
        || handle.contains(&query)
        || entry.runtime.to_lowercase().contains(&query)
    {
        Some(2)
    } else {
        None
    }
}

pub(crate) fn ranked<'a>(
    entries: &'a [SearchEntry],
    recent: &[RecentEntry],
    default: &str,
    query: &str,
) -> Vec<&'a SearchEntry> {
    let recency = |key: &EntryKey| {
        recent
            .iter()
            .position(|entry| &entry.key == key)
            .unwrap_or(usize::MAX)
    };
    let mut found = entries
        .iter()
        .filter(|entry| match_rank(entry, query).is_some())
        .collect::<Vec<_>>();
    if query.is_empty() {
        found.sort_by(|a, b| {
            let fill = |entry: &SearchEntry| match &entry.key {
                EntryKey::Agent(key) if key == default => 0,
                EntryKey::Agent(_) => 1,
                EntryKey::Role(_) => 2,
            };
            recency(&a.key)
                .cmp(&recency(&b.key))
                .then_with(|| fill(a).cmp(&fill(b)))
                .then_with(|| {
                    if matches!(a.key, EntryKey::Role(_)) && matches!(b.key, EntryKey::Role(_)) {
                        a.name.to_lowercase().cmp(&b.name.to_lowercase())
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
        });
        found.truncate(4);
    } else {
        found.sort_by(|a, b| {
            match_rank(a, query)
                .cmp(&match_rank(b, query))
                .then_with(|| recency(&a.key).cmp(&recency(&b.key)))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
    }
    found
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum KeyResult {
    Select(usize),
    Start(usize),
    Clear,
    Text,
}

pub(crate) fn key_result(
    key: &str,
    query_empty: bool,
    selected: usize,
    ranked: usize,
    pinned: usize,
) -> KeyResult {
    let count = ranked + pinned;
    match key {
        "up" if count > 0 => KeyResult::Select((selected + count - 1) % count),
        "down" if count > 0 => KeyResult::Select((selected + 1) % count),
        "enter" if count > 0 => KeyResult::Start(selected.min(count - 1)),
        "escape" if !query_empty => KeyResult::Clear,
        "1" | "2" | "3" | "4" if query_empty => {
            let index = key.parse::<usize>().unwrap() - 1;
            if index < ranked {
                KeyResult::Start(index)
            } else {
                KeyResult::Text
            }
        }
        _ => KeyResult::Text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn agent(key: &str, name: &str) -> SearchEntry {
        SearchEntry {
            key: EntryKey::Agent(key.into()),
            name: name.into(),
            runtime: key.into(),
            handle: String::new(),
        }
    }
    fn names(entries: Vec<&SearchEntry>) -> Vec<&str> {
        entries
            .into_iter()
            .map(|entry| entry.name.as_str())
            .collect()
    }
    #[test]
    fn fills_default_then_catalog_then_roles_and_skips_stale_recents() {
        let mut entries = vec![agent("b", "Beta"), agent("a", "Alpha")];
        entries.push(SearchEntry {
            key: EntryKey::Role("r".into()),
            name: "Role".into(),
            runtime: "b".into(),
            handle: "coder".into(),
        });
        let recent = vec![
            RecentEntry {
                key: EntryKey::Agent("gone".into()),
                used_at: None,
            },
            RecentEntry {
                key: EntryKey::Role("r".into()),
                used_at: None,
            },
        ];
        assert_eq!(
            names(ranked(&entries, &recent, "a", "")),
            ["Role", "Alpha", "Beta"]
        );
        assert_eq!(
            names(ranked(&entries, &[], "a", "")),
            ["Alpha", "Beta", "Role"]
        );
        assert_eq!(names(ranked(&entries, &[], "a", "CODE")), ["Role"]);
    }
    #[test]
    fn search_ranks_prefix_word_substring_then_recency_then_name() {
        let entries = vec![
            agent("a", "Team codex"),
            agent("b", "Decode"),
            agent("c", "Codex"),
            agent("d", "Coder"),
        ];
        let recent = vec![RecentEntry {
            key: entries[3].key.clone(),
            used_at: Some(1),
        }];
        assert_eq!(
            names(ranked(&entries, &recent, "", "cod")),
            ["Coder", "Codex", "Team codex", "Decode"]
        );
    }
    #[test]
    fn migration_preserves_order_and_computers_are_isolated() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("start-chat-recent-runtimes"),
            " codex\nclaude-code\ncodex\n",
        )
        .unwrap();
        let mut store = RecentStore::load(directory.path());
        assert_eq!(store.entries("local").len(), 2);
        assert_eq!(
            store.entries("local")[0].key,
            EntryKey::Agent("codex".into())
        );
        assert!(store.entries("host").is_empty());
        store.record("host", EntryKey::Role("r".into()), 10);
        store.record("local", EntryKey::Role("local-role".into()), 11);
        store.save(directory.path()).unwrap();
        let store = RecentStore::load(directory.path());
        assert_eq!(store.entries("host")[0].used_at, Some(10));
        assert_eq!(
            store.entries("local")[0].key,
            EntryKey::Role("local-role".into())
        );
    }
    #[test]
    fn keys_include_pinned_rows_and_leave_search_digits_as_text() {
        assert_eq!(key_result("down", true, 3, 4, 2), KeyResult::Select(4));
        assert_eq!(key_result("up", true, 0, 4, 2), KeyResult::Select(5));
        assert_eq!(key_result("4", true, 0, 4, 2), KeyResult::Start(3));
        assert_eq!(key_result("4", false, 0, 4, 2), KeyResult::Text);
        assert_eq!(key_result("escape", false, 0, 4, 2), KeyResult::Clear);
        assert_eq!(key_result("enter", false, 5, 4, 2), KeyResult::Start(5));
    }
}
