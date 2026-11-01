use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use clap::Subcommand;
use starbreaker_datacore::database::Database;
use starbreaker_datacore::loadout::{EntityIndex, LoadoutNode, resolve_loadout_indexed};
use starbreaker_datacore::types::Record;
use starbreaker_p4k::MappedP4k;

use crate::common::{ExportOpts, load_dcb_bytes};
use crate::error::{CliError, Result};

fn bundled_extension(format: starbreaker_3d::ExportFormat) -> &'static str {
    match format {
        starbreaker_3d::ExportFormat::Glb => "glb",
        starbreaker_3d::ExportFormat::Stl => "stl",
        starbreaker_3d::ExportFormat::Blend => "blend",
    }
}

fn export_entity_name(name: &str) -> String {
    let trimmed = name.trim_matches('"');
    trimmed
        .rsplit('.')
        .next()
        .unwrap_or(trimmed)
        .to_string()
}

pub(crate) fn sanitize_export_name(name: &str) -> String {
    let mut cleaned = String::new();
    let mut last_was_space = false;

    for ch in name.chars() {
        if ch.is_alphanumeric() {
            cleaned.push(ch);
            last_was_space = false;
        } else if ch.is_whitespace() || matches!(ch, '_' | '-') {
            if !cleaned.is_empty() && !last_was_space {
                cleaned.push(' ');
                last_was_space = true;
            }
        }
    }

    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "Export".to_string()
    } else {
        cleaned.to_string()
    }
}

pub(crate) fn prepare_decomposed_output_root(output_root: &PathBuf, package_name: &str) -> Result<()> {
    if output_root.exists() {
        if output_root.is_file() {
            return Err(CliError::InvalidInput(format!(
                "decomposed output root '{}' already exists as a file",
                output_root.display(),
            )));
        }
    }

    let packages_root = output_root.join("Packages");
    let package_root = packages_root.join(package_name);
    std::fs::create_dir_all(&package_root)
        .map_err(|e| CliError::IoPath { source: e, path: package_root.display().to_string() })?;
    Ok(())
}

fn should_skip_existing_decomposed_asset(
    file: &starbreaker_3d::ExportedFile,
    skip_existing_assets: bool,
) -> bool {
    skip_existing_assets && file.kind.is_reusable_asset()
}

pub(crate) fn write_decomposed_file(
    file: &starbreaker_3d::ExportedFile,
    output_path: &PathBuf,
    skip_existing_assets: bool,
    p4k: &MappedP4k,
) -> Result<()> {
    if output_path.exists() {
        if !output_path.is_file() {
            return Err(CliError::InvalidInput(format!(
                "decomposed output path '{}' already exists as a directory",
                output_path.display(),
            )));
        }
        if should_skip_existing_decomposed_asset(file, skip_existing_assets)
            && existing_asset_matches_source_mtime(output_path, p4k, &file.relative_path)?
        {
            return Ok(());
        }
    }

    std::fs::write(output_path, &file.bytes)
        .map_err(|e| CliError::IoPath { source: e, path: output_path.display().to_string() })?;
    apply_source_mtime(output_path, p4k, &file.relative_path)?;
    Ok(())
}

fn should_write_ui_only_decomposed_file(relative_path: &str) -> bool {
    let normalized = relative_path.replace('\\', "/").to_ascii_lowercase();
    normalized.starts_with("data/ui/") || normalized.ends_with("/scene.json")
}

fn collect_existing_decomposed_assets(output_root: &Path, p4k: &MappedP4k) -> Result<HashSet<String>> {
    let data_root = output_root.join("Data");
    let mut existing = HashSet::new();
    if !data_root.exists() {
        return Ok(existing);
    }

    let mut pending = vec![data_root];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)
            .map_err(|e| CliError::IoPath { source: e, path: dir.display().to_string() })?
        {
            let entry = entry
                .map_err(|e| CliError::IoPath { source: e, path: dir.display().to_string() })?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|e| CliError::IoPath { source: e, path: path.display().to_string() })?;
            if file_type.is_dir() {
                pending.push(path);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }

            let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
                continue;
            };
            let filename = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
            if !matches!(extension, "blend" | "png" | "exr") && !filename.ends_with(".materials.json") {
                continue;
            }

            let relative = path
                .strip_prefix(output_root)
                .map_err(|_| {
                    CliError::InvalidInput(format!(
                        "failed to compute relative decomposed asset path for '{}'",
                        path.display(),
                    ))
                })?
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase();
            if !existing_asset_matches_source_mtime(&path, p4k, &relative)? {
                continue;
            }
            existing.insert(relative);
        }
    }

    Ok(existing)
}

