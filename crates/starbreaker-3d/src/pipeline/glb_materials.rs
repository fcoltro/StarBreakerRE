//! Bundled GLB material preparation beyond direct MTL texture decoding.
//!
//! Renders source-backed UI bindings into in-memory PNGs and maps each rendered
//! image to the matching screen submeshes by NMC helper ancestry. Public helpers
//! prepare scene bindings, specialize materials per screen, and hash UI variants.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use starbreaker_datacore::database::Database;
use starbreaker_p4k::MappedP4k;

use crate::mtl::{MtlFile, ShaderFamily};
use crate::nmc::NodeMeshCombo;
use crate::types::{EntityPayload, MaterialTextures, Mesh, UiBinding};

use super::{empty_material_textures, LoadedInteriors};

pub(crate) fn render_bundled_ui_textures(
    children: &mut [EntityPayload],
    interiors: &mut LoadedInteriors,
    db: &Database<'_>,
    p4k: &MappedP4k,
    texture_mip: u32,
    root_entity_name: &str,
) {
    let bindings = children
        .iter()
        .flat_map(|child| child.ui_bindings.iter())
        .chain(
            interiors
                .containers
                .iter()
                .flat_map(|container| container.placements.iter())
                .flat_map(|placement| placement.ui_bindings.iter()),
        )
        .collect::<Vec<_>>();
    if bindings.is_empty() {
        return;
    }

    let manufacturer = crate::decomposed::derive_manufacturer_id(root_entity_name);
    let ship_data = crate::ui_pipeline::UiShipData::derive(db, root_entity_name);
    let loc_data = crate::ui_pipeline::UiLocData::load(p4k);
    let defaults = crate::ui_pipeline::build_default_registry(&loc_data, &ship_data);
    let rendered = crate::decomposed::prerender_ui_bindings(
        &bindings,
        db,
        p4k,
        texture_mip,
        root_entity_name,
        manufacturer.as_deref(),
        &loc_data,
        &defaults,
        &ship_data,
    );
    drop(bindings);

    let attach = |binding: &mut UiBinding| {
        let key = crate::decomposed::ui_render_key(binding);
        match rendered.get(&key) {
            Some(Ok(png)) => binding.bundled_image_data = Some(png.clone()),
            Some(Err(error)) => log::warn!(
                "failed to render bundled UI texture for '{}': {error}",
                binding.helper_name.as_deref().unwrap_or(&binding.binding_kind),
            ),
            None => {}
        }
    };
    for child in children {
        for binding in &mut child.ui_bindings {
            attach(binding);
        }
    }
    for container in &mut interiors.containers {
        for placement in &mut container.placements {
            for binding in &mut placement.ui_bindings {
                attach(binding);
            }
        }
    }
}

pub(crate) fn bundled_ui_hash(bindings: &[UiBinding]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut has_image = false;
    for binding in bindings {
        let Some(image) = binding.bundled_image_data.as_ref() else {
            continue;
        };
        has_image = true;
        binding.helper_name.hash(&mut hasher);
        binding.binding_kind.hash(&mut hasher);
        image.hash(&mut hasher);
    }
    has_image.then(|| hasher.finish()).unwrap_or(0)
}

