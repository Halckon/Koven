//! Actual conditional jump traversal after a closed owner prefix.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerCallable, UnitOwnerLoopEdgeV1 as Edge,
    UnitOwnerLoopObservationV1, UnitOwnerObservationStatus, UnitOwnerOrigin,
    UnitOwnerValueEventV1 as Event, UnitOwnerValueV1 as Value,
};
use crate::parser::{Expression, Statement};
use crate::type_checking::{UnitItemId, UnitStatementId};

fn terminal_jump(fixture: &Fixture, body: crate::ast::StatementId) -> UnitExpressionId {
    let ast = fixture.parsed[0].ast();
    let Statement::ControlBody { elements } = ast.statements().get(body).unwrap().payload() else {
        panic!("actual control body");
    };
    let [statement] = elements.as_slice() else {
        panic!("one terminal statement");
    };
    let Statement::Expression { expression } = ast.statements().get(*statement).unwrap().payload()
    else {
        panic!("actual jump");
    };
    UnitExpressionId::new(fixture.source(0), *expression)
}

fn assert_actual_loop_source(
    fixture: &Fixture,
    index: usize,
    observed: &UnitOwnerLoopObservationV1,
) {
    let source = fixture.source(index);
    let callable =
        UnitOwnerCallable::Function(UnitItemId::new(source, fixture.parsed[index].roots()[0]));
    assert_eq!(observed.body_transport().producers().callable(), callable);
    for definition in observed.body_transport().producers().definitions() {
        let UnitOwnerOrigin::String(origin) = definition.origin() else {
            panic!("actual String construction");
        };
        assert_eq!(origin.source_unit(), source);
    }
    let (statement, condition, body) = fixture.parsed[index]
        .ast()
        .statements()
        .iter()
        .find_map(|(id, node)| match node.payload() {
            Statement::While {
                condition, body, ..
            } => Some((id, *condition, *body)),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        observed.statement(),
        UnitStatementId::new(source, statement)
    );
    assert_eq!(
        observed.condition(),
        UnitExpressionId::new(source, condition)
    );
    assert_eq!(observed.body(), UnitStatementId::new(source, body));
    for edge in observed.edges() {
        match *edge {
            Edge::EnterBody(condition, body) => {
                assert_eq!(condition.source_unit(), source);
                assert_eq!(body.source_unit(), source);
            }
            Edge::IfBranch {
                control,
                condition,
                body,
                ..
            } => {
                assert_eq!(control.source_unit(), source);
                assert_eq!(condition.source_unit(), source);
                assert_eq!(body.source_unit(), source);
            }
            Edge::ContinueBackedge(UnitDropPoint::ControlTransfer(jump), condition) => {
                assert_eq!(jump.source_unit(), source);
                assert_eq!(condition.source_unit(), source);
            }
            Edge::BreakExit(
                UnitDropPoint::ControlTransfer(jump),
                UnitDropPoint::LoopExit(target),
            ) => {
                assert_eq!(jump.source_unit(), source);
                assert_eq!(target.source_unit(), source);
            }
            Edge::ConditionFalseExit(condition, UnitDropPoint::LoopExit(target)) => {
                assert_eq!(condition.source_unit(), source);
                assert_eq!(target.source_unit(), source);
            }
            _ => panic!("unexpected conditional loop edge"),
        }
    }
}

#[test]
fn real_while_tail_if_records_both_completed_jump_edges() {
    for (left, right) in [
        ("continue", "break"),
        ("break", "continue"),
        ("continue", "continue"),
        ("break", "break"),
    ] {
        for (loop_condition, branch_condition, result) in
            [("flag", "exit", ""), ("((flag))", "(exit)", ": Unit")]
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
                    r#"fun run(flag: Boolean, exit: Boolean){result} {{ while ({loop_condition}) {{ {prefix}; if ({branch_condition}) {{ {left} }} else {{ {right} }} }} }}
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
                    .unwrap_or_else(|| panic!("missing actual terminal If edges: {text}"));
                assert_actual_loop_source(&fixture, 0, observed);
                let (control, condition, then_body, else_body) = fixture.parsed[0]
                    .ast()
                    .expressions()
                    .iter()
                    .find_map(|(id, node)| match node.payload() {
                        Expression::If {
                            condition,
                            then_branch,
                            else_branch: Some(else_branch),
                            ..
                        } => Some((id, *condition, *then_branch, *else_branch)),
                        _ => None,
                    })
                    .unwrap();
                let control = UnitExpressionId::new(fixture.source(0), control);
                let condition = UnitExpressionId::new(fixture.source(0), condition);
                let branches = [then_body, else_body];
                let jumps = branches.map(|body| terminal_jump(&fixture, body));
                assert_ne!(jumps[0], jumps[1]);
                let successor = |jump, kind| {
                    if kind == "continue" {
                        Edge::ContinueBackedge(
                            UnitDropPoint::ControlTransfer(jump),
                            observed.condition(),
                        )
                    } else {
                        Edge::BreakExit(
                            UnitDropPoint::ControlTransfer(jump),
                            UnitDropPoint::LoopExit(observed.statement()),
                        )
                    }
                };
                assert_eq!(
                    observed.edges(),
                    &[
                        Edge::EnterBody(observed.condition(), observed.body()),
                        Edge::IfBranch {
                            control,
                            condition,
                            branch: 0,
                            body: UnitStatementId::new(fixture.source(0), then_body)
                        },
                        successor(jumps[0], left),
                        Edge::IfBranch {
                            control,
                            condition,
                            branch: 1,
                            body: UnitStatementId::new(fixture.source(0), else_body)
                        },
                        successor(jumps[1], right),
                        Edge::ConditionFalseExit(
                            observed.condition(),
                            UnitDropPoint::LoopExit(observed.statement())
                        ),
                    ]
                );
                assert!(observed.is_consistent());
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
}

#[test]
fn conditional_loop_rejects_nonindependent_or_open_arms_atomically() {
    for (parameters, tail) in [
        ("flag: Boolean", "if (flag) { break } else { continue }"),
        (
            "flag: Boolean, exit: Boolean",
            "if ((flag)) { break } else { continue }",
        ),
        (
            "flag: Boolean, exit: Boolean",
            "if (!exit) { break } else { continue }",
        ),
        (
            "flag: Boolean, exit: Boolean",
            "if (true) { break } else { continue }",
        ),
        (
            "flag: Boolean, exit: Boolean",
            "if (predicate()) { break } else { continue }",
        ),
        ("flag: Boolean, exit: Boolean", "if (exit) { break }"),
        (
            "flag: Boolean, exit: Boolean",
            "if (exit) { return } else { continue }",
        ),
        (
            "flag: Boolean, exit: Boolean",
            "if (exit) { break } else { return }",
        ),
        (
            "flag: Boolean, exit: Boolean",
            "if (exit) { (break) } else { continue }",
        ),
        (
            "flag: Boolean, exit: Boolean",
            "if (exit) { break } else { (continue) }",
        ),
        (
            "flag: Boolean, exit: Boolean",
            "if (exit) { break; continue } else { continue }",
        ),
        (
            "flag: Boolean, exit: Boolean",
            "if (exit) { break } else { continue; break }",
        ),
        (
            "flag: Boolean, exit: Boolean",
            r#"if (exit) { val branch = "branch"; break } else { continue }"#,
        ),
        (
            "flag: Boolean, exit: Boolean",
            "if (exit) { if (exit) { break } else { continue } } else { continue }",
        ),
        (
            "flag: Boolean, exit: Boolean",
            r#"if (exit) { break } else { continue }; val suffix = "unreached""#,
        ),
        (
            "flag: Boolean, exit: Boolean, extra: Boolean",
            "if (exit) { break } else { continue }",
        ),
        (
            "flag: Boolean, borrow exit: Boolean",
            "if (exit) { break } else { continue }",
        ),
    ] {
        let text = format!(
            r#"fun run({parameters}) {{ while (flag) {{ val local = "local"; {tail} }} }}
fun predicate(): Boolean = true
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
        assert_eq!(
            owned.owner_observations().last().unwrap().status(),
            UnitOwnerObservationStatus::Complete
        );
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn conditional_loop_catalogs_are_separate_and_errors_withhold_publication() {
    let fixture = Fixture::new(&[
        (
            "a.ko",
            r#"fun first(flag: Boolean, exit: Boolean) { while(flag) { val a = "a"; if(exit) { break } else { continue } } }"#,
        ),
        (
            "b.ko",
            r#"fun second(flag: Boolean, exit: Boolean) { while(flag) { val b = "b"; if(exit) { continue } else { break } } }"#,
        ),
    ]);
    let first = fixture.shared();
    let repeated = fixture.shared();
    for owned in [&first, &repeated] {
        assert!(owned.clone().validate().is_ok());
        assert_eq!(owned.owner_observations().len(), 2);
    }
    for index in 0..2 {
        let expected_callable = UnitOwnerCallable::Function(UnitItemId::new(
            fixture.source(index),
            fixture.parsed[index].roots()[0],
        ));
        for owned in [&first, &repeated] {
            let observation = &owned.owner_observations()[index];
            assert_eq!(observation.callable(), expected_callable);
            assert_actual_loop_source(&fixture, index, observation.loop_observation_v1().unwrap());
        }
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
        r#"fun run(flag: Boolean, exit: Boolean) { while(flag) { val a = "a"; if(exit) { break } else { continue } } }
fun bad(input: String) { val moved = input }"#,
    )]);
    let owned = invalid.shared();
    assert!(owned.clone().validate().is_err());
    assert!(owned.owner_observations().is_empty());
}

// Deliberate seal corruption: compile locally, execute only in formal CI.
#[test]
fn conditional_loop_seal_rejects_missing_swapped_or_foreign_edges() {
    let fixture = Fixture::new(&[(
        "root.ko",
        r#"fun run(flag: Boolean, exit: Boolean) { while(flag) { val a = "a"; if(exit) { break } else { continue } } }"#,
    )]);
    let left = fixture.shared();
    let right = fixture.shared();
    let original = left.owner_observations()[0].loop_observation_v1().unwrap();
    let mut missing = original.clone();
    missing.edges.remove(3);
    assert!(!missing.is_consistent());
    let mut duplicate = original.clone();
    duplicate.edges[3] = duplicate.edges[1];
    assert!(!duplicate.is_consistent());
    let mut swapped = original.clone();
    swapped.edges.swap(2, 4);
    assert!(!swapped.is_consistent());
    let mut control = original.clone();
    let Edge::IfBranch {
        condition,
        branch,
        body,
        ..
    } = control.edges[1]
    else {
        panic!("conditional edge");
    };
    control.edges[1] = Edge::IfBranch {
        control: original.condition(),
        condition,
        branch,
        body,
    };
    assert!(!control.is_consistent());
    let mut foreign = original.clone();
    foreign.transport = right.owner_observations()[0]
        .loop_observation_v1()
        .unwrap()
        .transport
        .clone();
    assert!(!foreign.is_consistent());
    let mut drop = original.clone();
    let Some(Event::Drop(point, _, _)) = drop
        .transport
        .prefix
        .iter_mut()
        .find(|event| matches!(event, Event::Drop(..)))
    else {
        panic!("actual Drop");
    };
    *point = UnitDropPoint::ControlTransfer(condition);
    assert!(!drop.is_consistent());
}
