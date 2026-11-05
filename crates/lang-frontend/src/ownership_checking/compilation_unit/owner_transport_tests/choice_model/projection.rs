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
    let legacy = &owned.owner_observations()[0];
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
                drops += 1;
            }
            _ => {}
        }
    }
    assert_eq!(creates.len(), 3);
    assert_eq!(replacements, 2);
    assert_eq!(drops, 3);
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
