//! Composition admission changes observation only, never the old planner product.
use super::*;
use crate::ownership_checking::compilation_unit::owner_transport_tests::combination::{
    COPYABLE_LIVE, MIXED_BODIES, NESTED, mixed_source,
};

#[test]
fn combined_slices_and_mixed_rejection_preserve_complete_legacy_analysis() {
    for text in MIXED_BODIES
        .iter()
        .map(|(body, _)| mixed_source(body))
        .chain([COPYABLE_LIVE.to_owned(), NESTED.to_owned()])
    {
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let shared = fixture.shared();
        assert!(shared.clone().validate().is_ok());
        let parts = fixture.parts(&shared);
        compare(&fixture, &parts);
    }
}