fn existing_asset_matches_source_mtime(path: &Path, p4k: &MappedP4k, relative_path: &str) -> Result<bool> {
    let Some(source_seconds) = source_modified_unix_seconds(p4k, relative_path) else {
        return Ok(false);
    };
    let modified = path
        .metadata()
        .and_then(|metadata| metadata.modified())
        .map_err(|e| CliError::IoPath { source: e, path: path.display().to_string() })?;
    let Ok(asset_seconds) = modified.duration_since(UNIX_EPOCH) else {
        return Ok(false);
    };
    Ok(asset_seconds.as_secs() == source_seconds)
}

fn apply_source_mtime(path: &Path, p4k: &MappedP4k, relative_path: &str) -> Result<()> {
    let Some(seconds) = source_modified_unix_seconds(p4k, relative_path) else {
        return Ok(());
    };
    let file_time = filetime::FileTime::from_unix_time(seconds as i64, 0);
    filetime::set_file_mtime(path, file_time)
        .map_err(|e| CliError::IoPath { source: e, path: path.display().to_string() })
}

fn source_modified_unix_seconds(p4k: &MappedP4k, relative_path: &str) -> Option<u64> {
    source_asset_candidates(relative_path)
        .into_iter()
        .find_map(|candidate| p4k.entry_case_insensitive(&candidate).map(|entry| dos_datetime_to_unix_seconds(entry.last_modified)))
}

fn source_asset_candidates(relative_path: &str) -> Vec<String> {
    let path = relative_path.replace('/', "\\");
    if let Some(stem) = path.strip_suffix(".materials.json") {
        return vec![format!("{}.mtl", strip_numeric_suffix(stem, "_tex"))];
    }
    if let Some(stem) = path.strip_suffix(".blend") {
        let source_stem = strip_numeric_suffix(stem, "_lod");
        return [".cgfm", ".cgam", ".skinm", ".cgf", ".cga", ".skin", ".chr"]
            .into_iter()
            .map(|extension| format!("{source_stem}{extension}"))
            .collect();
    }
    if let Some(stem) = path.strip_suffix(".png") {
        let source_stem = strip_numeric_suffix(stem, "_tex");
        return [".dds", ".tif", ".png"]
            .into_iter()
            .map(|extension| format!("{source_stem}{extension}"))
            .collect();
    }
    if let Some(stem) = path.strip_suffix(".exr") {
        return [".dds", ".tif", ".exr"]
            .into_iter()
            .map(|extension| format!("{stem}{extension}"))
            .collect();
    }
    Vec::new()
}

fn strip_numeric_suffix<'a>(value: &'a str, marker: &str) -> &'a str {
    let lower = value.to_ascii_lowercase();
    let Some(pos) = lower.rfind(marker) else {
        return value;
    };
    let suffix = &value[pos + marker.len()..];
    if !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit()) {
        &value[..pos]
    } else {
        value
    }
}

fn dos_datetime_to_unix_seconds(value: u32) -> u64 {
    let time = value & 0xffff;
    let date = value >> 16;
    let second = (time & 0x1f) * 2;
    let minute = (time >> 5) & 0x3f;
    let hour = (time >> 11) & 0x1f;
    let day = (date & 0x1f).max(1);
    let month = ((date >> 5) & 0x0f).clamp(1, 12);
    let year = 1980 + ((date >> 9) & 0x7f) as i32;
    (days_from_civil(year, month, day) as u64) * 86_400
        + (hour as u64) * 3_600
        + (minute as u64) * 60
        + second as u64
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i32;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) as i64
}

