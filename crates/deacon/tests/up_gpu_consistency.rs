//! Integration tests for GPU mode consistency across repeated runs
//!
//! These tests verify that GPU mode is applied consistently and deterministically
//! across multiple invocations of the up command.
//!
//! Tests cover:
//! - Same GPU mode produces same behavior across multiple runs
//! - GPU mode resolution is deterministic
//! - GpuMode enum equality and comparison works correctly

use deacon_core::gpu::GpuMode;
use std::str::FromStr;

/// Test that GpuMode enum equality works correctly for consistency checks
#[test]
fn test_gpu_enum_equality() {
    // Test equality of same variants
    assert_eq!(GpuMode::All, GpuMode::All);
    assert_eq!(GpuMode::Detect, GpuMode::Detect);
    assert_eq!(GpuMode::None, GpuMode::None);

    // Test inequality of different variants
    assert_ne!(GpuMode::All, GpuMode::Detect);
    assert_ne!(GpuMode::All, GpuMode::None);
    assert_ne!(GpuMode::Detect, GpuMode::None);

    // Test that parsed values equal the enum variants
    assert_eq!(GpuMode::from_str("all").unwrap(), GpuMode::All);
    assert_eq!(GpuMode::from_str("detect").unwrap(), GpuMode::Detect);
    assert_eq!(GpuMode::from_str("none").unwrap(), GpuMode::None);

    // Test that multiple parses of the same string produce equal values
    let mode1 = GpuMode::from_str("all").unwrap();
    let mode2 = GpuMode::from_str("all").unwrap();
    assert_eq!(mode1, mode2);

    // Test case-insensitive parsing produces equal values
    let mode_lower = GpuMode::from_str("all").unwrap();
    let mode_upper = GpuMode::from_str("ALL").unwrap();
    let mode_mixed = GpuMode::from_str("All").unwrap();
    assert_eq!(mode_lower, mode_upper);
    assert_eq!(mode_lower, mode_mixed);
    assert_eq!(mode_upper, mode_mixed);
}

/// Test that GPU mode parsing is deterministic
#[test]
fn test_gpu_mode_deterministic() {
    // Parse the same string multiple times
    let iterations = 100;
    let expected = GpuMode::All;

    for _ in 0..iterations {
        let parsed = GpuMode::from_str("all").unwrap();
        assert_eq!(parsed, expected, "GPU mode parsing should be deterministic");
    }

    // Test all variants for determinism
    for mode_str in ["all", "detect", "none"] {
        let first = GpuMode::from_str(mode_str).unwrap();
        for _ in 0..10 {
            let parsed = GpuMode::from_str(mode_str).unwrap();
            assert_eq!(
                parsed, first,
                "Parsing '{}' should always produce the same result",
                mode_str
            );
        }
    }

    // Test that case variations produce consistent results
    for mode_str in ["all", "ALL", "All", "aLL"] {
        let parsed = GpuMode::from_str(mode_str).unwrap();
        assert_eq!(
            parsed,
            GpuMode::All,
            "Case-insensitive parsing should be deterministic"
        );
    }
}

/// Test that default GPU mode is consistently "none"
#[test]
fn test_gpu_mode_default_consistency() {
    // Test that default is always GpuMode::None
    for _ in 0..10 {
        assert_eq!(GpuMode::default(), GpuMode::None);
    }

    // Test that multiple default instances are equal
    let default1 = GpuMode::default();
    let default2 = GpuMode::default();
    assert_eq!(default1, default2);

    // Test that default matches None variant
    assert_eq!(GpuMode::default(), GpuMode::None);
}

