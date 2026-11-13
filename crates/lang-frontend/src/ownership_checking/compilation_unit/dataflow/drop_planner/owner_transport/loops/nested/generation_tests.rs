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
        assert_eq!(
            rule.entry_owners()
                .iter()
                .map(|(r, _)| *r)
                .collect::<Vec<_>>(),
            normal.header_roots()
        );
        for (i, (_, owner)) in rule.entry_owners().iter().enumerate() {
            assert!(
                rule.entry_owners()[..i]
                    .iter()
                    .all(|(_, prior)| prior != owner)
            );
            assert!(rule.recurrences().iter().all(|r| &r.producer() != owner));
        }
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

#[test]
fn child_generation_observes_entry_concat_without_literal_origin() {
    let source = SOURCE
        .replace(", own seed: String", "")
        .replace("var a=seed", "var a=\"left\"+\"right\"");
    let fixture = Fixture::new(&[("entry-concat.ko", &source)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty());
    let observation = owned
        .owner_observations()
        .iter()
        .find(|o| o.nested_normal_body_v1().is_some())
        .unwrap_or_else(|| panic!("child observation: {:?}", owned.owner_observations()));
    assert!(observation.is_consistent());
    let rule = observation
        .nested_normal_body_v1()
        .unwrap()
        .child_generation_rule_v1()
        .expect("actual concat entry owner");
    assert!(rule.is_consistent());
}

#[test]
fn entry_concat_does_not_admit_named_chained_or_control_producers() {
    for source in [
        SOURCE.replace("var a=seed", "var a=seed+\"right\""),
        SOURCE.replace("var a=seed", "var a=\"left\"+\"middle\"+\"right\""),
        SOURCE.replace("var a=seed", "var a=if(true){\"left\"+\"right\"}else{seed}"),
    ] {
        let fixture = Fixture::new(&[("entry-concat-rejected.ko", &source)]);
        let owned = fixture.shared();
        assert!(owned.owner_observations().iter().all(|o| {
            o.nested_normal_body_v1()
                .and_then(|n| n.child_generation_rule_v1())
                .is_none()
        }));
    }
}

#[test]
fn entry_concat_publishes_distinct_completed_origin_and_closes_literal_inputs() {
    use crate::ownership_checking::UnitOwnerOrigin;
    let fixture = Fixture::new(&[("concat-origin.ko", r#"fun run() { val a="left"+"right" }"#)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty());
    let observation = &owned.owner_observations()[0];
    assert!(observation.is_consistent());
    assert_eq!(observation.definitions().len(), 3);
    let UnitOwnerOrigin::StringConcat {
        expression,
        left,
        right,
    } = observation.definitions()[2].origin()
    else {
        panic!("independent concat origin");
    };
    assert_ne!(expression, left);
    assert_ne!(expression, right);
    assert_ne!(left, right);
    assert_eq!(
        observation.definitions()[0].origin(),
        UnitOwnerOrigin::String(left)
    );
    assert_eq!(
        observation.definitions()[1].origin(),
        UnitOwnerOrigin::String(right)
    );
    assert!(
        crate::ownership_checking::UnitOwnerProducerCatalogV1::from_legacy(observation).is_none()
    );
}

#[test]
fn entry_concat_rejects_second_concat_and_interpolated_operand() {
    let source = SOURCE.replace(", own seed: String", "");
    for source in [
        source.replace(
            "var a=seed; var b=\"b\"",
            r#"var a="left"+"right"; var b="other"+"second""#,
        ),
        source.replace("var a=seed", r#"var a="${outer}"+"right""#),
    ] {
        let fixture = Fixture::new(&[("entry-concat-boundary.ko", &source)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        assert!(owned.owner_observations().iter().all(|o| {
            o.nested_normal_body_v1()
                .and_then(|n| n.child_generation_rule_v1())
                .is_none()
        }));
    }
}

#[test]
fn entry_concat_does_not_publish_borrow_binding_owner() {
    use crate::ownership_checking::UnitOwnerOrigin;
    let fixture = Fixture::new(&[(
        "entry-concat-borrow.ko",
        r#"fun run() { borrow val a="left"+"right"; println(a) }"#,
    )]);
    let owned = fixture.shared();
    // A borrow binding cannot acquire the completed owned-producer claim.
    assert!(owned.owner_observations().iter().all(|o| {
        o.definitions()
            .iter()
            .all(|d| !matches!(d.origin(), UnitOwnerOrigin::StringConcat { .. }))
    }));
}
