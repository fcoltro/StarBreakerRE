//! Report comparison: `compare_reports` turns two `InventoryReport`s into a
//! `DiffReport` of `DiffItem`s (added / removed / modified, per `Tier`) with a
//! `DiffSummary`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::report::{
    normalize_archive_path, ArchiveEntryInventory, DataCoreRecordInventory, InventoryReport, Tier,
    DIFF_SCHEMA_VERSION,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    Added,
    Removed,
    Modified,
    MetadataChanged,
    Unchanged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffSide {
    Archive(ArchiveEntryInventory),
    DataCore(DataCoreRecordInventory),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffItem {
    pub tier: Tier,
    pub status: DiffStatus,
    pub key: String,
    pub display: String,
    pub old: Option<DiffSide>,
    pub new: Option<DiffSide>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiffSummary {
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
    pub metadata_changed: usize,
    pub unchanged: usize,
    pub p4k_items: usize,
    pub datacore_items: usize,
}

impl DiffSummary {
    pub(crate) fn add(&mut self, item: &DiffItem) {
        match item.status {
            DiffStatus::Added => self.added += 1,
            DiffStatus::Removed => self.removed += 1,
            DiffStatus::Modified => self.modified += 1,
            DiffStatus::MetadataChanged => self.metadata_changed += 1,
            DiffStatus::Unchanged => self.unchanged += 1,
        }
        match item.tier {
            Tier::P4k => self.p4k_items += 1,
            Tier::DataCore => self.datacore_items += 1,
        }
    }
}

pub fn compare_reports(
    old: &InventoryReport,
    new: &InventoryReport,
    include_unchanged: bool,
) -> crate::report::DiffReport {
    let mut items = Vec::new();
    items.extend(compare_archive(old, new));
    items.extend(compare_datacore(old, new));
    items.sort_by(|a, b| {
        status_rank(a.status)
            .cmp(&status_rank(b.status))
            .then_with(|| tier_rank(a.tier).cmp(&tier_rank(b.tier)))
            .then_with(|| a.display.cmp(&b.display))
            .then_with(|| a.key.cmp(&b.key))
    });

    let mut summary = DiffSummary::default();
    for item in &items {
        summary.add(item);
    }
    if !include_unchanged {
        items.retain(|item| item.status != DiffStatus::Unchanged);
    }

    crate::report::DiffReport {
        schema_version: DIFF_SCHEMA_VERSION,
        old_label: old.source.label.clone(),
        new_label: new.source.label.clone(),
        old_inventory_hash: old.inventory_hash.clone(),
        new_inventory_hash: new.inventory_hash.clone(),
        summary,
        items,
    }
}

fn compare_archive(
    old: &InventoryReport,
    new: &InventoryReport,
) -> Vec<DiffItem> {
    let old_map: BTreeMap<_, _> = old
        .archive
        .iter()
        .map(|entry| (entry.normalized_path.clone(), entry))
        .collect();
    let new_map: BTreeMap<_, _> = new
        .archive
        .iter()
        .map(|entry| (entry.normalized_path.clone(), entry))
        .collect();

    let mut keys: Vec<_> = old_map.keys().chain(new_map.keys()).cloned().collect();
    keys.sort();
    keys.dedup();

    let mut items = Vec::new();
    for key in keys {
        match (old_map.get(&key), new_map.get(&key)) {
            (None, Some(new_entry)) => items.push(archive_item(
                DiffStatus::Added,
                key,
                None,
                Some((*new_entry).clone()),
                Vec::new(),
            )),
            (Some(old_entry), None) => items.push(archive_item(
                DiffStatus::Removed,
                key,
                Some((*old_entry).clone()),
                None,
                Vec::new(),
            )),
            (Some(old_entry), Some(new_entry)) => {
                let mut reasons = Vec::new();
                if old_entry.crc32 != new_entry.crc32 {
                    reasons.push("crc32_changed".to_string());
                }
                if old_entry.uncompressed_size != new_entry.uncompressed_size {
                    reasons.push("uncompressed_size_changed".to_string());
                }
                let content_changed = !reasons.is_empty();

                if old_entry.compressed_size != new_entry.compressed_size {
                    reasons.push("compressed_size_changed".to_string());
                }
                if old_entry.compression_method != new_entry.compression_method {
                    reasons.push("compression_method_changed".to_string());
                }
                if old_entry.encrypted != new_entry.encrypted {
                    reasons.push("encrypted_changed".to_string());
                }
                if old_entry.last_modified != new_entry.last_modified {
                    reasons.push("last_modified_changed".to_string());
                }
                add_path_reasons(&mut reasons, &old_entry.path, &new_entry.path);

                let status = if content_changed {
                    DiffStatus::Modified
                } else if reasons.is_empty() {
                    DiffStatus::Unchanged
                } else {
                    DiffStatus::MetadataChanged
                };

                items.push(archive_item(
                    status,
                    key,
                    Some((*old_entry).clone()),
                    Some((*new_entry).clone()),
                    reasons,
                ));
            }
            (None, None) => {}
        }
    }
    items
}

pub(crate) fn archive_item_for_key(
    key: String,
    old_map: &BTreeMap<String, &ArchiveEntryInventory>,
    new_map: &BTreeMap<String, &ArchiveEntryInventory>,
) -> DiffItem {
    match (old_map.get(&key), new_map.get(&key)) {
        (None, Some(new_entry)) => archive_item(
            DiffStatus::Added,
            key,
            None,
            Some((*new_entry).clone()),
            Vec::new(),
        ),
        (Some(old_entry), None) => archive_item(
            DiffStatus::Removed,
            key,
            Some((*old_entry).clone()),
            None,
            Vec::new(),
        ),
        (Some(old_entry), Some(new_entry)) => {
            let mut reasons = Vec::new();
            if old_entry.crc32 != new_entry.crc32 {
                reasons.push("crc32_changed".to_string());
            }
            if old_entry.uncompressed_size != new_entry.uncompressed_size {
                reasons.push("uncompressed_size_changed".to_string());
            }
            let content_changed = !reasons.is_empty();

            if old_entry.compressed_size != new_entry.compressed_size {
                reasons.push("compressed_size_changed".to_string());
            }
            if old_entry.compression_method != new_entry.compression_method {
                reasons.push("compression_method_changed".to_string());
            }
            if old_entry.encrypted != new_entry.encrypted {
                reasons.push("encrypted_changed".to_string());
            }
            if old_entry.last_modified != new_entry.last_modified {
                reasons.push("last_modified_changed".to_string());
            }
            add_path_reasons(&mut reasons, &old_entry.path, &new_entry.path);

            let status = if content_changed {
                DiffStatus::Modified
            } else if reasons.is_empty() {
                DiffStatus::Unchanged
            } else {
                DiffStatus::MetadataChanged
            };

            archive_item(
                status,
                key,
                Some((*old_entry).clone()),
                Some((*new_entry).clone()),
                reasons,
            )
        }
        (None, None) => archive_item(DiffStatus::Unchanged, key, None, None, Vec::new()),
    }
}

fn compare_datacore(
    old: &InventoryReport,
    new: &InventoryReport,
) -> Vec<DiffItem> {
    let old_map: BTreeMap<_, _> = old
        .datacore
        .status
        .records()
        .iter()
        .map(|record| (record.id.clone(), record))
        .collect();
    let new_map: BTreeMap<_, _> = new
        .datacore
        .status
        .records()
        .iter()
        .map(|record| (record.id.clone(), record))
        .collect();

    let mut keys: Vec<_> = old_map.keys().chain(new_map.keys()).cloned().collect();
    keys.sort();
    keys.dedup();

    let mut items = Vec::new();
    for key in keys {
        match (old_map.get(&key), new_map.get(&key)) {
            (None, Some(new_record)) => items.push(datacore_item(
                DiffStatus::Added,
                key,
                None,
                Some((*new_record).clone()),
                Vec::new(),
            )),
            (Some(old_record), None) => items.push(datacore_item(
                DiffStatus::Removed,
                key,
                Some((*old_record).clone()),
                None,
                Vec::new(),
            )),
            (Some(old_record), Some(new_record)) => {
                let mut reasons = Vec::new();
                if old_record.record_type != new_record.record_type {
                    reasons.push("type_changed".to_string());
                }
                if old_record.content_hash != new_record.content_hash {
                    reasons.push("content_hash_changed".to_string());
                }
                let content_changed = reasons
                    .iter()
                    .any(|reason| reason == "type_changed" || reason == "content_hash_changed");

                if old_record.name != new_record.name {
                    reasons.push("name_changed".to_string());
                }
                if old_record.path != new_record.path {
                    reasons.push("path_changed".to_string());
                }

                let status = if content_changed {
                    DiffStatus::Modified
                } else if reasons.is_empty() {
                    DiffStatus::Unchanged
                } else {
                    DiffStatus::MetadataChanged
                };

                items.push(datacore_item(
                    status,
                    key,
                    Some((*old_record).clone()),
                    Some((*new_record).clone()),
                    reasons,
                ));
            }
            (None, None) => {}
        }
    }
    items
}

pub(crate) fn datacore_item_for_key(
    key: String,
    old_map: &BTreeMap<String, &DataCoreRecordInventory>,
    new_map: &BTreeMap<String, &DataCoreRecordInventory>,
) -> DiffItem {
    match (old_map.get(&key), new_map.get(&key)) {
        (None, Some(new_record)) => datacore_item(
            DiffStatus::Added,
            key,
            None,
            Some((*new_record).clone()),
            Vec::new(),
        ),
        (Some(old_record), None) => datacore_item(
            DiffStatus::Removed,
            key,
            Some((*old_record).clone()),
            None,
            Vec::new(),
        ),
        (Some(old_record), Some(new_record)) => {
            let mut reasons = Vec::new();
            if old_record.record_type != new_record.record_type {
                reasons.push("type_changed".to_string());
            }
            if old_record.content_hash != new_record.content_hash {
                reasons.push("content_hash_changed".to_string());
            }
            let content_changed = reasons
                .iter()
                .any(|reason| reason == "type_changed" || reason == "content_hash_changed");

            if old_record.name != new_record.name {
                reasons.push("name_changed".to_string());
            }
            if old_record.path != new_record.path {
                reasons.push("path_changed".to_string());
            }

            let status = if content_changed {
                DiffStatus::Modified
            } else if reasons.is_empty() {
                DiffStatus::Unchanged
            } else {
                DiffStatus::MetadataChanged
            };

            datacore_item(
                status,
                key,
                Some((*old_record).clone()),
                Some((*new_record).clone()),
                reasons,
            )
        }
        (None, None) => datacore_item(DiffStatus::Unchanged, key, None, None, Vec::new()),
    }
}

fn archive_item(
    status: DiffStatus,
    key: String,
    old: Option<ArchiveEntryInventory>,
    new: Option<ArchiveEntryInventory>,
    reasons: Vec<String>,
) -> DiffItem {
    let display = new
        .as_ref()
        .or(old.as_ref())
        .map(|entry| entry.path.clone())
        .unwrap_or_else(|| key.clone());
    DiffItem {
        tier: Tier::P4k,
        status,
        key,
        display,
        old: old.map(DiffSide::Archive),
        new: new.map(DiffSide::Archive),
        reasons,
    }
}

fn datacore_item(
    status: DiffStatus,
    key: String,
    old: Option<DataCoreRecordInventory>,
    new: Option<DataCoreRecordInventory>,
    reasons: Vec<String>,
) -> DiffItem {
    let display = new
        .as_ref()
        .or(old.as_ref())
        .map(|record| {
            if record.name.is_empty() {
                record.id.clone()
            } else {
                record.name.clone()
            }
        })
        .unwrap_or_else(|| key.clone());
    DiffItem {
        tier: Tier::DataCore,
        status,
        key,
        display,
        old: old.map(DiffSide::DataCore),
        new: new.map(DiffSide::DataCore),
        reasons,
    }
}

fn add_path_reasons(reasons: &mut Vec<String>, old: &str, new: &str) {
    if old == new {
        return;
    }
    if normalize_archive_path(old) == normalize_archive_path(new) {
        if old.replace('/', "\\") != new.replace('/', "\\") {
            reasons.push("path_separator_changed".to_string());
        }
        if old.replace('/', "\\").to_ascii_lowercase() == new.replace('/', "\\").to_ascii_lowercase()
            && old.replace('/', "\\") != new.replace('/', "\\")
        {
            reasons.push("path_case_changed".to_string());
        }
    }
}

fn status_rank(status: DiffStatus) -> u8 {
    match status {
        DiffStatus::Added => 0,
        DiffStatus::Removed => 1,
        DiffStatus::Modified => 2,
        DiffStatus::MetadataChanged => 3,
        DiffStatus::Unchanged => 4,
    }
}

fn tier_rank(tier: Tier) -> u8 {
    match tier {
        Tier::P4k => 0,
        Tier::DataCore => 1,
    }
}
