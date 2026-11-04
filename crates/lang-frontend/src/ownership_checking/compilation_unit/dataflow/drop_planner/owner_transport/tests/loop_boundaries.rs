//! Rejection must leave the entire original planner product unchanged, not just counts.
use super::*;
use crate::ownership_checking::compilation_unit::owner_transport_tests::loop_boundaries::{
    LOOPS, source,
};

#[test]
fn loop_rejection_preserves_complete_legacy_cleanup_analysis() {
    for control in LOOPS {
        let text = source(control);
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let shared = fixture.shared();
        assert!(shared.clone().validate().is_ok(), "{control}");
        let parts = fixture.parts(&shared);
        compare(&fixture, &parts);
    }
}
