//! Paged comparison for the app's virtualised list: `compare_report_page`
//! returns one filtered `DiffPage` plus per-filter counts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::compare::{
    archive_item_for_key, datacore_item_for_key, DiffItem, DiffStatus, DiffSummary,
};
use crate::filter::{diff_item_matches_filter, DiffFilter};
use crate::report::{
    ArchiveEntryInventory, DataCoreRecordInventory, InventoryReport, Tier, DIFF_SCHEMA_VERSION,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffPage {
    pub schema_version: u32,
    pub old_label: String,
    pub new_label: String,
    pub old_inventory_hash: String,
    pub new_inventory_hash: String,
    pub summary: DiffSummary,
    pub total_matching: usize,
    pub offset: usize,
    pub limit: usize,
    pub items: Vec<DiffItem>,
}

pub fn compare_report_page(
    old: &InventoryReport,
    new: &InventoryReport,
    filter: &DiffFilter,
    offset: usize,
    limit: usize,
) -> DiffPage {
    let archive_old_map: BTreeMap<_, _> = old
        .archive
        .iter()
        .map(|entry| (entry.normalized_path.clone(), entry))
        .collect();
    let archive_new_map: BTreeMap<_, _> = new
        .archive
        .iter()
        .map(|entry| (entry.normalized_path.clone(), entry))
        .collect();
    let archive_keys = sorted_keys(&archive_old_map, &archive_new_map);

    let datacore_old_map: BTreeMap<_, _> = old
        .datacore
        .status
        .records()
        .iter()
        .map(|record| (record.id.clone(), record))
        .collect();
    let datacore_new_map: BTreeMap<_, _> = new
        .datacore
        .status
        .records()
        .iter()
        .map(|record| (record.id.clone(), record))
        .collect();
    let datacore_keys = sorted_keys(&datacore_old_map, &datacore_new_map);

    let summary = summarize(
        &archive_keys,
        &archive_old_map,
        &archive_new_map,
        &datacore_keys,
        &datacore_old_map,
        &datacore_new_map,
    );
    let (total_matching, items) = page_items(
        &archive_keys,
        &archive_old_map,
        &archive_new_map,
        &datacore_keys,
        &datacore_old_map,
        &datacore_new_map,
        filter,
        offset,
        limit.max(1),
    );

    DiffPage {
        schema_version: DIFF_SCHEMA_VERSION,
        old_label: old.source.label.clone(),
        new_label: new.source.label.clone(),
        old_inventory_hash: old.inventory_hash.clone(),
        new_inventory_hash: new.inventory_hash.clone(),
        summary,
        total_matching,
        offset,
        limit: limit.max(1),
        items,
    }
}

fn sorted_keys<T>(
    old_map: &BTreeMap<String, &T>,
    new_map: &BTreeMap<String, &T>,
) -> Vec<String> {
    let mut keys: Vec<_> = old_map.keys().chain(new_map.keys()).cloned().collect();
    keys.sort();
    keys.dedup();
    keys
}

fn summarize(
    archive_keys: &[String],
    archive_old_map: &BTreeMap<String, &ArchiveEntryInventory>,
    archive_new_map: &BTreeMap<String, &ArchiveEntryInventory>,
    datacore_keys: &[String],
    datacore_old_map: &BTreeMap<String, &DataCoreRecordInventory>,
    datacore_new_map: &BTreeMap<String, &DataCoreRecordInventory>,
) -> DiffSummary {
    let mut summary = DiffSummary::default();
    for key in archive_keys {
        summary.add(&archive_item_for_key(
            key.clone(),
            archive_old_map,
            archive_new_map,
        ));
    }
    for key in datacore_keys {
        summary.add(&datacore_item_for_key(
            key.clone(),
            datacore_old_map,
            datacore_new_map,
        ));
    }
    summary
}

fn page_items(
    archive_keys: &[String],
    archive_old_map: &BTreeMap<String, &ArchiveEntryInventory>,
    archive_new_map: &BTreeMap<String, &ArchiveEntryInventory>,
    datacore_keys: &[String],
    datacore_old_map: &BTreeMap<String, &DataCoreRecordInventory>,
    datacore_new_map: &BTreeMap<String, &DataCoreRecordInventory>,
    filter: &DiffFilter,
    offset: usize,
    limit: usize,
) -> (usize, Vec<DiffItem>) {
    let mut total_matching = 0usize;
    let mut items = Vec::new();
    let wanted_statuses = ordered_statuses(filter);
    let wanted_tiers = ordered_tiers(filter);

    for status in wanted_statuses {
        for tier in &wanted_tiers {
            match tier {
                Tier::P4k => page_archive_items(
                    status,
                    archive_keys,
                    archive_old_map,
                    archive_new_map,
                    filter,
                    offset,
                    limit,
                    &mut total_matching,
                    &mut items,
                ),
                Tier::DataCore => page_datacore_items(
                    status,
                    datacore_keys,
                    datacore_old_map,
                    datacore_new_map,
                    filter,
                    offset,
                    limit,
                    &mut total_matching,
                    &mut items,
                ),
            }
        }
    }

    (total_matching, items)
}

fn page_archive_items(
    status: DiffStatus,
    keys: &[String],
    old_map: &BTreeMap<String, &ArchiveEntryInventory>,
    new_map: &BTreeMap<String, &ArchiveEntryInventory>,
    filter: &DiffFilter,
    offset: usize,
    limit: usize,
    total_matching: &mut usize,
    items: &mut Vec<DiffItem>,
) {
    for key in keys {
        let item = archive_item_for_key(key.clone(), old_map, new_map);
        push_page_item(status, item, filter, offset, limit, total_matching, items);
    }
}

fn page_datacore_items(
    status: DiffStatus,
    keys: &[String],
    old_map: &BTreeMap<String, &DataCoreRecordInventory>,
    new_map: &BTreeMap<String, &DataCoreRecordInventory>,
    filter: &DiffFilter,
    offset: usize,
    limit: usize,
    total_matching: &mut usize,
    items: &mut Vec<DiffItem>,
) {
    for key in keys {
        let item = datacore_item_for_key(key.clone(), old_map, new_map);
        push_page_item(status, item, filter, offset, limit, total_matching, items);
    }
}

fn push_page_item(
    status: DiffStatus,
    item: DiffItem,
    filter: &DiffFilter,
    offset: usize,
    limit: usize,
    total_matching: &mut usize,
    items: &mut Vec<DiffItem>,
) {
    if item.status != status || !diff_item_matches_filter(&item, filter) {
        return;
    }
    if *total_matching >= offset && items.len() < limit {
        items.push(item);
    }
    *total_matching += 1;
}

fn ordered_statuses(filter: &DiffFilter) -> Vec<DiffStatus> {
    let statuses = [
        DiffStatus::Added,
        DiffStatus::Removed,
        DiffStatus::Modified,
        DiffStatus::MetadataChanged,
        DiffStatus::Unchanged,
    ];
    statuses
        .into_iter()
        .filter(|status| filter.statuses.is_empty() || filter.statuses.contains(status))
        .collect()
}

fn ordered_tiers(filter: &DiffFilter) -> Vec<Tier> {
    let tiers = [Tier::P4k, Tier::DataCore];
    tiers
        .into_iter()
        .filter(|tier| filter.tiers.is_empty() || filter.tiers.contains(tier))
        .collect()
}
