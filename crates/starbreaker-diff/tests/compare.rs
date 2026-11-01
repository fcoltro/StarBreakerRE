//! Integration tests for report comparison, filtering and paging on synthetic
//! inventories.

use starbreaker_diff::compare::DiffStatus;
use starbreaker_diff::inventory::inventory_hash;
use starbreaker_diff::report::{
    ArchiveEntryInventory, DataCoreInventory, DataCoreRecordInventory, DataCoreStatus,
    HashAlgorithms, InventoryMode, InventoryReport, SourceInfo, Tier, INVENTORY_SCHEMA_VERSION,
};
use starbreaker_diff::{compare_report_page, compare_reports, DiffFilter};

fn report(
    label: &str,
    archive: Vec<ArchiveEntryInventory>,
    records: Vec<DataCoreRecordInventory>,
) -> InventoryReport {
    let mut report = InventoryReport {
        schema_version: INVENTORY_SCHEMA_VERSION,
        mode: InventoryMode::Full,
        generated_by: "test".to_string(),
        generated_at_unix: 1,
        hash_algorithms: HashAlgorithms::default(),
        source: SourceInfo {
            label: label.to_string(),
            source_file: None,
            build_manifest: None,
            warnings: Vec::new(),
        },
        archive,
        datacore: DataCoreInventory {
            source_path: Some("Data\\Game2.dcb".to_string()),
            status: DataCoreStatus::Present { records },
        },
        inventory_hash: String::new(),
    };
    report.inventory_hash = inventory_hash(&report).unwrap();
    report
}

fn archive(path: &str, crc32: u32, uncompressed_size: u64) -> ArchiveEntryInventory {
    ArchiveEntryInventory {
        path: path.to_string(),
        normalized_path: starbreaker_diff::report::normalize_archive_path(path),
        crc32,
        compressed_size: uncompressed_size / 2,
        uncompressed_size,
        compression_method: 100,
        encrypted: true,
        last_modified: 10,
    }
}

fn record(id: &str, name: &str, record_type: &str, hash: &str) -> DataCoreRecordInventory {
    DataCoreRecordInventory {
        id: id.to_string(),
        record_type: record_type.to_string(),
        name: name.to_string(),
        path: format!("libs/foundry/records/{name}.xml"),
        content_hash: hash.to_string(),
    }
}

#[test]
fn classifies_archive_and_datacore_changes() {
    let old = report(
        "old",
        vec![archive("Data\\a.dds", 1, 10), archive("Data\\b.xml", 2, 20)],
        vec![record("guid-a", "Alpha", "EntityClassDefinition", "hash-a")],
    );
    let new = report(
        "new",
        vec![archive("data/a.dds", 1, 10), archive("Data\\b.xml", 3, 20)],
        vec![record("guid-a", "AlphaRenamed", "EntityClassDefinition", "hash-a")],
    );

    let diff = compare_reports(&old, &new, true);

    let file_a = diff.items.iter().find(|item| item.key == "data\\a.dds").unwrap();
    assert_eq!(file_a.status, DiffStatus::MetadataChanged);
    assert!(file_a.reasons.contains(&"path_case_changed".to_string()));

    let file_b = diff.items.iter().find(|item| item.key == "data\\b.xml").unwrap();
    assert_eq!(file_b.status, DiffStatus::Modified);
    assert!(file_b.reasons.contains(&"crc32_changed".to_string()));

    let dc = diff.items.iter().find(|item| item.key == "guid-a").unwrap();
    assert_eq!(dc.status, DiffStatus::MetadataChanged);
    assert!(dc.reasons.contains(&"name_changed".to_string()));
}

#[test]
fn inventory_hash_excludes_label_and_generated_time() {
    let mut a = report("first", vec![archive("Data\\a.dds", 1, 10)], Vec::new());
    let mut b = a.clone();
    b.source.label = "second".to_string();
    b.generated_at_unix = 999;
    a.inventory_hash = inventory_hash(&a).unwrap();
    b.inventory_hash = inventory_hash(&b).unwrap();

    assert_eq!(a.inventory_hash, b.inventory_hash);

    b.archive[0].crc32 = 42;
    b.inventory_hash = inventory_hash(&b).unwrap();
    assert_ne!(a.inventory_hash, b.inventory_hash);
}

#[test]
fn filters_use_shared_semantics() {
    let old = report(
        "old",
        vec![archive("Data\\Textures\\a.dds", 1, 10)],
        vec![record("guid-a", "Aurora", "EntityClassDefinition", "hash-a")],
    );
    let new = report(
        "new",
        vec![archive("Data\\Textures\\a.dds", 2, 10)],
        vec![record("guid-a", "Aurora", "EntityClassDefinition", "hash-b")],
    );
    let diff = compare_reports(&old, &new, true);

    let filtered = starbreaker_diff::filter_diff_items(
        &diff.items,
        &DiffFilter {
            search: Some("aurora".to_string()),
            tiers: vec![Tier::DataCore],
            statuses: vec![DiffStatus::Modified],
            include_unchanged: true,
            ..DiffFilter::default()
        },
    );

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].tier, Tier::DataCore);
}

#[test]
fn summary_counts_unchanged_even_when_rows_are_omitted() {
    let old = report("old", vec![archive("Data\\a.dds", 1, 10)], Vec::new());
    let new = report("new", vec![archive("Data\\a.dds", 1, 10)], Vec::new());

    let diff = compare_reports(&old, &new, false);

    assert_eq!(diff.summary.unchanged, 1);
    assert!(diff.items.is_empty());
}

#[test]
fn paged_compare_counts_all_matches_but_returns_requested_window() {
    let old = report(
        "old",
        vec![
            archive("Data\\a.dds", 1, 10),
            archive("Data\\b.dds", 2, 20),
            archive("Data\\c.txt", 3, 30),
        ],
        Vec::new(),
    );
    let new = report(
        "new",
        vec![
            archive("Data\\a.dds", 9, 10),
            archive("Data\\b.dds", 8, 20),
            archive("Data\\c.txt", 7, 30),
        ],
        Vec::new(),
    );

    let page = compare_report_page(
        &old,
        &new,
        &DiffFilter {
            extensions: vec![".dds".to_string()],
            statuses: vec![DiffStatus::Modified],
            include_unchanged: false,
            ..DiffFilter::default()
        },
        1,
        1,
    );

    assert_eq!(page.summary.modified, 3);
    assert_eq!(page.total_matching, 2);
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].key, "data\\b.dds");
}
