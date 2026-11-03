//! The first If slice preserves both real paths without inventing a joined producer.
use super::*;

pub(in crate::ownership_checking::compilation_unit) const CHANGED_ROOT: &str = r#"fun run(flag: Boolean): Unit {
var root = "old"
if (flag) { val old = replace(&root, "left") } else { val old = replace(&root, "right") }
val after = replace(&root, "new")
}"#;

#[test]
fn explicit_unit_if_keeps_branch_definitions_and_surviving_owner_identity() {
    let fixture = Fixture::new(&[(
        "root.ko",
        r#"fun run(flag: Boolean): Unit {
var root = "retained"
if (flag) { val left = "left" } else { val right = "right" }
val old = replace(&root, "new")
}"#,
    )]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = &owned.owner_observations()[0];
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    let retained = literal_owner(&fixture, 0, observation, "\"retained\"");
    let left = literal_owner(&fixture, 0, observation, "\"left\"");
    let right = literal_owner(&fixture, 0, observation, "\"right\"");
    let branch = observation
        .events()
        .iter()
        .find_map(|event| match event {
            UnitOwnerTransportEvent::If {
                control,
                then_events,
                else_events,
            } => Some((*control, then_events, else_events)),
            _ => None,
        })
        .expect("one actual branch");
    assert_eq!(
        branch.0,
        UnitExpressionId::new(
            fixture.source(0),
            fixture.expression(
                0,
                "if (flag) { val left = \"left\" } else { val right = \"right\" }"
            )
        )
    );
    for (events, owner, other) in [(branch.1, &left, &right), (branch.2, &right, &left)] {
        assert!(events.iter().any(|event| matches!(event,
            UnitOwnerTransportEvent::Drop { owner: dropped, .. } if dropped == owner)));
        assert!(!events.iter().any(|event| matches!(event,
            UnitOwnerTransportEvent::Result { owner: result, .. } if result == other)));
    }
    assert!(observation.events().iter().any(|event| matches!(event,
        UnitOwnerTransportEvent::Replace { previous, .. } if previous == &retained)));
    let again = fixture.shared();
    assert_eq!(owned, again);
    assert_ne!(
        observation.definitions()[0].id(),
        again.owner_observations()[0].definitions()[0].id()
    );
}

#[test]
fn changing_live_owner_on_each_edge_is_not_coalesced_by_symbol() {
    let owned = checked(CHANGED_ROOT);
    let observation = &owned.owner_observations()[0];
    assert!(matches!(
        observation.status(),
        UnitOwnerObservationStatus::Unsupported { .. }
    ));
    assert!(observation.definitions().is_empty());
    assert!(observation.events().is_empty());
    assert!(observation.is_consistent());
}

#[test]
fn mutually_exclusive_consumption_replays_from_the_same_incoming_owner() {
    let fixture = Fixture::new(&[(
        "root.ko",
        r#"fun run(flag: Boolean): Unit {
val root = "source"
if (flag) { val left = root } else { val right = root }
}"#,
    )]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = &owned.owner_observations()[0];
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    let owner = literal_owner(&fixture, 0, observation, "\"source\"");
    let (then_events, else_events) = observation
        .events()
        .iter()
        .find_map(|event| match event {
            UnitOwnerTransportEvent::If {
                then_events,
                else_events,
                ..
            } => Some((then_events, else_events)),
            _ => None,
        })
        .unwrap();
    for events in [then_events, else_events] {
        assert!(events.iter().any(|event| matches!(event,
            UnitOwnerTransportEvent::Move { owner: moved, .. } if moved == &owner)));
        assert!(events.iter().any(|event| matches!(event,
            UnitOwnerTransportEvent::Drop { owner: dropped, .. } if dropped == &owner)));
    }
    let mut flattened = observation.clone();
    flattened.events = flattened
        .events
        .into_iter()
        .flat_map(|event| match event {
            UnitOwnerTransportEvent::If {
                mut then_events,
                else_events,
                ..
            } => {
                then_events.extend(else_events);
                then_events
            }
            event => vec![event],
        })
        .collect();
    assert!(
        !flattened.is_consistent(),
        "exclusive paths cannot be a linear trace"
    );
    let mut missing = observation.clone();
    for event in &mut missing.events {
        if let UnitOwnerTransportEvent::If { else_events, .. } = event {
            else_events.clear();
        }
    }
    assert!(
        !missing.is_consistent(),
        "an incoming live owner cannot vanish on one path"
    );
    let mut foreign = observation.clone();
    let foreign_owner = fixture.shared().owner_observations()[0].definitions()[0].id();
    for event in &mut foreign.events {
        if let UnitOwnerTransportEvent::If { then_events, .. } = event {
            for event in then_events {
                if let UnitOwnerTransportEvent::Move { owner, .. } = event {
                    *owner = foreign_owner.clone();
                }
            }
        }
    }
    assert!(!foreign.is_consistent());
    assert_ne!(foreign, *observation);
}

