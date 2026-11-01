//! Inventory generation from a `Data.p4k`: hashes every archive entry and,
//! unless skipped, every DataCore record (`generate_inventory_from_p4k[_with_progress]`),
//! with cancellable progress events. Also `inventory_hash` for report identity.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use starbreaker_datacore::database::Database;
use starbreaker_p4k::MappedP4k;

use crate::canonical::canonical_record_hash;
use crate::error::{DiffError, Result};
use crate::report::{
    normalize_archive_path, ArchiveEntryInventory, BuildManifestInfo, DataCoreInventory,
    DataCoreRecordInventory, DataCoreStatus, HashAlgorithms, InventoryMode, InventoryReport,
    SourceFileInfo, SourceInfo, INVENTORY_SCHEMA_VERSION,
};

#[derive(Debug, Clone)]
pub struct InventoryOptions {
    pub skip_datacore: bool,
    pub label: Option<String>,
    pub generated_by: String,
}

impl Default for InventoryOptions {
    fn default() -> Self {
        Self {
            skip_datacore: false,
            label: None,
            generated_by: format!("starbreaker {}", env!("CARGO_PKG_VERSION")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressPhase {
    OpeningP4k,
    ReadingArchiveIndex,
    ReadingDataCore,
    HashingDataCoreRecords,
    FinalizingReport,
}

#[derive(Debug, Clone)]
pub struct ProgressEvent {
    pub phase: ProgressPhase,
    pub current: Option<usize>,
    pub total: Option<usize>,
    pub message: String,
}

pub fn generate_inventory_from_p4k(
    path: impl AsRef<Path>,
    options: &InventoryOptions,
) -> Result<InventoryReport> {
    generate_inventory_from_p4k_with_progress(path, options, None, None)
}

pub fn generate_inventory_from_p4k_with_progress(
    path: impl AsRef<Path>,
    options: &InventoryOptions,
    mut progress: Option<&mut dyn FnMut(ProgressEvent)>,
    cancel: Option<&AtomicBool>,
) -> Result<InventoryReport> {
    let path = path.as_ref();
    report_progress(
        &mut progress,
        ProgressPhase::OpeningP4k,
        None,
        None,
        format!("Opening {}", path.display()),
    );
    check_cancel(cancel)?;
    let p4k = MappedP4k::open(path)?;

    report_progress(
        &mut progress,
        ProgressPhase::ReadingArchiveIndex,
        Some(p4k.entries().len()),
        Some(p4k.entries().len()),
        "Reading P4k archive index".to_string(),
    );
    check_cancel(cancel)?;

    let mut archive: Vec<_> = p4k
        .entries()
        .iter()
        .map(|entry| ArchiveEntryInventory {
            path: entry.name.clone(),
            normalized_path: normalize_archive_path(&entry.name),
            crc32: entry.crc32,
            compressed_size: entry.compressed_size,
            uncompressed_size: entry.uncompressed_size,
            compression_method: entry.compression_method,
            encrypted: entry.is_encrypted,
            last_modified: entry.last_modified,
        })
        .collect();
    archive.sort_by(|a, b| a.normalized_path.cmp(&b.normalized_path));

    let mut warnings = Vec::new();
    let manifest = read_build_manifest(path, &mut warnings);
    let source_file = source_file_info(path, p4k.entries().len());
    let label = options
        .label
        .clone()
        .unwrap_or_else(|| default_label(path, source_file.as_ref(), manifest.as_ref()));

    let datacore = if options.skip_datacore {
        DataCoreInventory {
            source_path: None,
            status: DataCoreStatus::Skipped,
        }
    } else {
        report_progress(
            &mut progress,
            ProgressPhase::ReadingDataCore,
            None,
            None,
            "Reading DataCore".to_string(),
        );
        let (source_path, dcb_bytes) = read_datacore(&p4k)?;
        check_cancel(cancel)?;
        let db = Database::from_bytes(&dcb_bytes)?;
        let records: Vec<_> = db
            .records()
            .iter()
            .filter(|record| db.is_main_record(record))
            .collect();
        let total = records.len();
        let mut inventory = Vec::with_capacity(total);
        for (idx, record) in records.into_iter().enumerate() {
            if idx % 250 == 0 {
                check_cancel(cancel)?;
                report_progress(
                    &mut progress,
                    ProgressPhase::HashingDataCoreRecords,
                    Some(idx),
                    Some(total),
                    "Hashing DataCore records".to_string(),
                );
            }
            let record_type = db
                .resolve_string2(db.struct_def(record.struct_index).name_offset)
                .to_string();
            let name = db.resolve_string2(record.name_offset).to_string();
            let path = db.resolve_string(record.file_name_offset).to_string();
            let content_hash = canonical_record_hash(&db, record)?;
            inventory.push(DataCoreRecordInventory {
                id: record.id.to_string(),
                record_type,
                name,
                path,
                content_hash,
            });
        }
        report_progress(
            &mut progress,
            ProgressPhase::HashingDataCoreRecords,
            Some(total),
            Some(total),
            "Hashing DataCore records".to_string(),
        );
        inventory.sort_by(|a, b| a.id.cmp(&b.id));
        DataCoreInventory {
            source_path: Some(source_path),
            status: DataCoreStatus::Present { records: inventory },
        }
    };

    report_progress(
        &mut progress,
        ProgressPhase::FinalizingReport,
        None,
        None,
        "Finalizing inventory report".to_string(),
    );

    let mode = if options.skip_datacore {
        InventoryMode::P4kOnly
    } else {
        InventoryMode::Full
    };
    let mut report = InventoryReport {
        schema_version: INVENTORY_SCHEMA_VERSION,
        mode,
        generated_by: options.generated_by.clone(),
        generated_at_unix: now_unix(),
        hash_algorithms: HashAlgorithms::default(),
        source: SourceInfo {
            label,
            source_file,
            build_manifest: manifest,
            warnings,
        },
        archive,
        datacore,
        inventory_hash: String::new(),
    };
    report.inventory_hash = inventory_hash(&report)?;
    Ok(report)
}

pub fn inventory_hash(report: &InventoryReport) -> Result<String> {
    #[derive(serde::Serialize)]
    struct TechnicalInventory<'a> {
        schema_version: u32,
        mode: InventoryMode,
        hash_algorithms: &'a HashAlgorithms,
        archive: &'a [ArchiveEntryInventory],
        datacore: &'a DataCoreInventory,
    }

    let technical = TechnicalInventory {
        schema_version: report.schema_version,
        mode: report.mode,
        hash_algorithms: &report.hash_algorithms,
        archive: &report.archive,
        datacore: &report.datacore,
    };
    Ok(format!("blake3:{}", blake3::hash(&serde_json::to_vec(&technical)?)))
}

fn read_datacore(p4k: &MappedP4k) -> Result<(String, Vec<u8>)> {
    for path in ["Data\\Game2.dcb", "Data\\Game.dcb"] {
        match p4k.read_file(path) {
            Ok(bytes) => return Ok((path.to_string(), bytes)),
            Err(starbreaker_p4k::P4kError::EntryNotFound(_)) => {}
            Err(err) => return Err(err.into()),
        }
    }
    Err(DiffError::DataCoreNotFound)
}

fn source_file_info(path: &Path, entry_count: usize) -> Option<SourceFileInfo> {
    let metadata = fs::metadata(path).ok()?;
    Some(SourceFileInfo {
        path_hint: path.display().to_string(),
        size: Some(metadata.len()),
        modified_unix: metadata.modified().ok().and_then(system_time_unix),
        entry_count,
    })
}

fn read_build_manifest(path: &Path, warnings: &mut Vec<String>) -> Option<BuildManifestInfo> {
    let manifest_path = path.parent()?.join("build_manifest.id");
    let text = match fs::read_to_string(&manifest_path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => {
            warnings.push(format!(
                "Failed to read build_manifest.id at {}: {err}",
                manifest_path.display()
            ));
            return None;
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(err) => {
            warnings.push(format!(
                "Failed to parse build_manifest.id at {}: {err}",
                manifest_path.display()
            ));
            return None;
        }
    };
    let data = &value["Data"];
    let branch = data
        .get("Branch")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned);
    let version = branch.as_deref().and_then(parse_version);
    let requested_p4_change_num = data
        .get("RequestedP4ChangeNum")
        .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()));
    let channel_hint = path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned);
    Some(BuildManifestInfo {
        branch,
        version,
        requested_p4_change_num,
        channel_hint,
    })
}

