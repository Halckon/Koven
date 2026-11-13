//! Ordinary source-driven checks of the conditional child recurrence, not fault injection.
use super::tests::SOURCE;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
#[test]
fn child_generation_rules_bind_current_roots_to_actual_fresh_sites_and_drops() {
    let original = r#"val x=replace(&a,"inner-a"); val y=replace(&b,"inner-b")"#;
    let swapped = r#"val y=replace(&b,"inner-b"); val x=replace(&a,"inner-a")"#;
    assert!(SOURCE.contains(original));
    for source in [SOURCE.to_owned(), SOURCE.replace(original, swapped)] {
        let fixture = Fixture::new(&[("child-generation.ko", &source)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        let nested = owned
            .owner_observations()
            .iter()
            .find_map(|o| o.nested_normal_body_v1())
            .unwrap();
        let rule = nested
            .child_generation_rule_v1()
            .expect("sealed conditional recurrence");
        assert!(rule.is_consistent());
        assert_eq!(rule.control(), nested.control());
        assert_eq!(rule.condition(), nested.condition());
        let normal = nested.normal();
        assert_eq!(rule.recurrences().len(), normal.header_roots().len());
        for (recurrence, root) in rule.recurrences().iter().zip(normal.header_roots()) {
            let step = normal
                .steps()
                .iter()
                .find(|step| step.header_root() == *root)
                .unwrap();
            assert_eq!(recurrence.root(), *root);
            assert_eq!(recurrence.producer(), step.producer());
            assert_eq!(recurrence.retirement(), step.returned_value_drop());
        }
        // Child zero-or-more exit returns the same complete symbolic current vector.
        let (live, drops) = nested.false_exit_partition().unwrap();
        assert!(drops.is_empty());
        assert_eq!(live, normal.header_roots());
        assert_eq!(nested.parent_resume_roots(), Some(live));
    }
}
#[test]
fn child_generation_rule_does_not_admit_repeated_roots_or_non_normal_paths() {
    for source in [
        SOURCE.replace("replace(&b,\"inner-b\")", "replace(&a,\"inner-b\")"),
        SOURCE.replace("n1=n1-1; g1=n1>0", "break"),
        SOURCE.replace("n1=n1-1; g1=n1>0", "return"),
        SOURCE.replace("n1=n1-1; g1=n1>0", "g1=false"),
        SOURCE.replace("n1=n1-1; g1=n1>0", ""),
    ] {
        let fixture = Fixture::new(&[("child-generation-rejected.ko", &source)]);
        let owned = fixture.shared();
        assert!(owned.owner_observations().iter().all(|o| {
            o.nested_normal_body_v1()
                .and_then(|n| n.child_generation_rule_v1())
                .is_none()
        }));
    }
}