#[cfg(test)]
mod tests {
    use super::{
        prepare_decomposed_output_root, should_write_ui_only_decomposed_file, source_asset_candidates,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn source_asset_candidates_strip_lod_and_tex_suffixes_case_insensitively() {
        assert_eq!(
            source_asset_candidates("data/objects/foo_LOD0.blend")[0],
            "data\\objects\\foo.cgfm"
        );
        assert_eq!(
            source_asset_candidates("data/textures/foo_tex0.png")[0],
            "data\\textures\\foo.dds"
        );
        assert_eq!(
            source_asset_candidates("data/materials/foo_TEX0.materials.json")[0],
            "data\\materials\\foo.mtl"
        );
    }

    #[test]
    fn prepare_decomposed_output_root_preserves_existing_packages() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("starbreaker_prepare_package_test_{unique}"));
        let target_package = root.join("Packages").join("ship").join("DRAK Buccaneer_LOD0_TEX0");
        let sibling_package = root.join("Packages").join("ship").join("DRAK Ironclad_LOD0_TEX0");
        std::fs::create_dir_all(&target_package).unwrap();
        std::fs::create_dir_all(&sibling_package).unwrap();
        std::fs::write(target_package.join("scene.json"), b"old").unwrap();
        std::fs::write(sibling_package.join("scene.json"), b"sibling").unwrap();

        prepare_decomposed_output_root(
            &root,
            "ship/DRAK Buccaneer_LOD0_TEX0",
        )
        .unwrap();

        assert_eq!(std::fs::read(target_package.join("scene.json")).unwrap(), b"old");
        assert_eq!(std::fs::read(sibling_package.join("scene.json")).unwrap(), b"sibling");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn ui_only_filter_includes_ui_assets_and_scene_sidecars() {
        assert!(should_write_ui_only_decomposed_file(
            "Data/UI/Generated/ship/drak/Clipper/buildingblocks_canvas_i_door_small_drak.png"
        ));
        assert!(should_write_ui_only_decomposed_file(
            "data/ui/generated/ship/drak/clipper/buildingblocks_canvas_i_door_small_drak.png"
        ));
        assert!(should_write_ui_only_decomposed_file(
            "Packages/DRAK Clipper_LOD0_TEX0/scene.json"
        ));
    }

    #[test]
    fn ui_only_filter_excludes_non_ui_assets() {
        assert!(!should_write_ui_only_decomposed_file(
            "Data/Objects/ships/drak/clipper/clipper_lod0.blend"
        ));
        assert!(!should_write_ui_only_decomposed_file(
            "Data/Textures/ships/drak/clipper/albedo.dds"
        ));
    }
}

fn collect_existing_interior_assets(output_root: &Path) -> Result<HashMap<String, (String, Option<String>)>> {
    let packages_root = output_root.join("Packages");
    let mut existing = HashMap::new();
    if !packages_root.exists() {
        return Ok(existing);
    }

    let mut pending = vec![packages_root];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)
            .map_err(|e| CliError::IoPath { source: e, path: dir.display().to_string() })?
        {
            let entry = entry
                .map_err(|e| CliError::IoPath { source: e, path: dir.display().to_string() })?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|e| CliError::IoPath { source: e, path: path.display().to_string() })?;
            if file_type.is_dir() {
                pending.push(path);
                continue;
            }
            if !file_type.is_file() || path.file_name().and_then(|name| name.to_str()) != Some("scene.json") {
                continue;
            }
            let bytes = std::fs::read(&path)
                .map_err(|e| CliError::IoPath { source: e, path: path.display().to_string() })?;
            collect_scene_interior_assets(&mut existing, &bytes);
        }
    }

    Ok(existing)
}

fn collect_scene_interior_assets(
    existing: &mut HashMap<String, (String, Option<String>)>,
    scene_bytes: &[u8],
) {
    let Ok(scene) = serde_json::from_slice::<serde_json::Value>(scene_bytes) else {
        return;
    };
    let Some(interiors) = scene.get("interiors").and_then(|value| value.as_array()) else {
        return;
    };
    for placement in interiors
        .iter()
        .filter_map(|interior| interior.get("placements").and_then(|value| value.as_array()))
        .flatten()
    {
        let Some(cgf_path) = placement.get("cgf_path").and_then(|value| value.as_str()) else {
            continue;
        };
        let material_path = placement.get("material_path").and_then(|value| value.as_str());
        let Some(mesh_asset) = placement.get("mesh_asset").and_then(|value| value.as_str()) else {
            continue;
        };
        let material_sidecar = placement
            .get("material_sidecar")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);
        let key = interior_asset_lookup_key(cgf_path, material_path);
        existing.entry(key).or_insert_with(|| (mesh_asset.to_string(), material_sidecar));
    }
}

