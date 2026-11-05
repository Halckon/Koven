//! Top-level sequences are admitted; captures inside real If edges remain unsupported.
use super::*;

pub(in crate::ownership_checking::compilation_unit) const SEQUENTIAL_BODIES: &[(&str, &str)] = &[
    (
        r#"val text = "capture"
val callback = move { text }
if (flag) { val left = "left" } else { val right = "right" }
val forwarded = (callback)"#,
        r#"if (flag) { val left = "left" } else { val right = "right" }"#,
    ),
    (
        r#"if (flag) { val left = "left" } else { val right = "right" }
val text = "capture"
val callback = move { text }"#,
        "move { text }",
    ),
    (
        r#"if (flag) { if (flag) { val inner = "inner" } else { val other = "other" } }
else { val outer = "outer" }
val text = "capture"
val callback = move { text }"#,
        "move { text }",
    ),
];

pub(in crate::ownership_checking::compilation_unit) const MIXED_BODIES: &[(&str, &str)] = &[
    (
        r#"if (flag) { val text = "capture"; val callback = move { text } }
else { val right = "right" }"#,
        "move { text }",
    ),
    (
        r#"if (flag) { val left = "left" }
else { val text = "capture"; val callback = move { text } }"#,
        "move { text }",
    ),
    (
        r#"if (flag) { if (flag) { val text = "capture"; val callback = move { text } }
else { val inner = "inner" } } else { val outer = "outer" }"#,
        "move { text }",
    ),
];

pub(in crate::ownership_checking::compilation_unit) fn mixed_source(body: &str) -> String {
    format!(
        r#"fun run(flag: Boolean): Unit {{
var root = "old"
val old = replace(&root, "new")
{body}
}}
fun capture_sibling(): Unit {{ val sibling_text = "sibling capture"; val callback = move {{ sibling_text }} }}
fun branch_sibling(flag: Boolean): Unit {{
if (flag) {{ val left = "sibling left" }} else {{ val right = "sibling right" }}
}}"#
    )
}

#[test]
fn branch_capture_rejects_at_formation_and_resets_for_sibling_functions() {
    for (body, rejected_text) in MIXED_BODIES {
        let text = mixed_source(body);
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        let functions = owned
            .owner_observations()
            .iter()
            .filter(|observation| matches!(observation.callable(), UnitOwnerCallable::Function(_)))
            .collect::<Vec<_>>();
        let [rejected, capture, branch] = functions.as_slice() else {
            panic!("three independent ordinary functions");
        };
        assert_eq!(
            rejected.status(),
            UnitOwnerObservationStatus::Unsupported {
                point: UnitDropPoint::AfterExpression(UnitExpressionId::new(
                    fixture.source(0),
                    fixture.expression(0, rejected_text)
                )),
                reason: UnitOwnerUnsupportedReason::Syntax,
            }
        );
        assert!(rejected.definitions().is_empty());
        assert!(rejected.events().is_empty());
        assert!(rejected.is_consistent());
        for sibling in [capture, branch] {
            assert_eq!(sibling.status(), UnitOwnerObservationStatus::Complete);
            assert!(sibling.is_consistent());
            assert!(!sibling.definitions().is_empty());
        }
        assert!(
            capture
                .events()
                .iter()
                .any(|event| matches!(event, UnitOwnerTransportEvent::Capture { .. }))
        );
        assert!(
            branch
                .events()
                .iter()
                .any(|event| matches!(event, UnitOwnerTransportEvent::If { .. }))
        );
    }
}

#[test]
fn validator_rejects_an_entire_capture_sequence_moved_inside_an_if_edge() {
    let text = mixed_source(MIXED_BODIES[0].0);
    let fixture = Fixture::new(&[("root.ko", &text)]);
    let owned = fixture.shared();
    let mut capture = owned
        .owner_observations()
        .iter()
        .find(|observation| {
            observation
                .events()
                .iter()
                .any(|event| matches!(event, UnitOwnerTransportEvent::Capture { .. }))
        })
        .unwrap()
        .clone();
    assert!(capture.is_consistent());
    let control = UnitExpressionId::new(
        fixture.source(0),
        fixture.expression(
            0,
            r#"if (flag) { val left = "sibling left" } else { val right = "sibling right" }"#,
        ),
    );
    let mut event_inside_branch = capture.clone();
    let index = event_inside_branch
        .events
        .iter()
        .position(|event| matches!(event, UnitOwnerTransportEvent::Capture { .. }))
        .unwrap();
    let event = event_inside_branch.events[index].clone();
    event_inside_branch.events[index] = UnitOwnerTransportEvent::If {
        control,
        then_events: vec![event.clone()],
        else_events: vec![event],
    };
    assert!(
        !event_inside_branch.is_consistent(),
        "matching final locations cannot admit capture formation inside either edge"
    );
    let then_events = std::mem::take(&mut capture.events);
    capture.events.push(UnitOwnerTransportEvent::If {
        control,
        then_events,
        else_events: vec![],
    });
    assert!(
        !capture.is_consistent(),
        "conservation alone cannot admit branch-local capture formation or release"
    );
}

