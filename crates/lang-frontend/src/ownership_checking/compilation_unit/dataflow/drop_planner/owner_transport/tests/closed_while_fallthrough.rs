//! Real empty-arm completion and the separate enclosing While body backedge.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerCallable, UnitOwnerLoopEdgeV1 as Edge,
    UnitOwnerLoopObservationV1, UnitOwnerObservationStatus, UnitOwnerOrigin,
    UnitOwnerValueEventV1 as Event, UnitOwnerValueV1 as Value,
};
use crate::parser::{Expression, Statement};
use crate::type_checking::{UnitItemId, UnitStatementId};

fn assert_actual_edges(
    fixture: &Fixture,
    index: usize,
    observed: &UnitOwnerLoopObservationV1,
    kinds: [&str; 2],
    owned: &crate::ownership_checking::CompilationUnitOwnership,
) {
    let source = fixture.source(index);
    let ast = fixture.parsed[index].ast();
    let (statement, condition, body) = ast
        .statements()
        .iter()
        .find_map(|(id, node)| match node.payload() {
            Statement::While {
                condition, body, ..
            } => Some((id, *condition, *body)),
            _ => None,
        })
        .unwrap();
    let statement = UnitStatementId::new(source, statement);
    let condition = UnitExpressionId::new(source, condition);
    let body = UnitStatementId::new(source, body);
    assert_eq!(observed.statement(), statement);
    assert_eq!(observed.condition(), condition);
    assert_eq!(observed.body(), body);
    let (control, branch_condition, branches) = ast
        .expressions()
        .iter()
        .find_map(|(id, node)| match node.payload() {
            Expression::If {
                condition,
                then_branch,
                else_branch: Some(other),
                ..
            } => Some((id, *condition, [*then_branch, *other])),
            _ => None,
        })
        .unwrap();
    let control = UnitExpressionId::new(source, control);
    let branch_condition = UnitExpressionId::new(source, branch_condition);
    let mut expected = vec![Edge::EnterBody(condition, body)];
    for (branch, (arm, kind)) in branches.into_iter().zip(kinds).enumerate() {
        let Statement::ControlBody { elements } = ast.statements().get(arm).unwrap().payload()
        else {
            panic!("actual explicit arm");
        };
        let arm = UnitStatementId::new(source, arm);
        expected.push(Edge::IfBranch {
            control,
            condition: branch_condition,
            branch,
            body: arm,
        });
        if kind.is_empty() {
            assert!(elements.is_empty());
            let no_scope_drop = UnitDropPoint::AfterStatement(arm);
            assert!(
                owned
                    .drops()
                    .iter()
                    .all(|drop| drop.point() != no_scope_drop)
            );
            assert!(observed.body_transport().prefix().iter().all(
                |event| !matches!(event, Event::Drop(point, _, _) if *point == no_scope_drop)
            ));
            expected.push(Edge::IfFallthrough(arm, control));
        } else {
            let [statement] = elements.as_slice() else {
                panic!("one actual jump");
            };
            let Statement::Expression { expression } =
                ast.statements().get(*statement).unwrap().payload()
            else {
                panic!("actual jump expression");
            };
            let jump = UnitDropPoint::ControlTransfer(UnitExpressionId::new(source, *expression));
            let actual = ast.expressions().get(*expression).unwrap().payload();
            expected.push(if kind == "continue" {
                assert!(matches!(actual, Expression::Continue { .. }));
                Edge::ContinueBackedge(jump, condition)
            } else {
                assert_eq!(kind, "break");
                assert!(matches!(actual, Expression::Break { .. }));
                Edge::BreakExit(jump, UnitDropPoint::LoopExit(observed.statement()))
            });
        }
    }
    expected.push(Edge::Backedge(
        UnitDropPoint::AfterStatement(body),
        condition,
    ));
    expected.push(Edge::ConditionFalseExit(
        condition,
        UnitDropPoint::LoopExit(statement),
    ));
    assert_eq!(observed.edges(), expected);
    assert_eq!(
        observed
            .edges()
            .iter()
            .filter(|e| matches!(e, Edge::Backedge(..)))
            .count(),
        1
    );
    assert_eq!(
        observed
            .edges()
            .iter()
            .filter(|e| matches!(e, Edge::IfFallthrough(..)))
            .count(),
        kinds.iter().filter(|kind| kind.is_empty()).count()
    );
    assert!(observed.is_consistent());
    assert_eq!(
        observed.body_transport().producers().callable(),
        UnitOwnerCallable::Function(UnitItemId::new(source, fixture.parsed[index].roots()[0]))
    );
    assert!(
        !observed
            .body_transport()
            .producers()
            .definitions()
            .is_empty()
    );
    for definition in observed.body_transport().producers().definitions() {
        let UnitOwnerOrigin::String(origin) = definition.origin() else {
            panic!("real String origin");
        };
        assert_eq!(origin.source_unit(), source);
    }
}

