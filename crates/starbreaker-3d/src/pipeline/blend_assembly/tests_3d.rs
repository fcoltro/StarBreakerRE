//! Phase 3D tests: Object block size invariant.

use super::*;

#[test]
fn test_validate_empty_object_creation() {
    assert_eq!(OBJECT_SIZE, 1288);
}
