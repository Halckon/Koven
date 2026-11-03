//! Formation witnesses use current tokens after replacement, not declaration spans.
use super::*;

fn function(owned: &CompilationUnitOwnership) -> &UnitOwnerObservation {
    owned
        .owner_observations()
        .iter()
        .find(|observation| matches!(observation.callable(), UnitOwnerCallable::Function(_)))
        .unwrap()
}

#[test]
fn capture_formation_preserves_replaced_and_forwarded_owner_lineage() {
    let fixture = Fixture::new(&[(
        "root.ko",
        r#"fun run(): Unit {
var a = "old"
val incoming = "new"
val old = replace(&a, incoming)
val callback = move { a + old }
val forwarded = (callback)
}"#,
    )]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = function(&owned);
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    let old = literal_owner(&fixture, 0, observation, "\"old\"");
    let incoming = literal_owner(&fixture, 0, observation, "\"new\"");
    let captures = observation
        .events()
        .iter()
        .filter_map(|event| match event {
            UnitOwnerTransportEvent::Capture {
                expression,
                environment,
                inputs,
            } => Some((*expression, environment.clone(), inputs.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(expression, environment, inputs)] = captures.as_slice() else {
        panic!("one actual formation")
    };
    assert_eq!(
        *expression,
        UnitExpressionId::new(fixture.source(0), fixture.expression(0, "move { a + old }"))
    );
    assert_eq!(
        observation
            .definition(environment.clone())
            .unwrap()
            .origin(),
        UnitOwnerOrigin::Closure(*expression)
    );
    assert_eq!(
        inputs,
        &vec![
            (fixture.symbol(0, "a"), incoming.clone()),
            (fixture.symbol(0, "old"), old.clone())
        ]
    );
    assert_ne!(*environment, incoming);
    assert_ne!(*environment, old);
    let drops = observation
        .events()
        .iter()
        .filter_map(|event| match event {
            UnitOwnerTransportEvent::CaptureDrop {
                environment: actual,
                source,
                owner,
                ..
            } => {
                assert_eq!(actual, environment);
                Some((*source, owner.clone()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(drops, inputs.iter().rev().cloned().collect::<Vec<_>>());
    assert!(observation.events().iter().any(|event| matches!(event,
        UnitOwnerTransportEvent::Binding { symbol, owner, .. }
            if *symbol == fixture.symbol(0, "forwarded") && owner == environment)));
    let lambdas = owned
        .owner_observations()
        .iter()
        .filter(|observation| matches!(observation.callable(), UnitOwnerCallable::Lambda(_)))
        .collect::<Vec<_>>();
    assert_eq!(
        lambdas.len(),
        1,
        "the real lambda body must publish an observation"
    );
    assert!(lambdas.iter().all(|observation| matches!(
        observation.status(),
        UnitOwnerObservationStatus::Unsupported { .. }
    )));
}

#[test]
fn capture_replay_rejects_swapped_inputs_missing_release_and_foreign_tokens() {
    let fixture = Fixture::new(&[(
        "root.ko",
        r#"fun run(): Unit {
val a = "same"
val b = "same"
val callback = move { a + b }
}"#,
    )]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = function(&owned);
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    let mut damaged = observation.clone();
    let UnitOwnerTransportEvent::Capture { inputs, .. } = damaged
        .events
        .iter_mut()
        .find(|event| matches!(event, UnitOwnerTransportEvent::Capture { .. }))
        .unwrap()
    else {
        unreachable!()
    };
    let left = inputs[0].1.clone();
    inputs[0].1 = inputs[1].1.clone();
    inputs[1].1 = left;
    assert!(!damaged.is_consistent());
    let mut damaged = observation.clone();
    damaged
        .events
        .retain(|event| !matches!(event, UnitOwnerTransportEvent::CaptureDrop { .. }));
    assert!(!damaged.is_consistent());
    let other = fixture.shared();
    assert_eq!(owned, other);
    let foreign = function(&other).definitions()[0].id();
    assert!(observation.definition(foreign.clone()).is_none());
    let mut damaged = observation.clone();
    let UnitOwnerTransportEvent::Capture { inputs, .. } = damaged
        .events
        .iter_mut()
        .find(|event| matches!(event, UnitOwnerTransportEvent::Capture { .. }))
        .unwrap()
    else {
        unreachable!()
    };
    inputs[0].1 = foreign;
    assert!(!damaged.is_consistent());
    assert_ne!(observation, &damaged);
}

#[test]
fn capture_boundaries_discard_the_entire_observation_without_language_errors() {
    for suffix in [
        "val callback = { a }",
        "val n = 1; val callback = move { n; a }",
        "val inner = move { a }; val outer = move { inner }",
        "move { a }",
        "val callback = move { a }; callback()",
    ] {
        let text = format!(
            "fun run(): Unit {{ var a = \"old\"; val old = replace(&a, \"new\"); {suffix} }}"
        );
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok(), "{suffix}");
        let observation = function(&owned);
        assert!(
            matches!(
                observation.status(),
                UnitOwnerObservationStatus::Unsupported { .. }
            ),
            "{suffix}"
        );
        assert!(observation.definitions().is_empty());
        assert!(observation.events().is_empty());
        assert!(observation.is_consistent());
    }
}

#[test]
fn capture_formation_consumes_the_actual_owned_parameter() {
    let fixture = Fixture::new(&[(
        "root.ko",
        "fun run(own seed: String): Unit { val callback = move { seed } }",
    )]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = function(&owned);
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    let symbol = fixture.symbol(0, "seed");
    let parameter = observation
        .definitions()
        .iter()
        .find(|definition| definition.origin() == UnitOwnerOrigin::Parameter(symbol))
        .unwrap()
        .id();
    let inputs = observation
        .events()
        .iter()
        .find_map(|event| match event {
            UnitOwnerTransportEvent::Capture { inputs, .. } => Some(inputs),
            _ => None,
        })
        .unwrap();
    assert_eq!(inputs, &vec![(symbol, parameter)]);
}

#[test]
fn capture_environment_and_release_tokens_reject_matching_foreign_definitions() {
    let fixture = Fixture::new(&[(
        "root.ko",
        "fun run(own seed: String): Unit { val callback = move { seed } }",
    )]);
    let owned = fixture.shared();
    let other = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    assert!(other.clone().validate().is_ok());
    assert_eq!(owned, other);
    let observation = function(&owned);
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    let foreign_environment = function(&other)
        .events()
        .iter()
        .find_map(|event| match event {
            UnitOwnerTransportEvent::Capture { environment, .. } => Some(environment.clone()),
            _ => None,
        })
        .unwrap();
    let foreign_owner = function(&other)
        .events()
        .iter()
        .find_map(|event| match event {
            UnitOwnerTransportEvent::CaptureDrop { owner, .. } => Some(owner.clone()),
            _ => None,
        })
        .unwrap();
    assert!(
        observation
            .definition(foreign_environment.clone())
            .is_none()
    );
    assert!(observation.definition(foreign_owner.clone()).is_none());
    for field in [
        "Capture.environment",
        "CaptureDrop.environment",
        "CaptureDrop.owner",
    ] {
        let mut damaged = observation.clone();
        let changed = damaged
            .events
            .iter_mut()
            .find_map(|event| match (field, event) {
                ("Capture.environment", UnitOwnerTransportEvent::Capture { environment, .. }) => {
                    *environment = foreign_environment.clone();
                    Some(())
                }
                (
                    "CaptureDrop.environment",
                    UnitOwnerTransportEvent::CaptureDrop { environment, .. },
                ) => {
                    *environment = foreign_environment.clone();
                    Some(())
                }
                ("CaptureDrop.owner", UnitOwnerTransportEvent::CaptureDrop { owner, .. }) => {
                    *owner = foreign_owner.clone();
                    Some(())
                }
                _ => None,
            });
        assert_eq!(changed, Some(()), "fixture must exercise {field}");
        assert!(!damaged.is_consistent(), "foreign token in {field}");
        assert_ne!(observation, &damaged);
    }
}

#[test]
fn independent_live_environments_cannot_exchange_capture_slots() {
    let fixture = Fixture::new(&[(
        "root.ko",
        r#"fun run(): Unit {
val a = "same"
val b = "same"
val first = move { a }
val second = move { b }
val forwarded_first = first
val forwarded_second = second
}"#,
    )]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = function(&owned);
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    let captures = observation
        .events()
        .iter()
        .filter_map(|event| match event {
            UnitOwnerTransportEvent::Capture {
                environment,
                inputs,
                ..
            } => Some((environment.clone(), inputs.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(first, first_inputs), (second, second_inputs)] = captures.as_slice() else {
        panic!("two independent formations")
    };
    assert_ne!(first, second);
    assert_eq!(first_inputs.len(), 1);
    assert_eq!(second_inputs.len(), 1);
    let (first_source, first_owner) = &first_inputs[0];
    let (second_source, second_owner) = &second_inputs[0];
    assert_ne!(first_source, second_source);
    assert_ne!(first_owner, second_owner);
    let second_binding = observation
        .events()
        .iter()
        .position(|event| {
            matches!(event,
                UnitOwnerTransportEvent::Binding { symbol, owner, .. }
                    if *symbol == fixture.symbol(0, "second") && owner == second
            )
        })
        .unwrap();
    let first_release = observation
        .events()
        .iter()
        .position(|event| {
            matches!(event,
                UnitOwnerTransportEvent::CaptureDrop { environment, .. } if environment == first
            )
        })
        .unwrap();
    let second_release = observation
        .events()
        .iter()
        .position(|event| {
            matches!(event,
                UnitOwnerTransportEvent::CaptureDrop { environment, .. } if environment == second
            )
        })
        .unwrap();
    assert!(
        second_binding < first_release && first_release < second_release,
        "the second environment must be live when the first captured value is released"
    );
    for field in ["environment", "source", "owner"] {
        let mut damaged = observation.clone();
        let event = damaged
            .events
            .iter_mut()
            .find(|event| {
                matches!(event,
                    UnitOwnerTransportEvent::CaptureDrop { environment, .. } if environment == first
                )
            })
            .unwrap();
        let UnitOwnerTransportEvent::CaptureDrop {
            environment,
            source,
            owner,
            ..
        } = event
        else {
            unreachable!()
        };
        assert_eq!(*source, *first_source);
        assert_eq!(&*owner, first_owner);
        match field {
            "environment" => *environment = second.clone(),
            "source" => *source = *second_source,
            "owner" => *owner = second_owner.clone(),
            _ => unreachable!(),
        }
        assert!(
            !damaged.is_consistent(),
            "wrong live environment slot {field}"
        );
        assert_ne!(observation, &damaged);
    }
}
