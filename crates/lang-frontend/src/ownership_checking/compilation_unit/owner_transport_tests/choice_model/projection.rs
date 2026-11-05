//! The real legacy-to-value adapter, separate from synthetic choice graphs.
use super::*;

#[test]
fn value_projection_preserves_real_definition_identity_and_cleanup_catalog() {
    let source = r#"fun run(): Unit {
var a = "old a"
var b = "old b"
val incoming = "new"
val forwarded = (incoming)
val old = replace(&a, replace(&b, (forwarded)))
val moved = (old)
}"#;
    let fixture = Fixture::new(&[("root.ko", source)]);
    let owned = fixture.shared();
    assert!(owned.diagnostics().is_empty());
    assert!(owned.clone().validate().is_ok());
    let legacy = &owned.owner_observations()[0];
    assert_eq!(legacy.status(), UnitOwnerObservationStatus::Complete);
    assert!(legacy.is_consistent());
    let projection = legacy
        .value_observation_v1()
        .expect("straight-line String transport");
    assert!(projection.is_consistent());
    assert!(projection.choice().is_none());
    assert!(projection.suffix().is_empty());
    assert_eq!(projection.producers().definitions(), legacy.definitions());
    assert_eq!(projection.producers().definitions().len(), 3);
    let mut creates = Vec::new();
    let mut replacements = 0;
    let mut drops = 0;
    let mut call_drops = Vec::new();
    for event in projection.prefix() {
        match event {
            Event::Create(expression, owner) => {
                assert_eq!(
                    legacy.definition(owner.clone()).unwrap().origin(),
                    UnitOwnerOrigin::String(*expression)
                );
                creates.push(owner.clone());
            }
            Event::Replace {
                previous, incoming, ..
            } => {
                let (Value::Definition(previous), Value::Definition(incoming)) =
                    (previous, incoming)
                else {
                    panic!("no synthetic Choice in the production adapter");
                };
                assert_ne!(previous, incoming);
                assert!(legacy.definition(previous.clone()).is_some());
                assert!(legacy.definition(incoming.clone()).is_some());
                replacements += 1;
            }
            Event::Drop(point, _, Value::Definition(owner)) => {
                assert!(legacy.definition(owner.clone()).is_some());
                assert!(legacy.events().iter().any(|event| matches!(event, UnitOwnerTransportEvent::Drop { point: actual, owner: token, .. } if actual == point && token == owner)));
                if let UnitDropPoint::CallReturn(call) = point {
                    call_drops.push((
                        *call,
                        projection
                            .producers()
                            .definition(owner.clone())
                            .unwrap()
                            .origin(),
                    ));
                }
                drops += 1;
            }
            _ => {}
        }
    }
    assert_eq!(creates.len(), 3);
    assert_eq!(replacements, 2);
    assert_eq!(drops, 3);
    let inner = UnitExpressionId::new(
        fixture.source(0),
        fixture.expression(0, "replace(&b, (forwarded))"),
    );
    let outer = UnitExpressionId::new(
        fixture.source(0),
        fixture.expression(0, "replace(&a, replace(&b, (forwarded)))"),
    );
    assert_eq!(
        call_drops,
        vec![
            (
                inner,
                UnitOwnerOrigin::String(UnitExpressionId::new(
                    fixture.source(0),
                    fixture.expression(0, "\"new\"")
                ))
            ),
            (
                outer,
                UnitOwnerOrigin::String(UnitExpressionId::new(
                    fixture.source(0),
                    fixture.expression(0, "\"old b\"")
                ))
            ),
        ]
    );
    let mut wrong_call = projection.clone();
    let event = wrong_call
        .prefix
        .iter_mut()
        .find(|event| matches!(event, Event::Drop(UnitDropPoint::CallReturn(_), ..)))
        .unwrap();
    let Event::Drop(point, ..) = event else {
        unreachable!()
    };
    *point = UnitDropPoint::CallReturn(outer);
    assert!(
        !wrong_call.is_consistent(),
        "later replacement cannot prove an earlier root cleanup"
    );
    let drop_index = projection
        .prefix
        .iter()
        .position(|event| matches!(event, Event::Drop(UnitDropPoint::CallReturn(_), ..)))
        .unwrap();
    let mut wrong_root = projection.clone();
    let Event::Drop(_, root, _) = &mut wrong_root.prefix[drop_index] else {
        unreachable!()
    };
    *root = fixture.symbol(0, "a");
    assert!(!wrong_root.is_consistent());
    let mut wrong_owner = projection.clone();
    let Event::Drop(_, _, owner) = &mut wrong_owner.prefix[drop_index] else {
        unreachable!()
    };
    *owner = Value::Definition(literal_owner(&fixture, 0, legacy, "\"old a\""));
    assert!(!wrong_owner.is_consistent());
    let mut non_adjacent = projection.clone();
    non_adjacent.prefix.insert(
        drop_index,
        Event::Forward(
            inner,
            inner,
            Value::Definition(literal_owner(&fixture, 0, legacy, "\"old b\"")),
        ),
    );
    assert!(
        !non_adjacent.is_consistent(),
        "CallReturn needs the immediately preceding replacement, not any earlier match"
    );
    let independent = fixture.shared();
    assert!(
        independent.owner_observations()[0]
            .definition(creates[0].clone())
            .is_none(),
        "equal syntax must not interchange tokens across products"
    );
}

