//! V1 projection follows the real swap commit and its ordered call-return cleanup.
use super::*;

fn swap_fixture() -> Fixture {
    Fixture::new(&[(
        "root.ko",
        r#"fun run(): Unit {
var a = "left"
var b = "right"
swap(&a, &b)
}"#,
    )])
}

#[test]
fn value_swap_preserves_producers_and_both_call_return_drops() {
    let fixture = swap_fixture();
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let legacy = &owned.owner_observations()[0];
    let graph = legacy
        .value_observation_v1()
        .expect("closed String root swap");
    assert!(graph.is_consistent());
    assert_eq!(graph.producers().definitions(), legacy.definitions());
    let left_owner = literal_owner(&fixture, 0, legacy, "\"left\"");
    let right_owner = literal_owner(&fixture, 0, legacy, "\"right\"");
    let index = graph
        .prefix
        .iter()
        .position(|e| matches!(e, Event::Swap { .. }))
        .unwrap();
    let Event::Swap {
        call,
        left,
        right,
        left_value: Value::Definition(l),
        right_value: Value::Definition(r),
    } = &graph.prefix[index]
    else {
        panic!("real definition swap")
    };
    assert_eq!(l, &left_owner);
    assert_eq!(r, &right_owner);
    let drops: Vec<_> = graph.prefix[index + 1..]
        .iter()
        .map(|event| {
            let Event::Drop(UnitDropPoint::CallReturn(c), symbol, Value::Definition(owner)) = event
            else {
                panic!("actual contiguous call return cleanup")
            };
            assert_eq!(c, call);
            (*symbol, owner.clone())
        })
        .collect();
    assert_eq!(drops.len(), 2);
    assert!(drops.contains(&(*left, right_owner)));
    assert!(drops.contains(&(*right, left_owner)));
    assert_eq!(
        graph.validate().unwrap(),
        [vec![Slot::Dropped; 2], vec![Slot::Dropped; 2]]
    );
}

#[test]
fn value_swap_rejects_missing_repeated_or_aliased_commit_and_cleanup() {
    let fixture = swap_fixture();
    let owned = fixture.shared();
    let graph = owned.owner_observations()[0]
        .value_observation_v1()
        .unwrap();
    let index = graph
        .prefix
        .iter()
        .position(|e| matches!(e, Event::Swap { .. }))
        .unwrap();
    for mutation in 0..7 {
        let mut bad = graph.clone();
        match mutation {
            0 => {
                bad.prefix.remove(index);
            }
            1 => {
                bad.prefix.insert(index, bad.prefix[index].clone());
            }
            2 => {
                if let Event::Swap {
                    left_value,
                    right_value,
                    ..
                } = &mut bad.prefix[index]
                {
                    *right_value = left_value.clone();
                }
            }
            3 => {
                bad.prefix.pop();
            }
            4 => {
                let last = bad.prefix.last().unwrap().clone();
                bad.prefix.push(last);
            }
            5 => {
                let UnitOwnerOrigin::String(wrong_call) =
                    graph.producers().definitions()[0].origin()
                else {
                    unreachable!()
                };
                if let Event::Drop(point, _, _) = &mut bad.prefix[index + 1] {
                    *point = UnitDropPoint::CallReturn(wrong_call);
                }
            }
            _ => {
                if let Event::Drop(_, _, owner) = &mut bad.prefix[index + 1] {
                    *owner = Value::Definition(
                        fixture.shared().owner_observations()[0].definitions()[0].id(),
                    );
                }
            }
        }
        assert!(!bad.is_consistent(), "mutation {mutation}");
    }
}

#[test]
fn value_swap_composes_with_replace_and_repeated_swap() {
    let owned = checked(
        r#"fun run(): Unit {
var a = "left"
var b = "right"
swap(&a, &b)
val old = replace(&a, "incoming")
swap(&a, &b)
val kept = (a)
}"#,
    );
    let graph = owned.owner_observations()[0]
        .value_observation_v1()
        .unwrap();
    assert!(graph.is_consistent());
    assert_eq!(
        graph
            .prefix
            .iter()
            .filter(|event| matches!(event, Event::Swap { .. }))
            .count(),
        2
    );
    assert_eq!(graph.producers().definitions().len(), 3);
}