fn interior_asset_lookup_key(cgf_path: &str, material_path: Option<&str>) -> String {
    format!(
        "{}|{}",
        cgf_path.to_ascii_lowercase(),
        material_path.unwrap_or("").to_ascii_lowercase()
    )
}

fn read_reusable_decomposed_asset(
    output_root: &Path,
    reusable_asset_paths: &HashSet<String>,
    relative_path: &str,
) -> Option<Vec<u8>> {
    let key = relative_path.replace('\\', "/").to_ascii_lowercase();
    if !reusable_asset_paths.contains(&key) {
        return None;
    }
    std::fs::read(output_root.join(relative_path)).ok()
}

#[derive(Subcommand)]
pub enum EntityCommand {
    /// Export entity to a bundled file
    Export {
        /// Entity name (substring, case-insensitive)
        name: String,
        /// Output bundled file path
        output: Option<PathBuf>,
        /// Path to Data.p4k
        #[arg(long, env = "SC_DATA_P4K")]
        p4k: Option<PathBuf>,
        /// Write hierarchy JSON instead of GLB
        #[arg(long)]
        dump_hierarchy: bool,
        #[command(flatten)]
        opts: ExportOpts,
    },
    /// Print entity loadout tree
    Loadout {
        /// Entity name (substring, case-insensitive)
        name: String,
        /// Path to Data.p4k
        #[arg(long, env = "SC_DATA_P4K")]
        p4k: Option<PathBuf>,
    },
    /// Validate an existing decomposed entity export directory
    Validate {
        /// Decomposed export root directory
        output: PathBuf,
    },
}

impl EntityCommand {
    pub fn run(self) -> Result<()> {
        match self {
            Self::Export {
                name,
                output,
                p4k,
                dump_hierarchy,
                opts,
            } => export(name, output, p4k, dump_hierarchy, opts),
            Self::Loadout { name, p4k } => loadout(name, p4k),
            Self::Validate { output } => validate(output),
        }
    }
}

fn validate(output: PathBuf) -> Result<()> {
    let report = starbreaker_3d::validate_decomposed_export(&output)?;
    println!("Valid:              {}", report.is_valid);
    println!("Meshes:             {}", report.mesh_count);
    println!("Lights:             {}", report.light_count);
    println!("Empties:            {}", report.empty_count);
    println!("Materials:          {}", report.material_count);
    println!("Vertex groups:      {}", report.vertex_group_count);
    println!("Warnings:           {}", report.warnings.len());
    println!("Errors:             {}", report.errors.len());

    if !report.metadata.is_empty() {
        println!();
        println!("Metadata:");
        let mut metadata: Vec<_> = report.metadata.iter().collect();
        metadata.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
        for (key, value) in metadata {
            println!("  {key}: {value}");
        }
    }

    if !report.warnings.is_empty() {
        println!();
        println!("Warnings:");
        for warning in &report.warnings {
            println!("  {warning}");
        }
    }

    if !report.errors.is_empty() {
        println!();
        println!("Errors:");
        for error in &report.errors {
            println!("  {error}");
        }
        return Err(CliError::InvalidInput(format!(
            "decomposed export validation failed: {} error(s)",
            report.errors.len()
        )));
    }

    Ok(())
}

fn find_candidates<'a>(db: &'a Database, search: &str) -> Result<Vec<&'a Record>> {
    let search = search.to_lowercase();
    let entity_si = db
        .struct_id("EntityClassDefinition")
        .ok_or_else(|| CliError::NotFound("EntityClassDefinition struct not found in DCB".into()))?;
    let mut candidates: Vec<_> = db
        .records_of_type(entity_si)
        .filter(|r| {
            db.resolve_string2(r.name_offset)
                .to_lowercase()
                .contains(&search)
        })
        .collect();
    candidates.sort_by_key(|r| db.resolve_string2(r.name_offset).len());
    Ok(candidates)
}

