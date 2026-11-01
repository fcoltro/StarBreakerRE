//! Serialized report schema: `InventoryReport`, `DiffReport` and their parts,
//! schema-version constants, path normalisation, and JSON read/write helpers.

use serde::{Deserialize, Serialize};

use crate::error::Result;

pub const INVENTORY_SCHEMA_VERSION: u32 = 1;
pub const DIFF_SCHEMA_VERSION: u32 = 1;
pub const INVENTORY_EXTENSION: &str = ".starbreaker-inventory.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryMode {
    Full,
    P4kOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    P4k,
    DataCore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashAlgorithms {
    pub inventory_hash: String,
    pub archive_identity: String,
    pub datacore_hash: String,
}

impl Default for HashAlgorithms {
    fn default() -> Self {
        Self {
            inventory_hash: "blake3-inventory-v1".to_string(),
            archive_identity: "zip-crc32-size-v1".to_string(),
            datacore_hash: "starbreaker-dcb-canonical-json-v1".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFileInfo {
    pub path_hint: String,
    pub size: Option<u64>,
    pub modified_unix: Option<u64>,
    pub entry_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildManifestInfo {
    pub branch: Option<String>,
    pub version: Option<String>,
    pub requested_p4_change_num: Option<u64>,
    pub channel_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    pub label: String,
    pub source_file: Option<SourceFileInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_manifest: Option<BuildManifestInfo>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveEntryInventory {
    pub path: String,
    pub normalized_path: String,
    pub crc32: u32,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub compression_method: u16,
    pub encrypted: bool,
    pub last_modified: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataCoreRecordInventory {
    pub id: String,
    pub record_type: String,
    pub name: String,
    pub path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum DataCoreStatus {
    Present { records: Vec<DataCoreRecordInventory> },
    Skipped,
}

impl DataCoreStatus {
    pub fn records(&self) -> &[DataCoreRecordInventory] {
        match self {
            Self::Present { records } => records,
            Self::Skipped => &[],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataCoreInventory {
    pub source_path: Option<String>,
    #[serde(flatten)]
    pub status: DataCoreStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryReport {
    pub schema_version: u32,
    pub mode: InventoryMode,
    pub generated_by: String,
    pub generated_at_unix: u64,
    pub hash_algorithms: HashAlgorithms,
    pub source: SourceInfo,
    pub archive: Vec<ArchiveEntryInventory>,
    pub datacore: DataCoreInventory,
    pub inventory_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffReport {
    pub schema_version: u32,
    pub old_label: String,
    pub new_label: String,
    pub old_inventory_hash: String,
    pub new_inventory_hash: String,
    pub summary: crate::compare::DiffSummary,
    pub items: Vec<crate::compare::DiffItem>,
}

pub fn normalize_archive_path(path: &str) -> String {
    path.replace('/', "\\")
        .trim_start_matches('\\')
        .to_ascii_lowercase()
}

pub fn extension_for_path(path: &str) -> Option<String> {
    let file_name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    file_name
        .rsplit_once('.')
        .map(|(_, ext)| format!(".{}", ext.to_ascii_lowercase()))
}

pub fn read_inventory_report(path: impl AsRef<std::path::Path>) -> Result<InventoryReport> {
    let bytes = std::fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn write_inventory_report(
    path: impl AsRef<std::path::Path>,
    report: &InventoryReport,
) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(report)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

pub fn read_diff_report(path: impl AsRef<std::path::Path>) -> Result<DiffReport> {
    let bytes = std::fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn write_diff_report(path: impl AsRef<std::path::Path>, report: &DiffReport) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(report)?;
    std::fs::write(path, bytes)?;
    Ok(())
}