#[test]
fn real_empty_arm_completion_retains_a_separate_body_backedge() {
    for (left, right) in [
        ("", "break"),
        ("break", ""),
        ("", "continue"),
        ("continue", ""),
        ("", ""),
    ] {
        for (condition, branch_condition, result) in
            [("flag", "exit", ""), ("((exit))", "(flag)", ": Unit")]
        {
            for (prefix, definitions, replacements, moves) in [
                (r#"val local = "local""#, 1, 0, 0),
                (
                    r#"val local = "local"; val first = local; val last = (first)"#,
                    1,
                    0,
                    2,
                ),
                (
                    r#"var root = "old"; val next = "next"; val previous = replace(&root, next); val last = previous"#,
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
                    r#"fun run(flag: Boolean, exit: Boolean){result} {{ while({condition}) {{ {prefix}; if({branch_condition}) {{ {left} }} else {{ {right} }} }} }}
fun sibling() {{ val safe = "safe" }}"#
                );
                let fixture = Fixture::new(&[("root.ko", &text)]);
                let owned = fixture.shared();
                assert!(owned.clone().validate().is_ok(), "{text}");
                assert_eq!(owned.owner_observations().len(), 2);
                let legacy = &owned.owner_observations()[0];
                assert!(matches!(
                    legacy.status(),
                    UnitOwnerObservationStatus::Unsupported { .. }
                ));
                assert!(legacy.events().is_empty() && legacy.definitions().is_empty());
                assert!(legacy.value_observation_v1().is_none());
                let observed = legacy
                    .loop_observation_v1()
                    .expect("actual empty arm completion");
                assert_actual_edges(&fixture, 0, observed, [left, right], &owned);
                let graph = observed.body_transport();
                assert_eq!(graph.producers().definitions().len(), definitions);
                assert!(graph.choice().is_none() && graph.suffix().is_empty());
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
                for definition in graph.producers().definitions() {
                    assert_eq!(graph.prefix().iter().filter(|e| matches!(e, Event::Drop(_, _, Value::Definition(id)) if *id == definition.id())).count(), 1);
                }
                for event in graph.prefix() {
                    if let Event::Drop(point, symbol, _) = event {
                        assert!(!matches!(
                            point,
                            UnitDropPoint::ControlTransfer(_)
                                | UnitDropPoint::BranchEntry { .. }
                                | UnitDropPoint::BranchExit { .. }
                        ));
                        assert!(owned.drops().iter().any(|fact| fact.point() == *point
                            && fact.target() == UnitDropTarget::Named(*symbol)));
                    }
                }
                assert_eq!(
                    owned.owner_observations()[1].status(),
                    UnitOwnerObservationStatus::Complete
                );
                compare(&fixture, &fixture.parts(&owned));
            }
        }
    }
    // Preserve the complete former rejection input, including its sibling callables.
    let fixture = Fixture::new(&[(
        "root.ko",
        r#"fun run(flag: Boolean, exit: Boolean) { while (flag) { val local = "local"; if (exit) { break } else {} } }
fun predicate(): Boolean = true
fun sibling() { val safe = "safe" }"#,
    )]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    assert_eq!(owned.owner_observations().len(), 3);
    assert_actual_edges(
        &fixture,
        0,
        owned.owner_observations()[0].loop_observation_v1().unwrap(),
        ["break", ""],
        &owned,
    );
    assert_eq!(
        owned.owner_observations()[2].status(),
        UnitOwnerObservationStatus::Complete
    );
    compare(&fixture, &fixture.parts(&owned));
}

#[test]
fn normal_arm_rejects_unproved_tails_and_suffixes_without_partial_capability() {
    for tail in [
        "if (exit) {}",
        "(if (exit) {} else { break })",
        r#"if (exit) {} else {}; val suffix = "suffix""#,
        "if (exit) { 0 } else { break }",
        r#"if (exit) { val inner = "inner" } else { break }"#,
        "if (exit) { empty() } else { break }",
        "if (exit) { if (flag) {} else {} } else { break }",
        "if (exit) {} else { (continue) }",
        "if (exit) {} else { return }",
        "if (flag) {} else { break }",
        "if (true) {} else { break }",
        "if (exit) {} else { break }; continue",
    ] {
        let text = format!(
            r#"fun run(flag: Boolean, exit: Boolean) {{ while (flag) {{ val local = "local"; {tail} }} }}
fun empty() {{}}
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
        assert_eq!(owned.owner_observations().len(), 3);
        assert_eq!(
            owned.owner_observations()[2].status(),
            UnitOwnerObservationStatus::Complete
        );
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn normal_arm_catalogs_keep_actual_sources_and_error_atomicity() {
    let fixture = Fixture::new(&[
        (
            "a.ko",
            r#"fun first(flag: Boolean, exit: Boolean) { while(flag) { val a = "a"; if(exit) {} else { break } } }"#,
        ),
        (
            "b.ko",
            r#"fun second(flag: Boolean, exit: Boolean) { while(flag) { val b = "b"; if(exit) { continue } else {} } }"#,
        ),
    ]);
    let first = fixture.shared();
    let repeated = fixture.shared();
    for owned in [&first, &repeated] {
        assert!(owned.clone().validate().is_ok());
        assert_eq!(owned.owner_observations().len(), 2);
        for (index, kinds) in [["", "break"], ["continue", ""]].into_iter().enumerate() {
            let observation = &owned.owner_observations()[index];
            assert_eq!(
                observation.callable(),
                UnitOwnerCallable::Function(UnitItemId::new(
                    fixture.source(index),
                    fixture.parsed[index].roots()[0]
                ))
            );
            assert_actual_edges(
                &fixture,
                index,
                observation.loop_observation_v1().unwrap(),
                kinds,
                owned,
            );
        }
    }
    for index in 0..2 {
        let original = first.owner_observations()[index]
            .loop_observation_v1()
            .unwrap();
        for other in [
            repeated.owner_observations()[index]
                .loop_observation_v1()
                .unwrap(),
            first.owner_observations()[1 - index]
                .loop_observation_v1()
                .unwrap(),
        ] {
            assert!(
                other
                    .body_transport()
                    .producers()
                    .definition(original.body_transport().producers().definitions()[0].id())
                    .is_none()
            );
        }
    }
    let invalid = Fixture::new(&[(
        "root.ko",
        r#"fun run(flag: Boolean, exit: Boolean) { while(flag) { val a = "a"; if(exit) {} else { break } } }
fun bad(input: String) { val moved = input }"#,
    )]);
    let owned = invalid.shared();
    assert!(owned.clone().validate().is_err());
    assert!(owned.owner_observations().is_empty());
}

// Deliberate seal corruption: compile locally, execute only in formal CI.
#[test]
fn fallthrough_loop_seal_rejects_missing_or_misrouted_normal_edges() {
    let fixture = Fixture::new(&[(
        "root.ko",
        r#"fun run(flag: Boolean, exit: Boolean) { while(flag) { val a = "a"; if(exit) {} else {} } }"#,
    )]);
    let owned = fixture.shared();
    let original = owned.owner_observations()[0].loop_observation_v1().unwrap();
    let mut missing = original.clone();
    missing.edges.remove(5);
    assert!(!missing.is_consistent());
    let mut swapped = original.clone();
    swapped.edges.swap(2, 4);
    assert!(!swapped.is_consistent());
    let mut duplicate = original.clone();
    duplicate.edges.insert(5, duplicate.edges[5]);
    assert!(!duplicate.is_consistent());
    let mut target = original.clone();
    let Edge::IfFallthrough(arm, _) = target.edges[2] else {
        panic!("normal edge");
    };
    target.edges[2] = Edge::IfFallthrough(arm, original.condition());
    assert!(!target.is_consistent());
    let mut fake_continue = original.clone();
    fake_continue.edges[2] =
        Edge::ContinueBackedge(UnitDropPoint::AfterStatement(arm), original.condition());
    assert!(!fake_continue.is_consistent());
}