pub(crate) fn apply_bundled_ui_materials(
    mesh: &mut Mesh,
    materials: &mut Option<MtlFile>,
    textures: &mut Option<MaterialTextures>,
    nmc: Option<&NodeMeshCombo>,
    bindings: &[UiBinding],
) {
    let Some(materials) = materials.as_mut() else {
        return;
    };
    if !bindings.iter().any(|binding| binding.bundled_image_data.is_some()) {
        return;
    }

    let textures = textures.get_or_insert_with(|| empty_material_textures(0));
    resize_textures(textures, materials.materials.len());
    let original_material_count = materials.materials.len();
    let mut variants = HashMap::<(usize, u32), u32>::new();
    let mut assigned_submeshes = HashSet::new();

    for (binding_index, binding) in bindings.iter().enumerate() {
        let Some(png) = binding.bundled_image_data.as_ref() else {
            continue;
        };
        let target_node = binding.helper_name.as_deref().and_then(|helper| {
            nmc.and_then(|combo| {
                combo
                    .nodes
                    .iter()
                    .position(|node| node.name.eq_ignore_ascii_case(helper))
            })
        });

        for (submesh_index, submesh) in mesh.submeshes.iter_mut().enumerate() {
            if assigned_submeshes.contains(&submesh_index) {
                continue;
            }
            let original_id = submesh.material_id;
            let Some(source_material) = materials.materials.get(original_id as usize).cloned() else {
                continue;
            };
            if !is_ui_material(&source_material) {
                continue;
            }
            if let Some(target_node) = target_node {
                let Some(combo) = nmc else {
                    continue;
                };
                if !node_is_descendant_or_same(combo, submesh.node_parent_index as usize, target_node) {
                    continue;
                }
            } else if binding.helper_name.is_some() {
                continue;
            }

            let variant_key = (binding_index, original_id);
            let variant_id = if let Some(&variant_id) = variants.get(&variant_key) {
                variant_id
            } else {
                let mut variant = source_material.clone();
                let label = binding.helper_name.as_deref().unwrap_or(&binding.binding_kind);
                variant.name = format!("{}__ui_{label}", variant.name);
                let new_id = materials.materials.len() as u32;
                materials.materials.push(variant);
                append_texture_variant(textures, original_id as usize, png);
                variants.insert(variant_key, new_id);
                new_id
            };
            submesh.source_material_id.get_or_insert(original_id);
            submesh.material_id = variant_id;
            assigned_submeshes.insert(submesh_index);
        }
    }

    debug_assert!(materials.materials.len() >= original_material_count);
}

fn is_ui_material(material: &crate::mtl::SubMaterial) -> bool {
    matches!(
        material.shader_family(),
        ShaderFamily::DisplayScreen | ShaderFamily::Monitor | ShaderFamily::UiPlane
    ) || material.has_virtual_input("$RenderToTexture")
}

fn node_is_descendant_or_same(combo: &NodeMeshCombo, mut node: usize, target: usize) -> bool {
    for _ in 0..combo.nodes.len() {
        if node == target {
            return true;
        }
        let Some(parent) = combo.nodes.get(node).and_then(|node| node.parent_index) else {
            return false;
        };
        node = parent as usize;
    }
    false
}

fn resize_textures(textures: &mut MaterialTextures, len: usize) {
    textures.diffuse.resize_with(len, || None);
    textures.normal.resize_with(len, || None);
    textures.roughness.resize_with(len, || None);
    textures.emissive.resize_with(len, || None);
    textures.occlusion.resize_with(len, || None);
    textures.diffuse_transform.resize_with(len, || None);
    textures.normal_transform.resize_with(len, || None);
    textures.roughness_transform.resize_with(len, || None);
    textures.emissive_transform.resize_with(len, || None);
    textures.occlusion_transform.resize_with(len, || None);
    textures.bundled_fallbacks.resize_with(len, Vec::new);
}

