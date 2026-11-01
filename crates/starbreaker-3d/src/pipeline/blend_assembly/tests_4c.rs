//! Phase 4C tests for validation helpers across lights, empties, and decals.

use super::*;

#[test]
fn test_extracted_light_has_use_temperature_field() {
    // Test 1: Verify ExtractedLight struct has use_temperature field
    let light = ExtractedLight {
        name: "Test".to_string(),
        parent_empty_name: None,
        parent_empty_parent_entity_name: None,
        parent_empty_parent_node_name: None,
        parent_empty_loc: [0.0, 0.0, 0.0],
        parent_empty_quat: [1.0, 0.0, 0.0, 0.0],
        parent_empty_scale: [1.0, 1.0, 1.0],
        position_blend: [0.0, 0.0, 0.0],
        rotation_blend: [1.0, 0.0, 0.0, 0.0],
        color: [1.0, 1.0, 1.0],
        lamp_type: 0,
        energy_watts: 1.0,
        radius: 5.0,
        cutoff_distance: 5.0,
        radius_source: 5.0,
        spot_size: 0.0,
        spot_blend: 0.0,
        temperature_k: 6500.0,
        use_temperature: true,  // Verify field exists and can be set
        gobo_path: None,
        active_state: "defaultState".to_string(),
            states_json: None,
            semantic_light_kind: "point".to_string(),
    };
    
    assert_eq!(light.temperature_k, 6500.0);
    assert_eq!(light.use_temperature, true);
}

#[test]
fn test_extracted_light_temperature_false() {
    // Test 2: use_temperature can be set to false
    let light = ExtractedLight {
        name: "Test".to_string(),
        parent_empty_name: None,
        parent_empty_parent_entity_name: None,
        parent_empty_parent_node_name: None,
        parent_empty_loc: [0.0, 0.0, 0.0],
        parent_empty_quat: [1.0, 0.0, 0.0, 0.0],
        parent_empty_scale: [1.0, 1.0, 1.0],
        position_blend: [0.0, 0.0, 0.0],
        rotation_blend: [1.0, 0.0, 0.0, 0.0],
        color: [1.0, 1.0, 1.0],
        lamp_type: 0,
        energy_watts: 1.0,
        radius: 5.0,
        cutoff_distance: 5.0,
        radius_source: 5.0,
        spot_size: 0.0,
        spot_blend: 0.0,
        temperature_k: 3000.0,
        use_temperature: false,  // Explicitly false
        gobo_path: None,
        active_state: "defaultState".to_string(),
            states_json: None,
            semantic_light_kind: "point".to_string(),
    };
    
    assert_eq!(light.temperature_k, 3000.0);
    assert_eq!(light.use_temperature, false);
}

#[test]
fn test_temperature_values_range() {
    // Test 3: Temperature values within typical Kelvin range
    let test_temps = vec![
        (2700.0, true, "Warm white"),
        (3000.0, false, "Warm incandescent"),
        (5000.0, true, "Mid-range"),
        (6500.0, false, "Daylight"),
        (9000.0, true, "Cool white"),
        (12000.0, false, "Very cool"),
    ];
    
    for (temp, use_temp, desc) in test_temps {
        let light = ExtractedLight {
            name: format!("Light_{}", temp as i32),
            parent_empty_name: None,
        parent_empty_parent_entity_name: None,
        parent_empty_parent_node_name: None,
            parent_empty_loc: [0.0, 0.0, 0.0],
            parent_empty_quat: [1.0, 0.0, 0.0, 0.0],
            parent_empty_scale: [1.0, 1.0, 1.0],
            position_blend: [0.0, 0.0, 0.0],
            rotation_blend: [1.0, 0.0, 0.0, 0.0],
            color: [1.0, 1.0, 1.0],
            lamp_type: 0,
            energy_watts: 1.0,
            radius: 5.0,
            cutoff_distance: 5.0,
            radius_source: 5.0,
            spot_size: 0.0,
            spot_blend: 0.0,
            temperature_k: temp,
            use_temperature: use_temp,
            gobo_path: None,
            active_state: "defaultState".to_string(),
            states_json: None,
            semantic_light_kind: "point".to_string(),
        };
        
        assert_eq!(light.temperature_k, temp, "Failed for {}: {}", temp, desc);
        assert_eq!(light.use_temperature, use_temp, "Failed for {}: {}", temp, desc);
    }
}
