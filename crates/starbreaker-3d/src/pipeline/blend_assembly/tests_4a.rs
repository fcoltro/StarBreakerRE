//! Phase 4A tests for decal material identification.

use super::*;

#[test]
fn test_identify_decal_material_flags_decal() {
    let (is_decal, is_pom) =
        identify_decal_material_flags("Illum", "Shader { %DECAL %VERTEX_COLORS }", &[]);
    assert!(is_decal);
    assert!(!is_pom);
}

#[test]
fn test_identify_decal_material_flags_pom() {
    let (is_decal, is_pom) =
        identify_decal_material_flags("Illum", "Shader { %PARALLAX %VERTEX_COLORS }", &[]);
    assert!(!is_decal);
    assert!(!is_pom);
}

#[test]
fn test_identify_decal_material_flags_pom_alt() {
    let (is_decal, is_pom) = identify_decal_material_flags("Illum", "Shader { %POM }", &[]);
    assert!(!is_decal);
    assert!(!is_pom);
}

#[test]
fn test_identify_decal_material_flags_both() {
    let (is_decal, is_pom) =
        identify_decal_material_flags("Illum", "Shader { %DECAL %POM }", &[]);
    assert!(is_decal);
    assert!(is_pom);
}

#[test]
fn test_identify_decal_material_flags_neither() {
    let (is_decal, is_pom) =
        identify_decal_material_flags("Illum", "Shader { %VERTEX_COLORS %NORMAL_MAP }", &[]);
    assert!(!is_decal);
    assert!(!is_pom);
}

#[test]
fn test_identify_decal_material_flags_mesh_decal_shader() {
    let (is_decal, is_pom) =
        identify_decal_material_flags("MeshDecal", "%DIFFUSE_MAP%VERTDATA", &[]);
    assert!(is_decal);
    assert!(!is_pom);
}

#[test]
fn test_identify_decal_material_flags_mesh_decal_without_projection_signal() {
    let (is_decal, is_pom) = identify_decal_material_flags(
        "MeshDecal",
        "%DIFFUSE_MAP%USE_DAMAGE_MAP",
        &["DecalDiffuseOpacity".to_string()],
    );
    assert!(!is_decal);
    assert!(!is_pom);
}

#[test]
fn test_identify_decal_material_flags_glass_shader_is_excluded() {
    let (is_decal, is_pom) = identify_decal_material_flags(
        "GlassPBR",
        "%DECAL%DIRT%REFRACTION",
        &["DecalSmoothness".to_string()],
    );
    assert!(!is_decal);
    assert!(!is_pom);
}

#[test]
fn test_identify_decal_material_flags_parallax_occlusion_mapping() {
    let (is_decal, is_pom) =
        identify_decal_material_flags("MeshDecal", "%PARALLAX_OCCLUSION_MAPPING", &[]);
    assert!(is_decal);
    assert!(is_pom);
}

#[test]
fn test_identify_meshes_with_decals_single() {
    let mesh_materials = vec![
        (
            "Mesh_001".to_string(),
            vec![
                ("Base_Material".to_string(), "Illum".to_string(), "Shader { %VERTEX_COLORS }".to_string(), vec![]),
                ("Decal_Glass".to_string(), "Illum".to_string(), "Shader { %DECAL }".to_string(), vec![]),
                ("Metal".to_string(), "Illum".to_string(), "Shader { %METALLIC }".to_string(), vec![]),
            ],
        ),
    ];
    
    let result = identify_meshes_with_decals(&mesh_materials).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].mesh_path, "Mesh_001");
    assert_eq!(result[0].decal_materials.len(), 1);
    assert_eq!(result[0].decal_materials[0].material_index, 1);
}

#[test]
fn test_identify_meshes_with_decals_multiple() {
    let mesh_materials = vec![
        (
            "Mesh_A".to_string(),
            vec![
                ("Decal_001".to_string(), "Illum".to_string(), "Shader { %DECAL }".to_string(), vec![]),
                ("Normal_Mat".to_string(), "Illum".to_string(), "Shader { %VERTEX_COLORS }".to_string(), vec![]),
            ],
        ),
        (
            "Mesh_B".to_string(),
            vec![
                ("POM_Rock".to_string(), "Illum".to_string(), "Shader { %POM }".to_string(), vec![]),
                ("Decal_002".to_string(), "Illum".to_string(), "Shader { %DECAL %PARALLAX }".to_string(), vec![]),
            ],
        ),
        (
            "Mesh_C".to_string(),
            vec![
                ("Base".to_string(), "Illum".to_string(), "Shader { %VERTEX_COLORS }".to_string(), vec![]),
                ("Diffuse".to_string(), "Illum".to_string(), "Shader { %NORMAL_MAP }".to_string(), vec![]),
            ],
        ),
    ];
    
    let result = identify_meshes_with_decals(&mesh_materials).unwrap();
    assert_eq!(result.len(), 2);  // Only Mesh_A and Mesh_B have decals
    assert_eq!(result[0].decal_materials.len(), 1);
    assert_eq!(result[1].decal_materials.len(), 1);
}

#[test]
fn test_identify_meshes_with_decals_none() {
    let mesh_materials = vec![
        (
            "Mesh_001".to_string(),
            vec![
                ("Material_A".to_string(), "Illum".to_string(), "Shader { %VERTEX_COLORS }".to_string(), vec![]),
                ("Material_B".to_string(), "Illum".to_string(), "Shader { %NORMAL_MAP }".to_string(), vec![]),
            ],
        ),
    ];
    
    let result = identify_meshes_with_decals(&mesh_materials).unwrap();
    assert_eq!(result.len(), 0);
}
