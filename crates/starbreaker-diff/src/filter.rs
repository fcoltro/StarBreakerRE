//! `DiffFilter` (search text, tier, status, extension, record type, path
//! prefix, include-unchanged) and the helpers that apply it to diff items
//! (`filter_diff_items`, `diff_item_matches_filter`).

use crate::compare::{DiffItem, DiffSide, DiffStatus};
use crate::report::{extension_for_path, Tier};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DiffFilter {
    pub search: Option<String>,
    pub tiers: Vec<Tier>,
    pub statuses: Vec<DiffStatus>,
    pub extensions: Vec<String>,
    pub record_types: Vec<String>,
    pub path_prefixes: Vec<String>,
    pub include_unchanged: bool,
}

pub fn filter_diff_items<'a>(
    items: &'a [DiffItem],
    filter: &DiffFilter,
) -> Vec<&'a DiffItem> {
    items
        .iter()
        .filter(|item| diff_item_matches_filter(item, filter))
        .collect()
}

pub fn diff_item_matches_filter(item: &DiffItem, filter: &DiffFilter) -> bool {
    if !filter.tiers.is_empty() && !filter.tiers.contains(&item.tier) {
        return false;
    }
    if !filter.statuses.is_empty() && !filter.statuses.contains(&item.status) {
        return false;
    }
    if !filter.include_unchanged && item.status == DiffStatus::Unchanged {
        return false;
    }
    if !filter.extensions.is_empty() && !matches_extension(item, &filter.extensions) {
        return false;
    }
    if !filter.record_types.is_empty() && !matches_record_type(item, &filter.record_types) {
        return false;
    }
    if !filter.path_prefixes.is_empty() && !matches_path_prefix(item, &filter.path_prefixes) {
        return false;
    }
    if let Some(search) = &filter.search
        && !search.trim().is_empty()
        && !matches_search(item, search)
    {
        return false;
    }
    true
}

fn matches_search(item: &DiffItem, search: &str) -> bool {
    let needle = search.to_ascii_lowercase();
    searchable_fields(item)
        .into_iter()
        .any(|field| field.to_ascii_lowercase().contains(&needle))
}

fn matches_extension(item: &DiffItem, extensions: &[String]) -> bool {
    if item.tier != Tier::P4k {
        return false;
    }
    let normalized: Vec<_> = extensions
        .iter()
        .map(|ext| {
            if ext.starts_with('.') {
                ext.to_ascii_lowercase()
            } else {
                format!(".{}", ext.to_ascii_lowercase())
            }
        })
        .collect();
    archive_paths(item)
        .into_iter()
        .filter_map(|path| extension_for_path(&path))
        .any(|ext| normalized.contains(&ext))
}

fn matches_record_type(item: &DiffItem, record_types: &[String]) -> bool {
    if item.tier != Tier::DataCore {
        return false;
    }
    let wanted: Vec<_> = record_types
        .iter()
        .map(|record_type| record_type.to_ascii_lowercase())
        .collect();
    datacore_types(item)
        .into_iter()
        .any(|record_type| wanted.contains(&record_type.to_ascii_lowercase()))
}

fn matches_path_prefix(item: &DiffItem, prefixes: &[String]) -> bool {
    let normalized: Vec<_> = prefixes
        .iter()
        .map(|prefix| prefix.replace('/', "\\").to_ascii_lowercase())
        .collect();
    item_paths(item).into_iter().any(|path| {
        let path = path.replace('/', "\\").to_ascii_lowercase();
        normalized.iter().any(|prefix| path.starts_with(prefix))
    })
}

fn searchable_fields(item: &DiffItem) -> Vec<String> {
    let mut fields = vec![item.key.clone(), item.display.clone()];
    for side in [&item.old, &item.new].into_iter().flatten() {
        match side {
            DiffSide::Archive(entry) => {
                fields.push(entry.path.clone());
                if let Some(ext) = extension_for_path(&entry.path) {
                    fields.push(ext);
                }
            }
            DiffSide::DataCore(record) => {
                fields.push(record.id.clone());
                fields.push(record.name.clone());
                fields.push(record.record_type.clone());
                fields.push(record.path.clone());
            }
        }
    }
    fields
}

fn archive_paths(item: &DiffItem) -> Vec<String> {
    [&item.old, &item.new]
        .into_iter()
        .flatten()
        .filter_map(|side| match side {
            DiffSide::Archive(entry) => Some(entry.path.clone()),
            DiffSide::DataCore(_) => None,
        })
        .collect()
}

fn item_paths(item: &DiffItem) -> Vec<String> {
    [&item.old, &item.new]
        .into_iter()
        .flatten()
        .map(|side| match side {
            DiffSide::Archive(entry) => entry.path.clone(),
            DiffSide::DataCore(record) => record.path.clone(),
        })
        .collect()
}

fn datacore_types(item: &DiffItem) -> Vec<String> {
    [&item.old, &item.new]
        .into_iter()
        .flatten()
        .filter_map(|side| match side {
            DiffSide::Archive(_) => None,
            DiffSide::DataCore(record) => Some(record.record_type.clone()),
        })
        .collect()
}
