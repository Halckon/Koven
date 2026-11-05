//! Composition admission changes observation only, never the old planner product.
use super::*;
use crate::ownership_checking::compilation_unit::owner_transport_tests::capture_branch_release::{
    ASYMMETRIC_RELEASE, NESTED_RELEASE, RETURN_RELEASE,
};
use crate::ownership_checking::compilation_unit::owner_transport_tests::capture_composition::{
    NESTED_UNUSED_EDGE, TWO_ENVIRONMENTS, TWO_SLOTS,
};
use crate::ownership_checking::compilation_unit::owner_transport_tests::combination::{
    BRANCH_RELEASE, COPYABLE_LIVE, MIXED_BODIES, NESTED, SEQUENTIAL_BODIES, mixed_source,
};

#[test]
fn combined_slices_and_mixed_rejection_preserve_complete_legacy_analysis() {
    for text in MIXED_BODIES
        .iter()
        .chain(SEQUENTIAL_BODIES.iter())
        .map(|(body, _)| mixed_source(body))
        .chain([
            COPYABLE_LIVE.to_owned(),
            NESTED.to_owned(),
            BRANCH_RELEASE.to_owned(),
            NESTED_RELEASE.to_owned(),
            ASYMMETRIC_RELEASE.to_owned(),
            RETURN_RELEASE.to_owned(),
            TWO_ENVIRONMENTS.to_owned(),
            TWO_SLOTS.to_owned(),
            NESTED_UNUSED_EDGE.to_owned(),
            mixed_source(
                r#"val text = "capture"
val callback = move { text }
if (flag) { val left = "left" } else { val right = "right" }"#,
            ),
        ])
    {
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let shared = fixture.shared();
        assert!(shared.clone().validate().is_ok());
        let parts = fixture.parts(&shared);
        compare(&fixture, &parts);
    }
}