fn append_texture_variant(textures: &mut MaterialTextures, source: usize, png: &[u8]) {
    textures.diffuse.push(Some(png.to_vec()));
    textures.normal.push(textures.normal.get(source).cloned().unwrap_or(None));
    textures.roughness.push(textures.roughness.get(source).cloned().unwrap_or(None));
    textures.emissive.push(Some(png.to_vec()));
    textures.occlusion.push(textures.occlusion.get(source).cloned().unwrap_or(None));
    textures.diffuse_transform.push(None);
    textures.normal_transform.push(textures.normal_transform.get(source).copied().unwrap_or(None));
    textures.roughness_transform.push(textures.roughness_transform.get(source).copied().unwrap_or(None));
    textures.emissive_transform.push(None);
    textures.occlusion_transform.push(textures.occlusion_transform.get(source).copied().unwrap_or(None));
    let mut fallbacks = textures.bundled_fallbacks.get(source).cloned().unwrap_or_default();
    if !fallbacks.iter().any(|fallback| fallback == "generated_ui") {
        fallbacks.push("generated_ui".to_string());
    }
    textures.bundled_fallbacks.push(fallbacks);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mtl::{MaterialSetAuthoredData, SubMaterial, TextureSlotBinding};
    use crate::types::SubMesh;

    fn ui_material() -> SubMaterial {
        SubMaterial {
            name: "screen".to_string(),
            shader: "DisplayScreen".to_string(),
            diffuse: [0.0, 0.0, 0.0],
            opacity: 1.0,
            alpha_test: 0.0,
            string_gen_mask: String::new(),
            is_nodraw: false,
            specular: [0.04, 0.04, 0.04],
            shininess: 0.0,
            emissive: [1.0, 1.0, 1.0],
            glow: 1.0,
            surface_type: String::new(),
            diffuse_tex: None,
            normal_tex: None,
            layers: Vec::new(),
            palette_tint: 0,
            texture_slots: vec![TextureSlotBinding {
                slot: "TexSlot9".to_string(),
                path: "$RenderToTexture".to_string(),
                is_virtual: true,
            }],
            public_params: Vec::new(),
            authored_attributes: Vec::new(),
            authored_textures: Vec::new(),
            authored_child_blocks: Vec::new(),
        }
    }

    #[test]
    fn generated_ui_specializes_only_the_target_helper_material() {
        let mut mesh = Mesh {
            positions: vec![[0.0, 0.0, 0.0]; 3],
            indices: vec![0, 1, 2, 0, 1, 2],
            uvs: None,
            secondary_uvs: None,
            normals: None,
            tangents: None,
            colors: None,
            submeshes: vec![
                SubMesh { material_name: None, material_id: 0, source_material_id: None, first_index: 0, num_indices: 3, first_vertex: 0, num_vertices: 3, node_parent_index: 0 },
                SubMesh { material_name: None, material_id: 0, source_material_id: None, first_index: 3, num_indices: 3, first_vertex: 0, num_vertices: 3, node_parent_index: 1 },
            ],
            model_min: [0.0; 3],
            model_max: [0.0; 3],
            scaling_min: [0.0; 3],
            scaling_max: [0.0; 3],
            skin: None,
            morph_targets: Vec::new(),
        };
        let mut materials = Some(MtlFile {
            materials: vec![ui_material()],
            source_path: None,
            paint_override: None,
            material_set: MaterialSetAuthoredData::default(),
        });
        let mut textures = None;
        let nmc = NodeMeshCombo {
            nodes: vec![
                crate::nmc::NmcNode { name: "screen_a".to_string(), parent_index: None, bone_to_world: [[0.0; 4]; 3], world_to_bone: [[0.0; 4]; 3], scale: [1.0; 3], geometry_type: 0, properties: HashMap::new() },
                crate::nmc::NmcNode { name: "screen_b".to_string(), parent_index: None, bone_to_world: [[0.0; 4]; 3], world_to_bone: [[0.0; 4]; 3], scale: [1.0; 3], geometry_type: 0, properties: HashMap::new() },
            ],
            material_indices: vec![0, 0],
        };
        let binding = UiBinding {
            helper_name: Some("screen_b".to_string()),
            bundled_image_data: Some(vec![1, 2, 3]),
            ..Default::default()
        };

        apply_bundled_ui_materials(&mut mesh, &mut materials, &mut textures, Some(&nmc), &[binding]);

        assert_eq!(mesh.submeshes[0].material_id, 0);
        assert_eq!(mesh.submeshes[1].material_id, 1);
        assert_eq!(materials.expect("materials").materials.len(), 2);
        assert_eq!(textures.expect("textures").diffuse[1], Some(vec![1, 2, 3]));
    }
}