fn parse_version(branch: &str) -> Option<String> {
    let bytes = branch.as_bytes();
    for start in 0..bytes.len() {
        if !bytes[start].is_ascii_digit() {
            continue;
        }
        let rest = &branch[start..];
        let mut parts = rest.splitn(4, '.');
        let a = parts.next()?;
        let b = parts.next()?;
        let c_rest = parts.next()?;
        if a.is_empty()
            || b.is_empty()
            || !a.bytes().all(|b| b.is_ascii_digit())
            || !b.bytes().all(|b| b.is_ascii_digit())
        {
            continue;
        }
        let c: String = c_rest
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect();
        if c.is_empty() {
            continue;
        }
        return Some(format!("{a}.{b}.{c}"));
    }
    None
}

fn default_label(
    path: &Path,
    source_file: Option<&SourceFileInfo>,
    manifest: Option<&BuildManifestInfo>,
) -> String {
    if let Some(manifest) = manifest {
        if let Some(version) = &manifest.version {
            let channel = manifest.channel_hint.as_deref().unwrap_or("SC");
            if let Some(build) = manifest.requested_p4_change_num {
                return format!("{version} {channel}.{build}");
            }
            return format!("{version} {channel}");
        }
        if let Some(branch) = &manifest.branch {
            return branch.clone();
        }
    }

    if let Some(parent) = path.parent().and_then(|parent| parent.file_name()).and_then(|s| s.to_str())
        && let Some(source_file) = source_file
        && let Some(modified) = source_file.modified_unix
    {
        return format!("{parent} Data.p4k {modified}");
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Data.p4k")
        .to_string()
}

fn check_cancel(cancel: Option<&AtomicBool>) -> Result<()> {
    if cancel.is_some_and(|cancel| cancel.load(Ordering::Relaxed)) {
        Err(DiffError::Cancelled)
    } else {
        Ok(())
    }
}

fn report_progress(
    progress: &mut Option<&mut dyn FnMut(ProgressEvent)>,
    phase: ProgressPhase,
    current: Option<usize>,
    total: Option<usize>,
    message: String,
) {
    if let Some(progress) = progress {
        progress(ProgressEvent {
            phase,
            current,
            total,
            message,
        });
    }
}

fn now_unix() -> u64 {
    system_time_unix(SystemTime::now()).unwrap_or(0)
}

fn system_time_unix(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH).ok().map(|duration| duration.as_secs())
}
