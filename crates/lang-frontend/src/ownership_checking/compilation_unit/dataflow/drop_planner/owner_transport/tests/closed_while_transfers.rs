//! Real terminal jumps in a closed While body, not a false result inferred as a backedge.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerCallable, UnitOwnerLoopEdgeV1 as Edge,
    UnitOwnerObservationStatus, UnitOwnerValueEventV1 as Event,
};
use crate::parser::Statement;
use crate::type_checking::UnitItemId;

#[test]
fn real_while_terminal_jumps_publish_closed_transport() {
    for jump in ["continue", "break"] {
        for condition in ["flag", "((flag))"] {
            for result in ["", ": Unit"] {
                for (body, definitions, replacements, moves) in [
                    (r#"val local = "local""#, 1, 0, 0),
                    (
                        r#"val local = "local"; val alias = local; val last = (alias)"#,
                        1,
                        0,
                        2,
                    ),
                    (
                        r#"var root = "old"; val next = "next"; val previous = replace(&root, next); val forwarded = (previous)"#,
                        2,
                        1,
                        2,
                    ),
                    (
                        r#"var left = "left"; var right = "right"; val previous = replace(&left, replace(&right, "next"))"#,
                        3,
                        2,
                        0,
                    ),
                ] {
                    let text = format!(
                        r#"fun run(flag: Boolean){result} {{ while ({condition}) {{ {body}; {jump} }} }}
fun sibling() {{ val safe = "safe" }}"#
                    );
                    let fixture = Fixture::new(&[("root.ko", &text)]);
                    let owned = fixture.shared();
                    assert!(owned.clone().validate().is_ok(), "{text}");
                    let legacy = &owned.owner_observations()[0];
                    assert!(matches!(
                        legacy.status(),
                        UnitOwnerObservationStatus::Unsupported { .. }
                    ));
                    assert!(legacy.events().is_empty() && legacy.definitions().is_empty());
                    assert!(legacy.value_observation_v1().is_none());
                    let observed = legacy
                        .loop_observation_v1()
                        .unwrap_or_else(|| panic!("missing actual terminal jump: {text}"));
                    let expression =
                        UnitExpressionId::new(fixture.source(0), fixture.expression(0, jump));
                    let target = UnitDropPoint::LoopExit(observed.statement());
                    let expected = if jump == "continue" {
                        Edge::ContinueBackedge(
                            UnitDropPoint::ControlTransfer(expression),
                            observed.condition(),
                        )
                    } else {
                        Edge::BreakExit(UnitDropPoint::ControlTransfer(expression), target)
                    };
                    assert_eq!(
                        observed.edges(),
                        &[
                            Edge::EnterBody(observed.condition(), observed.body()),
                            expected,
                            Edge::ConditionFalseExit(observed.condition(), target),
                        ]
                    );
                    assert_eq!(
                        observed.condition(),
                        UnitExpressionId::new(fixture.source(0), fixture.expression(0, condition))
                    );
                    let ast = fixture.parsed[0].ast();
                    let Statement::While {
                        condition: actual_condition,
                        body: actual_body,
                        ..
                    } = ast
                        .statements()
                        .get(observed.statement().statement())
                        .unwrap()
                        .payload()
                    else {
                        panic!("actual While");
                    };
                    assert_eq!(*actual_condition, observed.condition().expression());
                    assert_eq!(*actual_body, observed.body().statement());
                    assert!(observed.is_consistent());
                    let graph = observed.body_transport();
                    assert_eq!(graph.producers().definitions().len(), definitions);
                    assert_eq!(
                        graph
                            .prefix()
                            .iter()
                            .filter(|e| matches!(e, Event::Replace { .. }))
                            .count(),
                        replacements
                    );
                    assert_eq!(
                        graph
                            .prefix()
                            .iter()
                            .filter(|e| matches!(e, Event::Move(..)))
                            .count(),
                        moves
                    );
                    let drops = graph
                        .prefix()
                        .iter()
                        .filter_map(|e| match e {
                            Event::Drop(point, symbol, _) => Some((*point, *symbol)),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(drops.len(), definitions);
                    for (point, symbol) in drops {
                        assert!(
                            !matches!(point, UnitDropPoint::ControlTransfer(_)),
                            "ASAP retirement must not move to the jump"
                        );
                        assert!(owned.drops().iter().any(|fact| fact.point() == point
                            && fact.target() == UnitDropTarget::Named(symbol)));
                    }
                    assert_eq!(
                        owned.owner_observations()[1].status(),
                        UnitOwnerObservationStatus::Complete
                    );
                    assert!(
                        owned.owner_observations()[1]
                            .loop_observation_v1()
                            .is_none()
                    );
                    compare(&fixture, &fixture.parts(&owned));
                }
            }
        }
    }
}

#[test]
fn terminal_jump_collection_rejects_nonterminal_or_unclosed_bodies() {
    for body in [
        "continue",
        "break",
        r#"val local = "x"; (continue)"#,
        r#"val local = "x"; (break)"#,
        r#"val local = "x"; continue; val suffix = "unreached""#,
        r#"val local = "x"; break; val suffix = "unreached""#,
        r#"val local = "x"; continue; break"#,
        r#"val local = "x"; break; continue"#,
        r#"val local = "x"; return"#,
        r#"val local = "x"; if (flag) { continue } else { break }"#,
        r#"val local = "x"; while (flag) { val inner = "inner"; continue }; break"#,
        r#"val local = "x"; val callback = move { local }; continue"#,
        r#"val local: String? = "x"; continue"#,
        r#"var local = "x"; local = "next"; break"#,
        r#"val local = "x"; make(); continue"#,
        r#"val local = "x"; val other = make(); break"#,
        r#"val local = "x"; "temporary"; continue"#,
    ] {
        let text = format!(
            r#"fun run(flag: Boolean) {{ while (flag) {{ {body} }} }}
fun make(): String = "made"
fun sibling() {{ val safe = "safe" }}"#
        );
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(
            owned.clone().validate().is_ok(),
            "{text}: {:?}",
            owned.diagnostics()
        );
        assert!(
            owned.owner_observations()[0]
                .loop_observation_v1()
                .is_none(),
            "{text}"
        );
        let sibling = UnitOwnerCallable::Function(UnitItemId::new(
            fixture.source(0),
            fixture.parsed[0].roots()[2],
        ));
        let sibling = owned
            .owner_observations()
            .iter()
            .find(|observation| observation.callable() == sibling)
            .unwrap();
        assert_eq!(sibling.status(), UnitOwnerObservationStatus::Complete);
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn terminal_loop_tokens_stay_local_and_errors_withhold_all_observations() {
    let fixture = Fixture::new(&[
        (
            "a.ko",
            r#"fun first(flag: Boolean) { while (flag) { val a = "a"; continue } }"#,
        ),
        (
            "b.ko",
            r#"fun second(flag: Boolean) { while (flag) { val b = "b"; break } }"#,
        ),
    ]);
    let first = fixture.shared();
    let repeated = fixture.shared();
    assert!(first.clone().validate().is_ok());
    assert!(repeated.clone().validate().is_ok());
    assert_eq!(first.owner_observations().len(), 2);
    assert_eq!(repeated.owner_observations().len(), 2);
    for (index, observed) in first.owner_observations().iter().enumerate() {
        let observed = observed.loop_observation_v1().unwrap();
        for other in [
            repeated.owner_observations()[index]
                .loop_observation_v1()
                .unwrap(),
            first.owner_observations()[1 - index]
                .loop_observation_v1()
                .unwrap(),
        ] {
            let token = observed.body_transport().producers().definitions()[0].id();
            assert!(
                other
                    .body_transport()
                    .producers()
                    .definition(token)
                    .is_none()
            );
        }
    }
    for jump in ["continue", "break"] {
        let text = format!(
            r#"fun run(flag: Boolean) {{ while (flag) {{ val a = "a"; {jump} }} }}
fun bad(input: String) {{ val moved = input }}"#
        );
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_err());
        assert!(owned.owner_observations().is_empty());
    }
}

// Deliberately corrupted seals are compiled locally, executed only in formal CI.
#[test]
fn terminal_jump_seal_rejects_changed_kind_point_target_and_catalog() {
    for jump in ["continue", "break"] {
        let text =
            format!(r#"fun run(flag: Boolean) {{ while (flag) {{ val a = "a"; {jump} }} }}"#);
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let left = fixture.shared();
        let right = fixture.shared();
        let original = left.owner_observations()[0].loop_observation_v1().unwrap();
        let actual_jump = UnitExpressionId::new(fixture.source(0), fixture.expression(0, jump));
        for wrong in [
            Edge::Backedge(
                UnitDropPoint::AfterStatement(original.body()),
                original.condition(),
            ),
            Edge::ContinueBackedge(
                UnitDropPoint::ControlTransfer(original.condition()),
                original.condition(),
            ),
            Edge::BreakExit(
                UnitDropPoint::ControlTransfer(original.condition()),
                UnitDropPoint::LoopExit(original.statement()),
            ),
            Edge::BreakExit(
                UnitDropPoint::ControlTransfer(actual_jump),
                UnitDropPoint::LoopExit(original.body()),
            ),
            if jump == "continue" {
                Edge::BreakExit(
                    UnitDropPoint::ControlTransfer(actual_jump),
                    UnitDropPoint::LoopExit(original.statement()),
                )
            } else {
                Edge::ContinueBackedge(
                    UnitDropPoint::ControlTransfer(actual_jump),
                    original.condition(),
                )
            },
        ] {
            let mut changed = original.clone();
            changed.edges[1] = wrong;
            assert!(!changed.is_consistent());
        }
        let mut changed = original.clone();
        changed.transport = right.owner_observations()[0]
            .loop_observation_v1()
            .unwrap()
            .transport
            .clone();
        assert!(!changed.is_consistent());
        let mut changed = original.clone();
        let Some(Event::Drop(point, _, _)) = changed
            .transport
            .prefix
            .iter_mut()
            .find(|e| matches!(e, Event::Drop(..)))
        else {
            panic!("actual drop");
        };
        *point = UnitDropPoint::ControlTransfer(actual_jump);
        assert!(
            !changed.is_consistent(),
            "no ControlTransfer Drop capability was added"
        );
    }
}