pub(in crate::ownership_checking::compilation_unit) const COPYABLE_LIVE: &str = r#"fun run(flag: Boolean, number: Int): Unit {
var root = "retained"
if (flag) { val left = "left" } else { val right = "right" }
val copied_number = number
val copied_flag = flag
val old = replace(&root, "new")
}"#;

#[test]
fn live_copyable_bindings_do_not_need_owner_tokens_at_if_join() {
    let fixture = Fixture::new(&[("root.ko", COPYABLE_LIVE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = &owned.owner_observations()[0];
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    let owner = literal_owner(&fixture, 0, observation, "\"retained\"");
    assert!(observation.events().iter().any(|event| matches!(event,
        UnitOwnerTransportEvent::Replace { previous, .. } if previous == &owner)));
    assert!(
        observation
            .definitions()
            .iter()
            .all(|definition| !matches!(definition.origin(), UnitOwnerOrigin::Parameter(_)))
    );
}

pub(in crate::ownership_checking::compilation_unit) const NESTED: &str = r#"fun run(flag: Boolean): Unit {
if (flag) { if (flag) { val inner = "inner" } else { val other = "other" } }
else { val outer = "outer" }
}"#;

#[test]
fn nested_if_keeps_actual_control_tree_and_rejects_repeated_control() {
    let fixture = Fixture::new(&[("root.ko", NESTED)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = &owned.owner_observations()[0];
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    let [
        UnitOwnerTransportEvent::If {
            control,
            then_events,
            else_events,
        },
    ] = observation.events()
    else {
        panic!("exactly the outer branch event");
    };
    assert_eq!(*control, UnitExpressionId::new(fixture.source(0), fixture.expression(0,
        "if (flag) { if (flag) { val inner = \"inner\" } else { val other = \"other\" } }\nelse { val outer = \"outer\" }")));
    let [
        UnitOwnerTransportEvent::If {
            control: inner_control,
            then_events: inner_then,
            else_events: inner_else,
        },
    ] = then_events.as_slice()
    else {
        panic!("nested control is inside true edge, not flattened");
    };
    assert_eq!(
        *inner_control,
        UnitExpressionId::new(
            fixture.source(0),
            fixture.expression(
                0,
                "if (flag) { val inner = \"inner\" } else { val other = \"other\" }"
            )
        )
    );
    for (events, literal) in [
        (inner_then, "\"inner\""),
        (inner_else, "\"other\""),
        (else_events, "\"outer\""),
    ] {
        let owner = literal_owner(&fixture, 0, observation, literal);
        let drops = events
            .iter()
            .filter_map(|event| match event {
                UnitOwnerTransportEvent::Drop { owner, .. } => Some(owner),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(drops, vec![&owner]);
        assert!(
            events
                .iter()
                .all(|event| !matches!(event, UnitOwnerTransportEvent::If { .. }))
        );
    }
    let mut repeated = observation.clone();
    // No creation/drop events are copied, so only the control-identity guard rejects this.
    repeated.events.push(UnitOwnerTransportEvent::If {
        control: *inner_control,
        then_events: vec![],
        else_events: vec![],
    });
    assert!(!repeated.is_consistent());
}

pub(in crate::ownership_checking::compilation_unit) const BRANCH_RELEASE: &str = r#"fun run(flag: Boolean): Unit {
val text = "capture"
val callback = move { text }
if (flag) { val left = callback } else { val right = callback }
}"#;

#[test]
fn top_level_capture_and_if_sequences_preserve_environment_identity() {
    for (body, _) in SEQUENTIAL_BODIES {
        let text = mixed_source(body);
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        let observation = owned
            .owner_observations()
            .iter()
            .find(|observation| matches!(observation.callable(), UnitOwnerCallable::Function(_)))
            .unwrap();
        assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
        assert!(observation.is_consistent());
        let environment = observation
            .events()
            .iter()
            .find_map(|event| match event {
                UnitOwnerTransportEvent::Capture { environment, .. } => Some(environment.clone()),
                _ => None,
            })
            .expect("the original leaf capture must remain observable");
        assert!(
            observation
                .events()
                .iter()
                .any(|event| matches!(event, UnitOwnerTransportEvent::If { .. }))
        );
        assert!(observation.events().iter().any(|event| matches!(event,
            UnitOwnerTransportEvent::CaptureDrop { environment: released, .. }
                if released == &environment)));
        if body.contains("forwarded") {
            let branch = observation
                .events()
                .iter()
                .position(|event| matches!(event, UnitOwnerTransportEvent::If { .. }))
                .unwrap();
            let forwarded = observation
                .events()
                .iter()
                .position(|event| {
                    matches!(event,
                UnitOwnerTransportEvent::Binding { symbol, owner, .. }
                    if *symbol == fixture.symbol(0, "forwarded") && owner == &environment)
                })
                .unwrap();
            assert!(
                branch < forwarded,
                "the same environment survives both real edges"
            );
        }
    }
}

#[test]
fn both_branches_release_a_top_level_capture_environment() {
    let fixture = Fixture::new(&[("root.ko", BRANCH_RELEASE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = owned
        .owner_observations()
        .iter()
        .find(|observation| matches!(observation.callable(), UnitOwnerCallable::Function(_)))
        .unwrap();
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
}