fn export(
    name: String,
    output: Option<PathBuf>,
    p4k_path: Option<PathBuf>,
    dump_hierarchy: bool,
    opts: ExportOpts,
) -> Result<()> {
    // Decomposed exports generate scene.blend with linked mesh instances.
    if opts.kind.to_lowercase() == "decomposed" {
        return export_blend(name, output, p4k_path, opts);
    }

    crate::log_mem_stats("start");
    let (p4k, dcb_bytes) = load_dcb_bytes(p4k_path.as_deref(), None)?;
    crate::log_mem_stats("after p4k+dcb load");
    let p4k = p4k.ok_or_else(|| CliError::MissingRequirement("P4k required for entity export".into()))?;
    let db = Database::from_bytes(&dcb_bytes)?;
    crate::log_mem_stats("after db parse");

    let candidates = find_candidates(&db, &name)?;
    if candidates.is_empty() {
        return Err(CliError::NotFound(format!("no EntityClassDefinition records matching '{name}'")));
    }

    let record = candidates[0];
    let rname = db.resolve_string2(record.name_offset);
    let export_name = sanitize_export_name(&export_entity_name(rname));
    if candidates.len() > 1 {
        eprintln!("Found {} candidates, using shortest match: {rname}", candidates.len());
    }

    let idx = EntityIndex::new(&db);
    let export_opts = starbreaker_3d::ExportOptions::from(&opts);
    let output = output.unwrap_or_else(|| {
        match export_opts.kind {
            starbreaker_3d::ExportKind::Bundled => {
                PathBuf::from(format!("{export_name}.{}", bundled_extension(export_opts.format)))
            }
            starbreaker_3d::ExportKind::Decomposed => PathBuf::from(name.clone()),
        }
    });
    let existing_asset_paths = if export_opts.kind == starbreaker_3d::ExportKind::Decomposed
        && opts.skip_existing_assets
    {
        Some(collect_existing_decomposed_assets(&output, &p4k)?)
    } else {
        None
    };
    let existing_interior_assets = if export_opts.kind == starbreaker_3d::ExportKind::Decomposed
        && opts.skip_existing_assets
    {
        Some(collect_existing_interior_assets(&output)?)
    } else {
        None
    };

    crate::log_mem_stats("before loadout resolve");
    let tree = resolve_loadout_indexed(&idx, record);
    crate::log_mem_stats("after loadout resolve");

    eprintln!("\nLoadout tree for {}:", tree.root.entity_name);
    for child in &tree.root.children {
        let g = if child.geometry_path.is_some() { "G" } else { "." };
        eprintln!("  {g} {} -> {}", child.item_port_name, child.entity_name);
    }

    if dump_hierarchy {
        let json = starbreaker_3d::dump_hierarchy(&db, &p4k, record, &tree);
        let json_path = output.with_extension("json");
        std::fs::write(&json_path, &json)
            .map_err(|e| CliError::IoPath { source: e, path: json_path.display().to_string() })?;
        eprintln!("Hierarchy written to {}", json_path.display());
        return Ok(());
    }

    crate::log_mem_stats("before export");
    let existing_asset_loader =
        |relative_path: &str| existing_asset_paths.as_ref().and_then(|paths| {
            read_reusable_decomposed_asset(&output, paths, relative_path)
        });
    let result = starbreaker_3d::assemble_glb_with_loadout_with_progress(
        &db,
        &p4k,
        record,
        &tree,
        &export_opts,
        None,
        existing_asset_paths.as_ref(),
        existing_interior_assets.as_ref(),
        Some(&existing_asset_loader),
    )?;
    crate::log_mem_stats("after export");
    eprintln!("Geometry: {}", result.geometry_path);
    eprintln!("Material: {}", result.material_path);
    match result.kind {
        starbreaker_3d::ExportKind::Bundled => {
            let bundled_bytes = result.bundled_bytes().ok_or_else(|| {
                CliError::InvalidInput(format!(
                    "entity export returned non-bundled output for {:?}",
                    result.kind,
                ))
            })?;
            eprintln!("Bundled export size: {} bytes", bundled_bytes.len());
            std::fs::write(&output, bundled_bytes)
                .map_err(|e| CliError::IoPath { source: e, path: output.display().to_string() })?;
        }
        starbreaker_3d::ExportKind::Decomposed => {
            let decomposed = result.decomposed.as_ref().ok_or_else(|| {
                CliError::InvalidInput("entity export returned no decomposed files".into())
            })?;
            eprintln!("Decomposed export file count: {}", decomposed.files.len());
            // The decomposed exporter names its package folder with a
            // `_LOD<n>_TEX<n>` suffix. Use that exact name here so we clean
            // the right directory and don't leave an empty sibling folder.
            let package_name = format!(
                "{export_name}_LOD{}_TEX{}",
                export_opts.lod_level, export_opts.texture_mip
            );
            prepare_decomposed_output_root(&output, &package_name)?;
            for file in &decomposed.files {
                if opts.ui_only_files && !should_write_ui_only_decomposed_file(&file.relative_path) {
                    continue;
                }
                let output_path = output.join(&file.relative_path);
                if let Some(parent) = output_path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| CliError::IoPath { source: e, path: parent.display().to_string() })?;
                }
                write_decomposed_file(file, &output_path, opts.skip_existing_assets, &p4k)?;
            }
        }
    }
    crate::log_mem_stats("after write");
    eprintln!("Written to {}", output.display());
    Ok(())
}