#[test]
fn branch_local_definition_cannot_be_recreated_on_the_other_edge() {
    let owned = checked(
        r#"fun run(flag: Boolean): Unit {
if (flag) { val left = "left" } else { val right = "right" }
}"#,
    );
    let observation = &owned.owner_observations()[0];
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    let mut duplicate = observation.clone();
    for event in &mut duplicate.events {
        if let UnitOwnerTransportEvent::If {
            then_events,
            else_events,
            ..
        } = event
        {
            else_events.extend(then_events.iter().cloned());
        }
    }
    assert!(!duplicate.is_consistent());
    let mut wrong_edge = observation.clone();
    for event in &mut wrong_edge.events {
        if let UnitOwnerTransportEvent::If {
            control,
            then_events,
            ..
        } = event
        {
            for event in then_events {
                if let UnitOwnerTransportEvent::Drop { point, .. } = event {
                    *point = UnitDropPoint::BranchExit {
                        control: *control,
                        branch: 1,
                    };
                }
            }
        }
    }
    assert!(!wrong_edge.is_consistent());
}

pub(in crate::ownership_checking::compilation_unit) const ROLLBACK_BODIES: &[&str] = &[
    r#"if (flag) { val left = "left" } else { val rejected = "bad" + "shape" }"#,
    r#"if (flag) { val left = "left" } else { val right = "right" }
val rejected = "bad" + "shape""#,
];

pub(in crate::ownership_checking::compilation_unit) fn rollback_source(body: &str) -> String {
    format!(
        r#"fun run(flag: Boolean): Unit {{
var root = "old"
val old = replace(&root, "new")
{body}
}}
fun independent(): Unit {{ val untouched = "sibling" }}"#
    )
}

#[test]
fn unsupported_second_edge_or_suffix_discards_successful_branch_and_keeps_sibling() {
    for body in ROLLBACK_BODIES {
        let text = rollback_source(body);
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        let [rejected, sibling] = owned.owner_observations() else {
            panic!("both functions have independent observations");
        };
        assert_eq!(
            rejected.status(),
            UnitOwnerObservationStatus::Unsupported {
                point: UnitDropPoint::AfterExpression(UnitExpressionId::new(
                    fixture.source(0),
                    fixture.expression(0, "\"bad\" + \"shape\"")
                )),
                reason: UnitOwnerUnsupportedReason::Syntax,
            }
        );
        assert!(rejected.definitions().is_empty());
        assert!(rejected.events().is_empty());
        assert!(rejected.is_consistent());
        assert_eq!(sibling.status(), UnitOwnerObservationStatus::Complete);
        assert_eq!(sibling.definitions().len(), 1);
        assert!(!sibling.events().is_empty());
        assert!(sibling.is_consistent());
        assert!(rejected.definition(sibling.definitions()[0].id()).is_none());
    }
}
