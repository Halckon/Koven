//! Root swap must transport producer identities, not infer them from cleanup counts.
use super::*;

#[test]
fn root_swap_then_replace_preserves_actual_owners() {
    let fixture = Fixture::new(&[(
        "root.ko",
        r#"fun run(): Unit {
var a = "left"
var b = "right"
swap(&a, &b)
val old = replace(&a, "incoming")
val kept = (b)
}"#,
    )]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = &owned.owner_observations()[0];
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    assert!(observation.is_consistent());
    assert!(observation.value_observation_v1().unwrap().is_consistent());
    let left = literal_owner(&fixture, 0, observation, "\"left\"");
    let right = literal_owner(&fixture, 0, observation, "\"right\"");
    let swaps: Vec<_> = observation
        .events()
        .iter()
        .filter_map(|event| match event {
            UnitOwnerTransportEvent::Swap {
                left_owner,
                right_owner,
                ..
            } => Some((left_owner, right_owner)),
            _ => None,
        })
        .collect();
    assert_eq!(swaps, vec![(&left, &right)]);
    assert!(observation.events().iter().any(|event| matches!(event,
        UnitOwnerTransportEvent::Replace { previous, .. } if previous == &right)));
    assert!(observation.events().iter().any(|event| matches!(event,
        UnitOwnerTransportEvent::Move { owner, .. } if owner == &left)));
    let again = fixture.shared();
    let foreign = again.owner_observations()[0].definitions()[0].id();
    // Missing, duplicated, aliased and foreign exchanges fail conservation replay.
    for mutation in 0..4 {
        let mut broken = observation.clone();
        let index = broken
            .events
            .iter()
            .position(|event| matches!(event, UnitOwnerTransportEvent::Swap { .. }))
            .unwrap();
        match mutation {
            0 => {
                broken.events.remove(index);
            }
            1 => {
                broken.events.insert(index, broken.events[index].clone());
            }
            _ => {
                if let UnitOwnerTransportEvent::Swap {
                    left_owner,
                    right_owner,
                    ..
                } = &mut broken.events[index]
                {
                    *right_owner = if mutation == 2 {
                        left_owner.clone()
                    } else {
                        foreign.clone()
                    };
                }
            }
        }
        assert!(!broken.is_consistent());
    }
    assert_eq!(observation, &fixture.shared().owner_observations()[0]);
}

#[test]
fn root_swap_unsupported_suffix_discards_prefix() {
    let owned = checked(
        r#"fun opaque(): Unit {}
fun run(): Unit { var a = "a"; var b = "b"; swap(&a, &b); opaque() }"#,
    );
    assert!(owned.owner_observations().iter().any(|observation| {
        matches!(
            observation.status(),
            UnitOwnerObservationStatus::Unsupported { .. }
        ) && observation.definitions().is_empty()
            && observation.events().is_empty()
    }));
}

#[test]
fn root_swap_repeated_commit_restores_original_locations() {
    let owned = checked(
        r#"fun run(): Unit {
var a = "a"
var b = "b"
swap(&a, &b)
swap(&a, &b)
val after = replace(&a, "new")
val kept = (b)
}"#,
    );
    let observation = &owned.owner_observations()[0];
    assert_eq!(observation.status(), UnitOwnerObservationStatus::Complete);
    let swaps: Vec<_> = observation
        .events()
        .iter()
        .filter_map(|event| match event {
            UnitOwnerTransportEvent::Swap {
                left_owner,
                right_owner,
                ..
            } => Some((left_owner, right_owner)),
            _ => None,
        })
        .collect();
    assert_eq!(swaps.len(), 2);
    assert_eq!(swaps[0], (swaps[1].1, swaps[1].0));
    assert!(observation.events().iter().any(|event| matches!(event,
        UnitOwnerTransportEvent::Replace { previous, .. } if previous == swaps[0].0)));
}

#[test]
fn root_swap_branch_disagreement_is_not_a_joined_owner() {
    let owned = checked(
        r#"fun run(flag: Boolean): Unit {
var a = "a"
var b = "b"
if (flag) { swap(&a, &b) } else { }
val old = replace(&a, "new")
val kept = (b)
}"#,
    );
    let observation = &owned.owner_observations()[0];
    assert!(matches!(
        observation.status(),
        UnitOwnerObservationStatus::Unsupported { .. }
    ));
    assert!(observation.definitions().is_empty());
    assert!(observation.events().is_empty());
}