/// Export the root geometry of an entity directly to `.blend` format.
///
/// Resolves the entity's geometry path from DataCore, loads the `.skinm` from
/// the P4K, parses it via `starbreaker_3d::parse_skin`, and writes the result
/// as a Blender 5.x `.blend` library file containing a single OB_MESH object.
///
/// Phase 68 will extend this to handle the full node hierarchy (children,
/// lights, empties) and per-face material assignments.
fn export_blend(
    name: String,
    output: Option<PathBuf>,
    p4k_path: Option<PathBuf>,
    opts: ExportOpts,
) -> Result<()> {
    let (p4k, dcb_bytes) = load_dcb_bytes(p4k_path.as_deref(), None)?;
    let p4k = p4k.ok_or_else(|| CliError::MissingRequirement("P4k required for entity blend export".into()))?;
    let db = Database::from_bytes(&dcb_bytes)?;

    let candidates = find_candidates(&db, &name)?;
    if candidates.is_empty() {
        return Err(CliError::NotFound(format!("no EntityClassDefinition records matching '{name}'")));
    }
    let record = candidates[0];
    let rname = db.resolve_string2(record.name_offset);
    let export_name = sanitize_export_name(&export_entity_name(rname));
    if candidates.len() > 1 {
        eprintln!("Found {} candidates, using shortest match: {rname}", candidates.len());
    }

    let is_decomposed = opts.kind.to_lowercase() == "decomposed";

    if is_decomposed {
        // Blend decomposed export: keep the Rust-owned decomposed graph from
        // `starbreaker_3d`, then assemble `scene.blend` via a Rust-launched
        // background Blender process from the emitted `scene.json`.
        let idx = EntityIndex::new(&db);
        let tree = resolve_loadout_indexed(&idx, record);
        let mut export_opts = starbreaker_3d::ExportOptions::from(&opts);
        export_opts.kind = starbreaker_3d::ExportKind::Decomposed;

        let output_dir = output.unwrap_or_else(|| PathBuf::from(&name));
        let package_name = format!(
            "{export_name}_LOD{}_TEX{}",
            export_opts.lod_level, export_opts.texture_mip
        );
        let package_name = match &export_opts.decomposed_package_subdir {
            Some(subdir) => format!("{subdir}/{package_name}"),
            None => package_name,
        };

        let existing_asset_paths = if opts.skip_existing_assets {
            Some(collect_existing_decomposed_assets(&output_dir, &p4k)?)
        } else {
            None
        };
        let existing_interior_assets = if opts.skip_existing_assets {
            Some(collect_existing_interior_assets(&output_dir)?)
        } else {
            None
        };
        prepare_decomposed_output_root(&output_dir, &package_name)?;

        let existing_asset_loader =
            |relative_path: &str| existing_asset_paths.as_ref().and_then(|paths| {
                read_reusable_decomposed_asset(&output_dir, paths, relative_path)
            });
        let result = starbreaker_3d::assemble_glb_with_loadout_with_progress(
            &db,
            &p4k,
            record,
            &tree,
            &export_opts,
            None,
            existing_asset_paths.as_ref(),
            existing_interior_assets.as_ref(),
            Some(&existing_asset_loader),
        )?;

        let decomposed = result
            .decomposed
            .as_ref()
            .ok_or_else(|| CliError::InvalidInput("entity export returned no decomposed files".into()))?;

        for file in &decomposed.files {
            if opts.ui_only_files && !should_write_ui_only_decomposed_file(&file.relative_path) {
                continue;
            }
            let output_path = output_dir.join(&file.relative_path);
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| CliError::IoPath {
                    source: e,
                    path: parent.display().to_string(),
                })?;
            }
            write_decomposed_file(file, &output_path, opts.skip_existing_assets, &p4k)?;
        }

        let scene_json_path = output_dir.join("Packages").join(&package_name).join("scene.json");
        if !scene_json_path.is_file() {
            return Err(CliError::NotFound(format!(
                "decomposed scene manifest missing after export: {}",
                scene_json_path.display()
            )));
        }

        eprintln!("Written to {}", output_dir.display());
        return Ok(());
    }

    // Resolve geometry path from DataCore
    let geom_compiled = db
        .compile_path::<String>(
            record.struct_id(),
            "Components[SGeometryResourceParams].Geometry.Geometry.Geometry.path",
        )
        .map_err(|_| CliError::NotFound(format!("entity '{name}' has no geometry component")))?;
    let geometry_path = db
        .query_single::<String>(&geom_compiled, record)
        .map_err(|e| CliError::InvalidInput(format!("DataCore query failed: {e}")))?
        .ok_or_else(|| CliError::NotFound(format!("entity '{name}' has no geometry path")))?;

    // Derive the P4K path for the mesh data file (.skinm / .cgfm)
    // DataCore paths look like "objects/ships/aegs/gladius.skin"; strip Data/ prefix
    // and convert slashes, then append 'm' for the mashed geometry streams file.
    let clean_path = geometry_path
        .strip_prefix("Data/").or_else(|| geometry_path.strip_prefix("Data\\"))
        .or_else(|| geometry_path.strip_prefix("data/")).or_else(|| geometry_path.strip_prefix("data\\"))
        .unwrap_or(&geometry_path);
    let p4k_base = format!("Data\\{}", clean_path.replace('/', "\\"));
    let lod = opts.lod;
    let companion_path = if lod > 0 {
        let lod_path = if let Some(dot) = p4k_base.rfind('.') {
            format!("{}_lod{}{}", &p4k_base[..dot], lod, &p4k_base[dot..])
        } else {
            format!("{}_lod{}", p4k_base, lod)
        };
        let lod_companion = format!("{lod_path}m");
        if p4k.entry_case_insensitive(&lod_companion).is_some() {
            lod_companion
        } else {
            format!("{p4k_base}m")
        }
    } else {
        format!("{p4k_base}m")
    };

    let entry = p4k
        .entry_case_insensitive(&companion_path)
        .ok_or_else(|| CliError::NotFound(format!("mesh data not found in P4K: {companion_path}")))?;

    eprintln!("Loading mesh from: {}", entry.name);
    let mesh_bytes = p4k.read(entry)
        .map_err(|e| CliError::InvalidInput(format!("Failed to read mesh data: {e}")))?;

    // The metadata file (.skin / .cgf) holds the NMC and the bone table.
    let metadata = p4k.entry_case_insensitive(&p4k_base).and_then(|e| p4k.read(e).ok());

    // Bundled blend export: the shared .blend mesh writer, as decomposed exports use.
    let blend_bytes = starbreaker_3d::skin_to_blend(&export_name, &mesh_bytes, metadata.as_deref())
        .map_err(|e| CliError::InvalidInput(format!("Failed to convert mesh: {e}")))?;
    let output = output.unwrap_or_else(|| PathBuf::from(format!("{export_name}.blend")));
    std::fs::write(&output, &blend_bytes)
        .map_err(|e| CliError::IoPath { source: e, path: output.display().to_string() })?;
    eprintln!("Written {} bytes to {}", blend_bytes.len(), output.display());
    Ok(())
}

fn loadout(name: String, p4k_path: Option<PathBuf>) -> Result<()> {
    let (_, dcb_bytes) = load_dcb_bytes(p4k_path.as_deref(), None)?;
    let db = Database::from_bytes(&dcb_bytes)?;

    let candidates = find_candidates(&db, &name)?;
    if candidates.is_empty() {
        return Err(CliError::NotFound(format!("no EntityClassDefinition records matching '{name}'")));
    }

    let idx = EntityIndex::new(&db);
    for record in &candidates {
        let tree = resolve_loadout_indexed(&idx, record);
        print_loadout_node(&tree.root, 0);
    }
    Ok(())
}

fn print_loadout_node(node: &LoadoutNode, depth: usize) {
    let indent = "  ".repeat(depth);
    let geom = node.geometry_path.as_deref().unwrap_or("-");
    println!(
        "{indent}{} [{}] geom={geom}",
        node.entity_name, node.item_port_name
    );
    for child in &node.children {
        print_loadout_node(child, depth + 1);
    }
}