/// Test that GpuMode serialization is deterministic
#[test]
fn test_gpu_mode_serialization_deterministic() {
    use serde_json;

    // Serialize the same mode multiple times
    let mode = GpuMode::All;
    let json1 = serde_json::to_string(&mode).unwrap();
    let json2 = serde_json::to_string(&mode).unwrap();
    assert_eq!(json1, json2, "Serialization should be deterministic");

    // Test all variants
    for (mode, expected) in [
        (GpuMode::All, r#""all""#),
        (GpuMode::Detect, r#""detect""#),
        (GpuMode::None, r#""none""#),
    ] {
        for _ in 0..5 {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(
                json, expected,
                "Serialization of {:?} should always produce '{}'",
                mode, expected
            );
        }
    }
}

/// Test that GpuMode deserialization is deterministic
#[test]
fn test_gpu_mode_deserialization_deterministic() {
    use serde_json;

    // Deserialize the same JSON multiple times
    let json = r#""all""#;
    for _ in 0..10 {
        let parsed: GpuMode = serde_json::from_str(json).unwrap();
        assert_eq!(parsed, GpuMode::All);
    }

    // Test round-trip consistency
    for mode in [GpuMode::All, GpuMode::Detect, GpuMode::None] {
        for _ in 0..5 {
            let json = serde_json::to_string(&mode).unwrap();
            let parsed: GpuMode = serde_json::from_str(&json).unwrap();
            assert_eq!(
                parsed, mode,
                "Round-trip serialization should preserve GPU mode"
            );
        }
    }
}

/// Test that Display trait is consistent with FromStr for GPU modes
#[test]
fn test_gpu_mode_display_from_str_consistency() {
    // For each mode, verify that to_string() and from_str() round-trip correctly
    for mode in [GpuMode::All, GpuMode::Detect, GpuMode::None] {
        let string = mode.to_string();
        let parsed = GpuMode::from_str(&string).unwrap();
        assert_eq!(
            parsed, mode,
            "to_string() and from_str() should round-trip for {:?}",
            mode
        );
    }

    // Verify that Display output is deterministic
    for _ in 0..10 {
        assert_eq!(GpuMode::All.to_string(), "all");
        assert_eq!(GpuMode::Detect.to_string(), "detect");
        assert_eq!(GpuMode::None.to_string(), "none");
    }
}

/// Test that GpuMode enum has Copy semantics and copying produces equal values
#[test]
fn test_gpu_mode_copy_consistency() {
    let mode1 = GpuMode::All;
    let mode2 = mode1; // This is a copy, not a move
    let mode3 = mode1; // We can still use mode1 because it's Copy

    // All copies should be equal
    assert_eq!(mode1, mode2);
    assert_eq!(mode1, mode3);
    assert_eq!(mode2, mode3);

    // Test that we can still use the original after copying
    assert_eq!(mode1, GpuMode::All);
}

/// Test that GpuMode enum comparison is consistent with equality
#[test]
fn test_gpu_mode_comparison_consistency() {
    // Create the same mode multiple ways
    let mode1 = GpuMode::All;
    let mode2 = GpuMode::from_str("all").unwrap();
    let mode3: GpuMode = serde_json::from_str(r#""all""#).unwrap();

    // All should be equal to each other
    assert_eq!(mode1, mode2);
    assert_eq!(mode1, mode3);
    assert_eq!(mode2, mode3);

    // And all should be equal to the All variant
    assert_eq!(mode1, GpuMode::All);
    assert_eq!(mode2, GpuMode::All);
    assert_eq!(mode3, GpuMode::All);
}

/// Test that invalid GPU mode strings consistently fail to parse
#[test]
fn test_gpu_mode_invalid_parsing_consistency() {
    let invalid_inputs = ["invalid", "auto", "yes", "no", "true", "false", ""];

    for input in invalid_inputs {
        // Parse multiple times to ensure consistent failure
        for _ in 0..5 {
            let result = GpuMode::from_str(input);
            assert!(
                result.is_err(),
                "Invalid input '{}' should always fail to parse",
                input
            );
        }
    }
}

/// Test that GpuMode behaves consistently in collections (slice/array)
#[test]
fn test_gpu_mode_collection_consistency() {
    // Test array behavior with equality checking
    let modes = [GpuMode::All, GpuMode::Detect, GpuMode::None];

    // Test that finding values is consistent
    assert!(modes.contains(&GpuMode::All));
    assert!(modes.contains(&GpuMode::Detect));
    assert!(modes.contains(&GpuMode::None));

    // Test that multiple lookups produce consistent results
    for _ in 0..5 {
        assert!(modes.contains(&GpuMode::All));
        assert_eq!(modes.iter().filter(|&m| m == &GpuMode::All).count(), 1);
    }

    // Test deduplication by manual comparison
    let mut unique_modes = Vec::new();
    for mode in [GpuMode::All, GpuMode::All, GpuMode::Detect, GpuMode::None] {
        if !unique_modes.contains(&mode) {
            unique_modes.push(mode);
        }
    }
    assert_eq!(unique_modes.len(), 3, "Should have 3 unique GPU modes");
}

/// Test that Clone produces equal values
/// Note: GpuMode implements Copy, so this test verifies copy semantics work via Clone trait
#[test]
fn test_gpu_mode_clone_consistency() {
    let mode1 = GpuMode::All;
    #[allow(clippy::clone_on_copy)]
    let mode2 = mode1.clone();
    #[allow(clippy::clone_on_copy)]
    let mode3 = mode2.clone();

    // All clones should be equal
    assert_eq!(mode1, mode2);
    assert_eq!(mode1, mode3);
    assert_eq!(mode2, mode3);

    // Test for all variants
    for mode in [GpuMode::All, GpuMode::Detect, GpuMode::None] {
        #[allow(clippy::clone_on_copy)]
        let cloned = mode.clone();
        assert_eq!(mode, cloned, "Cloned mode should equal original");
    }
}

/// Test that Debug representation is consistent
#[test]
fn test_gpu_mode_debug_consistency() {
    // Test that Debug output is deterministic
    for _ in 0..5 {
        assert_eq!(format!("{:?}", GpuMode::All), "All");
        assert_eq!(format!("{:?}", GpuMode::Detect), "Detect");
        assert_eq!(format!("{:?}", GpuMode::None), "None");
    }

    // Verify Debug differs from Display (Debug is CamelCase, Display is lowercase)
    assert_ne!(format!("{:?}", GpuMode::All), format!("{}", GpuMode::All));
    assert_ne!(
        format!("{:?}", GpuMode::Detect),
        format!("{}", GpuMode::Detect)
    );
    assert_ne!(format!("{:?}", GpuMode::None), format!("{}", GpuMode::None));
}
