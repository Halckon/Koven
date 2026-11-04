//! Rejecting identity observation must preserve every existing cleanup field.
use super::*;
use crate::ownership_checking::compilation_unit::owner_transport_tests::branch_boundaries::{
    BRANCHES, source,
};

#[test]
fn branch_paths_keep_identical_cleanup_with_observation_disabled() {
    for control in BRANCHES {
        let text = source(control);
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let shared = fixture.shared();
        assert!(shared.clone().validate().is_ok(), "{control}");
        let parts = fixture.parts(&shared);
        compare(&fixture, &parts);
    }
}

#[test]
fn supported_if_observation_preserves_all_legacy_cleanup_fields() {
    for text in [
        r#"fun run(flag: Boolean): Unit {
var root = "retained"
if (flag) { val left = "left" } else { val right = "right" }
val old = replace(&root, "new")
}"#,
        r#"fun run(flag: Boolean): Unit {
val root = "source"
if (flag) { val left = root } else { val right = root }
}"#,
        r#"fun run(flag: Boolean): Unit {
if (flag) { if (flag) { val inner = "inner" } else { val other = "other" } }
else { val outer = "outer" }
}"#,
    ] {
        let fixture = Fixture::new(&[("root.ko", text)]);
        let shared = fixture.shared();
        assert!(shared.clone().validate().is_ok());
        assert!(
            shared
                .owner_observations()
                .iter()
                .all(|observation| observation.status()
                    == crate::ownership_checking::UnitOwnerObservationStatus::Complete)
        );
        let parts = fixture.parts(&shared);
        compare(&fixture, &parts);
    }
}

#[test]
fn rejected_if_joins_and_rollback_keep_identical_complete_cleanup_products() {
    use crate::ownership_checking::compilation_unit::owner_transport_tests::if_observation::{
        CHANGED_ROOT, ROLLBACK_BODIES, rollback_source,
    };
    for text in std::iter::once(CHANGED_ROOT.to_owned())
        .chain(ROLLBACK_BODIES.iter().map(|body| rollback_source(body)))
    {
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let shared = fixture.shared();
        assert!(shared.clone().validate().is_ok());
        assert!(matches!(
            shared.owner_observations()[0].status(),
            crate::ownership_checking::UnitOwnerObservationStatus::Unsupported { .. }
        ));
        let parts = fixture.parts(&shared);
        compare(&fixture, &parts);
    }
}

#[test]
fn choice_model_consumers_preserve_complete_analysis_while_still_unsupported() {
    use crate::ownership_checking::compilation_unit::owner_transport_tests::choice_model::{
        CONSUMERS, consumer_source,
    };
    for body in CONSUMERS {
        let text = consumer_source(body);
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let shared = fixture.shared();
        assert!(shared.clone().validate().is_ok());
        let parts = fixture.parts(&shared);
        compare(&fixture, &parts);
    }
}
