//! Real shared capture slots, not conditional or loop admission.
use super::*;

pub(in crate::ownership_checking::compilation_unit) const SOURCES: &[&str] = &[
    r#"fun run(): Unit {
val text = "source"
val callback = { text == "test" }
val forwarded = callback
}"#,
    r#"fun run(): Unit {
val text = "source"
val first = { text == "one" }
val second = { text == "two" }
val moved_first = first
val moved_second = second
}"#,
    r#"fun run(): Unit {
var text = "old"
val previous = replace(&text, "new")
val callback = { text == previous }
val forwarded = callback
}"#,
    r#"fun run(): Unit {
val text = "source"
val callback = { text == "test" }
val forwarded = callback
val later = text
}"#,
];

#[test]
fn shared_string_environment_has_real_source_owner_slots() {
    for text in SOURCES {
        let fixture = Fixture::new(&[("root.ko", text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        let observation = &owned.owner_observations()[0];
        assert_eq!(
            observation.status(),
            UnitOwnerObservationStatus::Complete,
            "{text}"
        );
        assert!(observation.is_consistent());
        let source = literal_owner(
            &fixture,
            0,
            observation,
            if text.contains("replace") {
                "\"new\""
            } else {
                "\"source\""
            },
        );
        let formations = observation
            .events()
            .iter()
            .enumerate()
            .filter_map(|(index, event)| match event {
                UnitOwnerTransportEvent::SharedCapture {
                    expression,
                    environment,
                    inputs,
                } => Some((index, *expression, environment, inputs)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            formations.len(),
            if text.contains("moved_first") { 2 } else { 1 }
        );
        for (formed_at, expression, environment, inputs) in &formations {
            assert_eq!(
                observation
                    .definition((*environment).clone())
                    .unwrap()
                    .origin(),
                UnitOwnerOrigin::Closure(*expression)
            );
            assert_eq!(inputs[0], (fixture.symbol(0, "text"), source.clone()));
            if text.contains("replace") {
                assert_eq!(inputs.len(), 2);
                assert_eq!(
                    inputs[1],
                    (
                        fixture.symbol(0, "previous"),
                        literal_owner(&fixture, 0, observation, "\"old\"")
                    )
                );
            } else {
                assert_eq!(inputs.len(), 1);
            }
            let ends = observation
                .events()
                .iter()
                .enumerate()
                .filter_map(|(index, event)| match event {
                    UnitOwnerTransportEvent::SharedCaptureEnd {
                        point,
                        environment: actual,
                        source,
                        owner,
                    } if actual == *environment => Some((index, *point, *source, owner.clone())),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(ends.len(), inputs.len());
            assert_eq!(
                ends.iter()
                    .map(|(_, _, symbol, owner)| (*symbol, owner.clone()))
                    .collect::<Vec<_>>(),
                inputs.iter().rev().cloned().collect::<Vec<_>>()
            );
            let end_point = ends[0].1;
            assert!(
                ends.iter()
                    .all(|(index, point, _, _)| index > formed_at && *point == end_point)
            );
            assert!(ends.windows(2).all(|pair| pair[1].0 == pair[0].0 + 1));
            let env_drop = ends.last().unwrap().0 + 1;
            assert!(
                matches!(&observation.events()[env_drop], UnitOwnerTransportEvent::Drop { point, owner, .. } if *point == end_point && owner == *environment)
            );
            for (_, borrowed) in *inputs {
                let source_drop = observation.events().iter().position(|event| matches!(event, UnitOwnerTransportEvent::Drop { owner, .. } if owner == borrowed)).unwrap();
                assert!(
                    source_drop > env_drop,
                    "source cannot die before its last environment"
                );
            }
        }
        // A shared slot never steals its source into an owned capture.
        assert!(!observation.events().iter().any(|event| matches!(
            event,
            UnitOwnerTransportEvent::Capture { .. } | UnitOwnerTransportEvent::CaptureDrop { .. }
        )));
        assert!(observation.value_observation_v1().is_none());
    }
}

pub(in crate::ownership_checking::compilation_unit) const BOUNDARIES: &[&str] = &[
    r#"fun run(text: String): Unit { val callback = { text == "x" } }"#,
    r#"fun run(): Unit { val text = "x"; val inner = { text == "x" }; val outer = { inner() } }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; if (flag) { val callback = { text == "x" } } else {} }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; while (flag) { val callback = { text == "x" } } }"#,
    r#"fun inspect(callback: () -> Boolean): Unit {}
fun run(): Unit { val text = "x"; inspect({ text == "x" }) }"#,
    r#"fun run(number: Int): Unit { val text = "x"; val callback = { text == "x" && number == 0 } }"#,
    r#"fun run(flag: Boolean): Unit { val text = "x"; val callback = { text == "x" }; if (flag) {} else {}; val forwarded = callback }"#,
];

#[test]
fn shared_capture_outside_the_named_straight_line_slice_stays_unsupported() {
    for text in BOUNDARIES {
        let owned = checked(text);
        let observation = owned
            .owner_observations()
            .iter()
            .rev()
            .find(|observation| matches!(observation.callable(), UnitOwnerCallable::Function(_)))
            .unwrap();
        assert!(
            matches!(
                observation.status(),
                UnitOwnerObservationStatus::Unsupported { .. }
            ),
            "{text}"
        );
        assert!(observation.events().is_empty());
        assert!(observation.definitions().is_empty());
    }
}

// Corrupted observation products are CI-only validation tests, not local runtime probes.
#[test]
fn shared_capture_validator_rejects_missing_duplicate_reordered_and_foreign_ends() {
    let fixture = Fixture::new(&[("root.ko", SOURCES[2])]);
    let owned = fixture.shared();
    let original = &owned.owner_observations()[0];
    let start = original
        .events
        .iter()
        .position(|event| matches!(event, UnitOwnerTransportEvent::SharedCapture { .. }))
        .unwrap();
    let end = original
        .events
        .iter()
        .position(|event| matches!(event, UnitOwnerTransportEvent::SharedCaptureEnd { .. }))
        .unwrap();
    let mut missing = original.clone();
    missing.events.remove(end);
    assert!(!missing.is_consistent());
    let mut duplicate = original.clone();
    duplicate.events.insert(end, original.events[end].clone());
    assert!(!duplicate.is_consistent());
    let mut reordered = original.clone();
    reordered.events.swap(end, end + 1);
    assert!(!reordered.is_consistent());
    let mut early_source_drop = original.clone();
    let source_drop = early_source_drop.events.remove(end + 3);
    early_source_drop.events.insert(end, source_drop);
    assert!(!early_source_drop.is_consistent());
    for empty in [false, true] {
        let mut damaged = original.clone();
        let UnitOwnerTransportEvent::SharedCapture { inputs, .. } = &mut damaged.events[start]
        else {
            panic!("formation")
        };
        if empty {
            inputs.clear();
        } else {
            inputs.push(inputs[0].clone());
        }
        assert!(!damaged.is_consistent());
    }
    let mut wrong_point = original.clone();
    let UnitOwnerTransportEvent::SharedCaptureEnd { point, .. } = &mut wrong_point.events[end]
    else {
        panic!("end")
    };
    *point = UnitDropPoint::AfterExpression(UnitExpressionId::new(
        fixture.source(0),
        fixture.expression(0, "callback"),
    ));
    assert!(!wrong_point.is_consistent());
    let foreign = fixture.shared();
    let UnitOwnerTransportEvent::SharedCapture {
        environment,
        inputs,
        ..
    } = &foreign.owner_observations()[0].events[start]
    else {
        panic!("formation")
    };
    for replace_environment in [false, true] {
        let mut damaged = original.clone();
        let UnitOwnerTransportEvent::SharedCaptureEnd {
            environment: actual,
            owner,
            ..
        } = &mut damaged.events[end]
        else {
            panic!("end")
        };
        if replace_environment {
            *actual = environment.clone();
        } else {
            *owner = inputs[1].1.clone();
        }
        assert!(!damaged.is_consistent());
        assert_ne!(original, &damaged);
    }
}