#[test]
fn value_projection_does_not_upgrade_unsupported_or_non_string_domains() {
    for source in [
        "fun run(flag: Boolean): Unit { var value = if (flag) \"left\" else \"right\" }",
        "fun run(flag: Boolean): Unit { if (flag) { val left = \"left\" } else { val right = \"right\" } }",
        "fun run(): String = \"implicit result\"",
    ] {
        let fixture = Fixture::new(&[("root.ko", source)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{source}");
        assert!(
            owned.owner_observations()[0]
                .value_observation_v1()
                .is_none(),
            "{source}"
        );
    }
    let parameter = Fixture::new(&[(
        "root.ko",
        "fun run(own value: String): Unit { val moved = value }",
    )]);
    let owned = parameter.shared();
    assert!(owned.diagnostics().is_empty());
    let legacy = &owned.owner_observations()[0];
    assert_eq!(legacy.status(), UnitOwnerObservationStatus::Complete);
    assert!(
        legacy
            .definitions()
            .iter()
            .any(|definition| matches!(definition.origin(), UnitOwnerOrigin::Parameter(_)))
    );
    assert!(legacy.value_observation_v1().is_none());
    let fixture = Fixture::new(&[("root.ko", "fun run(): Unit {}")]);
    let owned = fixture.shared();
    assert!(
        owned.owner_observations()[0]
            .value_observation_v1()
            .unwrap()
            .is_consistent()
    );
}

#[test]
fn value_projection_validator_rejects_missing_duplicate_and_foreign_transport() {
    let fixture = Fixture::new(&[("root.ko", "fun run(): Unit { val value = \"owned\" }")]);
    let owned = fixture.shared();
    let graph = owned.owner_observations()[0]
        .value_observation_v1()
        .unwrap();
    let mut unproven_call = graph.clone();
    let event = unproven_call.prefix.last_mut().unwrap();
    let Event::Drop(point, ..) = event else {
        unreachable!()
    };
    *point = UnitDropPoint::CallReturn(UnitExpressionId::new(
        fixture.source(0),
        fixture.expression(0, "\"owned\""),
    ));
    assert!(
        !unproven_call.is_consistent(),
        "CallReturn requires an earlier matching replacement"
    );
    let mut missing = graph.clone();
    missing.prefix.pop();
    assert!(!missing.is_consistent());
    let mut duplicate = graph.clone();
    duplicate.prefix.push(graph.prefix[0].clone());
    assert!(!duplicate.is_consistent());
    let independent = fixture.shared();
    let mut foreign = graph.clone();
    foreign.catalog = independent.owner_observations()[0]
        .value_observation_v1()
        .unwrap()
        .catalog;
    assert!(!foreign.is_consistent());
    let mut suffix = graph.clone();
    suffix.suffix = graph.prefix.clone();
    assert!(!suffix.is_consistent());
}

#[test]
fn independent_catalog_rejects_bad_indices_origins_and_product_tokens() {
    let fixture = Fixture::new(&[(
        "root.ko",
        "fun run(): Unit { val a = \"a\"; val b = \"b\" }",
    )]);
    let owned = fixture.shared();
    let graph = owned.owner_observations()[0]
        .value_observation_v1()
        .unwrap();
    let mut wrong_index = graph.producers().clone();
    wrong_index.definitions[0].id.index = 1;
    assert!(!wrong_index.is_consistent());
    let mut duplicate_origin = graph.producers().clone();
    duplicate_origin.definitions[1].origin = duplicate_origin.definitions[0].origin;
    assert!(!duplicate_origin.is_consistent());
    let independent = fixture.shared();
    let mut foreign = graph.producers().clone();
    foreign.definitions[0] = independent.owner_observations()[0].definitions()[0].clone();
    assert!(!foreign.is_consistent());
}
