//! Mixed terminal paths retain distinct nonempty owner identities and destinations.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerLoopEdgeV1 as Edge, UnitOwnerOrigin,
    UnitOwnerValueEventV1 as Event, UnitOwnerValueV1 as Value,
};
use crate::parser::{Expression, Statement};
use crate::type_checking::UnitStatementId;

const TAIL: &str = "if(other) { continue } else { break }";
fn source(body: &str) -> String {
    format!(
        r#"fun run(flag: Boolean, other: Boolean) {{ val held = "outer"; while(flag) {{ {body} }}; val retired = held }}"#
    )
}
fn check(
    fixture: &Fixture,
    index: usize,
    owned: &crate::ownership_checking::CompilationUnitOwnership,
) {
    let source = fixture.source(index);
    let legacy = &owned.owner_observations()[index];
    assert!(legacy.invariant_normal_join_observation_v1().is_none());
    assert!(legacy.value_observation_v1().is_none() && legacy.loop_observation_v1().is_none());
    assert!(
        legacy.invariant_loop_observation_v1().is_none()
            && legacy.invariant_break_observation_v1().is_none()
    );
    assert!(legacy.invariant_split_continue_observation_v1().is_none());
    assert!(legacy.definitions().is_empty() && legacy.events().is_empty());
    let observed = legacy
        .invariant_split_exit_observation_v1()
        .expect("actual two-arm Continue witness");
    assert!(observed.is_consistent());
    let ast = fixture.parsed[index].ast();
    let (statement, condition, body) = ast
        .statements()
        .iter()
        .find_map(|(id, node)| {
            if let Statement::While {
                condition, body, ..
            } = node.payload()
            {
                Some((id, *condition, *body))
            } else {
                None
            }
        })
        .unwrap();
    let (control, branch_condition, then_body, else_body) = ast
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            if let Expression::If {
                condition,
                then_branch,
                else_branch: Some(else_branch),
                ..
            } = node.payload()
            {
                Some((id, *condition, *then_branch, *else_branch))
            } else {
                None
            }
        })
        .unwrap();
    let statement = UnitStatementId::new(source, statement);
    let body = UnitStatementId::new(source, body);
    let condition = UnitExpressionId::new(source, condition);
    let control = UnitExpressionId::new(source, control);
    let branch_condition = UnitExpressionId::new(source, branch_condition);
    assert_eq!(
        (observed.statement(), observed.condition(), observed.body()),
        (statement, condition, body)
    );
    assert_eq!(
        (observed.control(), observed.branch_condition()),
        (control, branch_condition)
    );
    assert_eq!(
        legacy.status(),
        crate::ownership_checking::UnitOwnerObservationStatus::Unsupported {
            point: UnitDropPoint::AfterStatement(statement),
            reason: crate::ownership_checking::UnitOwnerUnsupportedReason::Syntax
        }
    );
    assert_eq!(observed.entry(), observed.split_boundary());
    assert_eq!(observed.entry(), observed.exit());
    assert_eq!(observed.entry().symbol(), fixture.local(index, "held"));
    let inputs = observed.input_producers();
    assert_eq!(inputs.definitions().len(), 1);
    assert_eq!(
        inputs
            .definition(observed.entry().owner())
            .unwrap()
            .origin(),
        UnitOwnerOrigin::String(UnitExpressionId::new(
            source,
            fixture.expression(index, "\"outer\"")
        ))
    );
    let mut edges = vec![Edge::EnterBody(condition, body)];
    let mut jumps = Vec::new();
    for (branch, arm_body) in [then_body, else_body].into_iter().enumerate() {
        let Statement::ControlBody { elements } = ast.statements().get(arm_body).unwrap().payload()
        else {
            panic!("real arm")
        };
        let [tail] = elements.as_slice() else {
            panic!("one real jump")
        };
        let Statement::Expression { expression } = ast.statements().get(*tail).unwrap().payload()
        else {
            panic!("real tail")
        };
        assert!(matches!(
            ast.expressions().get(*expression).unwrap().payload(),
            Expression::Continue { .. } | Expression::Break { .. }
        ));
        let jump = UnitExpressionId::new(source, *expression);
        let arm_body = UnitStatementId::new(source, arm_body);
        let arm = &observed.arms()[branch];
        assert_eq!(
            (arm.control(), arm.branch(), arm.body(), arm.jump()),
            (control, branch, arm_body, jump)
        );
        for snapshot in [arm.entry(), arm.after_jump(), arm.completion()] {
            assert_eq!(snapshot, observed.entry());
        }
        edges.push(Edge::IfBranch {
            control,
            condition: branch_condition,
            branch,
            body: arm_body,
        });
        let expected_edge = match ast.expressions().get(*expression).unwrap().payload() {
            Expression::Continue { .. } => {
                Edge::ContinueBackedge(UnitDropPoint::ControlTransfer(jump), condition)
            }
            Expression::Break { .. } => Edge::BreakExit(
                UnitDropPoint::ControlTransfer(jump),
                UnitDropPoint::LoopExit(statement),
            ),
            _ => panic!("actual terminal jump"),
        };
        assert_eq!(arm.edge(), expected_edge);
        edges.push(expected_edge);
        assert!(
            owned
                .drops()
                .iter()
                .all(|drop| drop.point() != UnitDropPoint::ControlTransfer(jump))
        );
        jumps.push(jump);
    }
    assert_ne!(jumps[0], jumps[1]);
    edges.push(Edge::ConditionFalseExit(
        condition,
        UnitDropPoint::LoopExit(statement),
    ));
    assert_eq!(observed.edges(), edges);
    let actual_continues = jumps
        .iter()
        .filter(|jump| {
            matches!(
                ast.expressions().get(jump.expression()).unwrap().payload(),
                Expression::Continue { .. }
            )
        })
        .count();
    assert_eq!(
        observed
            .edges()
            .iter()
            .filter(|e| matches!(e, Edge::ContinueBackedge(..)))
            .count(),
        actual_continues
    );
    assert_eq!(
        observed
            .edges()
            .iter()
            .filter(|e| matches!(e, Edge::BreakExit(..)))
            .count(),
        2 - actual_continues
    );
    assert!(
        !observed
            .edges()
            .iter()
            .any(|e| matches!(e, Edge::Backedge(..)))
    );
    let graph = observed.body_transport();
    assert!(graph.is_consistent() && graph.choice().is_none() && graph.suffix().is_empty());
    assert!(
        graph
            .producers()
            .definition(observed.entry().owner())
            .is_none()
    );
    for definition in graph.producers().definitions() {
        assert!(inputs.definition(definition.id()).is_none());
        let UnitOwnerOrigin::String(expression) = definition.origin() else {
            panic!("real string")
        };
        assert_eq!(expression.source_unit(), source);
        let literal = ast.expressions().get(expression.expression()).unwrap();
        let body_span = ast.statements().get(body.statement()).unwrap().span();
        assert!(matches!(literal.payload(), Expression::String { .. }));
        assert_eq!(literal.span().source_id(), body_span.source_id());
        assert!(
            body_span.start() <= literal.span().start() && literal.span().end() <= body_span.end()
        );
        assert!(graph.prefix().iter().any(|event| matches!(event,Event::Create(id,owner) if *id==expression && *owner==definition.id())));
        let drops = graph
            .prefix()
            .iter()
            .filter_map(|event| match event {
                Event::Drop(point, symbol, Value::Definition(owner))
                    if *owner == definition.id() =>
                {
                    Some((*point, *symbol))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), 1);
        for (point, symbol) in drops {
            assert_ne!(symbol, observed.entry().symbol());
            assert!(
                !jumps
                    .iter()
                    .any(|jump| point == UnitDropPoint::ControlTransfer(*jump))
            );
            assert!(owned.drops().iter().any(
                |drop| drop.point() == point && drop.target() == UnitDropTarget::Named(symbol)
            ));
        }
    }
    assert!(
        owned
            .drops()
            .iter()
            .all(|d| d.target() != UnitDropTarget::Named(fixture.local(index, "held")))
    );
    let retirement = ast
        .statements()
        .iter()
        .find_map(|(id, node)| {
            fixture
                .sources
                .slice(node.span())
                .unwrap()
                .starts_with("val retired")
                .then_some(UnitStatementId::new(source, id))
        })
        .unwrap();
    let retired = fixture.local(index, "retired");
    let actual = owned
        .drops()
        .iter()
        .filter(|d| d.target() == UnitDropTarget::Named(retired))
        .collect::<Vec<_>>();
    assert_eq!(actual.len(), 1);
    assert_eq!(actual[0].point(), UnitDropPoint::AfterStatement(retirement));
    // Suffix Drop is legacy compatibility evidence, not a claimed runtime execution.
}

fn assert_source(text: &str, producers: usize, replacements: usize) {
    let fixture = Fixture::new(&[("root.ko", text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok(), "{text}");
    assert_eq!(owned.owner_observations().len(), 1);
    check(&fixture, 0, &owned);
    let graph = owned.owner_observations()[0]
        .invariant_split_exit_observation_v1()
        .unwrap()
        .body_transport();
    assert_eq!(graph.producers().definitions().len(), producers);
    assert_eq!(
        graph
            .prefix()
            .iter()
            .filter(|e| matches!(e, Event::Replace { .. }))
            .count(),
        replacements
    );
    compare(&fixture, &fixture.parts(&owned));
}

#[test]
fn actual_split_exits_preserve_nonempty_owners_at_each_real_destination() {
    let original = r#"fun run(flag: Boolean, other: Boolean) { val held = "outer"; while(flag) { val local = "inner"; if(other) { continue } else { break } }; val retired = held }"#;
    assert_source(original, 1, 0);
    for (left, right) in [
        ("continue", "break"),
        ("break", "continue"),
        ("break", "break"),
    ] {
        for condition in ["flag", "((flag))"] {
            for branch_condition in ["other", "((other))"] {
                for result in ["", ": Unit"] {
                    for (body, producers, replacements) in [
                        (r#"val local = "inner""#, 1, 0),
                        (r#"val local = "inner"; val alias = (local)"#, 1, 0),
                        (
                            r#"var root = "old"; val incoming = "next"; val previous = replace(&root, incoming)"#,
                            2,
                            1,
                        ),
                        (
                            r#"var left = "left"; var right = "right"; val previous = replace(&left, replace(&right, "next"))"#,
                            3,
                            2,
                        ),
                    ] {
                        let text = format!(
                            r#"fun run(flag: Boolean, other: Boolean){result} {{ val held = "outer"; while({condition}) {{ {body}; if({branch_condition}) {{ {left} }} else {{ {right} }} }}; val retired = held }}"#
                        );
                        assert_source(&text, producers, replacements);
                    }
                }
            }
        }
    }
}

fn reject(text: &str) {
    let fixture = Fixture::new(&[("reject.ko", text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok(), "valid negative: {text}");
    assert_eq!(owned.owner_observations().len(), 1);
    assert!(
        owned.owner_observations()[0]
            .invariant_split_exit_observation_v1()
            .is_none(),
        "{text}"
    );
    compare(&fixture, &fixture.parts(&owned));
}
#[test]
fn split_exits_reject_unproved_paths_and_preserve_prior_capabilities() {
    for tail in [
        "if(other) { continue } else { continue }",
        "if(other) { break } else {}",
        "if(other) {} else { break }",
        "if(other) {} else {}",
        "if(flag) { continue } else { break }",
        "if(other) { (break) } else { continue }",
        "if(other) { (continue) } else { break }",
        r#"if(other) { val arm = "arm"; break } else { continue }"#,
        r#"if(other) { break } else { break }; val tail = "tail""#,
        "if(other) { if(flag) { continue } else { break } } else { break }",
        "if(other) { while(flag) { break }; break } else { continue }",
        "if(other) { break }",
        "if(other) { return } else { break }",
    ] {
        reject(&source(&format!(r#"val local = "inner"; {tail}"#)));
    }
    for body in [
        format!(r#"val local = held + "inner"; {TAIL}"#),
        format!(r#"val local: String? = "inner"; {TAIL}"#),
        TAIL.to_string(),
    ] {
        reject(&source(&body));
    }
    for (prefix, suffix) in [
        (r#"var held = "outer""#, "val retired = held"),
        (r#"val held = ("outer")"#, "val retired = held"),
        (r#"val held = "outer""#, "val retired = (held)"),
        (r#"val held = "outer""#, "var retired = held"),
        (r#"val held = "outer""#, "val retired: String? = held"),
        (
            r#"val held = "outer"; val extra = "extra""#,
            "val retired = held",
        ),
    ] {
        reject(&format!(
            r#"fun run(flag: Boolean, other: Boolean) {{ {prefix}; while(flag) {{ val local = "inner"; {TAIL} }}; {suffix} }}"#
        ));
    }
    for tail in ["", "; continue", "; break"] {
        let text = format!(
            r#"fun run(flag: Boolean) {{ val held = "outer"; while(flag) {{ val local = "inner"{tail} }}; val retired = held }}"#
        );
        reject(&text);
        let fixture = Fixture::new(&[("old.ko", &text)]);
        let owned = fixture.shared();
        let o = &owned.owner_observations()[0];
        assert_eq!(
            o.invariant_loop_observation_v1().is_some(),
            tail != "; break"
        );
        assert_eq!(
            o.invariant_break_observation_v1().is_some(),
            tail == "; break"
        );
    }
    let old = source(r#"val local = "inner"; if(other) { continue } else { continue }"#);
    let fixture = Fixture::new(&[("old-split.ko", &old)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    assert_eq!(owned.owner_observations().len(), 1);
    assert!(
        owned.owner_observations()[0]
            .invariant_split_continue_observation_v1()
            .is_some()
    );
    assert!(
        owned.owner_observations()[0]
            .invariant_split_exit_observation_v1()
            .is_none()
    );
}

#[test]
fn split_exit_sources_and_failures_keep_capabilities_separate() {
    let first = source(&format!(
        r#"var root = "inner"; val previous = replace(&root, "next"); {TAIL}"#
    ));
    let second = first.replace("run", "second");
    let fixture = Fixture::new(&[("a.ko", &first), ("b.ko", &second)]);
    let owned = fixture.shared();
    let again = fixture.shared();
    assert!(owned.clone().validate().is_ok() && again.clone().validate().is_ok());
    assert_eq!(owned.owner_observations().len(), 2);
    for index in 0..2 {
        check(&fixture, index, &owned);
        let value = owned.owner_observations()[index]
            .invariant_split_exit_observation_v1()
            .unwrap();
        for foreign in [
            owned.owner_observations()[1 - index]
                .invariant_split_exit_observation_v1()
                .unwrap(),
            again.owner_observations()[index]
                .invariant_split_exit_observation_v1()
                .unwrap(),
        ] {
            for arm in foreign.arms() {
                assert!(
                    value
                        .input_producers()
                        .definition(arm.after_jump().owner())
                        .is_none()
                );
            }
            assert!(
                value
                    .body_transport()
                    .producers()
                    .definition(foreign.body_transport().producers().definitions()[0].id())
                    .is_none()
            );
        }
    }
    compare(&fixture, &fixture.parts(&owned));
    for body in [
        "val local = held",
        "borrow val alias = held",
        r#"var root = "inner"; val old = replace(&root, held)"#,
    ] {
        let text = source(&format!("{body}; {TAIL}"));
        let fixture = Fixture::new(&[("error.ko", &text)]);
        let failed = fixture.shared();
        assert!(failed.clone().validate().is_err(), "{text}");
        assert!(
            failed
                .owner_observations()
                .iter()
                .all(|o| o.invariant_split_exit_observation_v1().is_none())
        );
    }
    for tail in [
        "if(other) { continue } else { val moved = held; break }",
        "if(other) { val moved = held; break } else { continue }",
    ] {
        let text = source(&format!(r#"val local = "inner"; {tail}"#));
        let fixture = Fixture::new(&[("arm-error.ko", &text)]);
        let failed = fixture.shared();
        assert!(failed.clone().validate().is_err(), "{text}");
        assert!(
            failed
                .owner_observations()
                .iter()
                .all(|o| o.invariant_split_exit_observation_v1().is_none())
        );
    }
    let bad = format!("{first}\nfun bad(input: String) {{ val moved = input }}");
    let fixture = Fixture::new(&[("sibling-error.ko", &bad)]);
    let failed = fixture.shared();
    assert!(failed.clone().validate().is_err() && failed.owner_observations().is_empty());
}

// Compile-only damage guards, not a replacement for the actual source witnesses.
#[test]
fn split_exit_seal_rejects_wrong_destinations_and_foreign_arm_evidence() {
    use crate::ownership_checking::compilation_unit::owner_transport::{
        LoopBodyCompletion, LoopBranchExit,
    };
    let text = source(&format!(r#"val local = "inner"; {TAIL}"#));
    let fixture = Fixture::new(&[("root.ko", &text)]);
    let owned = fixture.shared();
    let again = fixture.shared();
    let original = owned.owner_observations()[0]
        .invariant_split_exit_observation_v1()
        .unwrap();
    let foreign = again.owner_observations()[0]
        .invariant_split_exit_observation_v1()
        .unwrap();
    let mut changed = original.clone();
    changed.arms.swap(0, 1);
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.arms[1] = changed.arms[0].clone();
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.arms[0].jump = changed.arms[1].jump;
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.arms[0].edge = Edge::BreakExit(
        UnitDropPoint::ControlTransfer(changed.arms[0].jump),
        UnitDropPoint::LoopExit(changed.statement()),
    );
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.arms[1].edge = Edge::BreakExit(
        UnitDropPoint::ControlTransfer(changed.arms[1].jump),
        UnitDropPoint::LoopExit(changed.body()),
    );
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.arms[0].edge = Edge::ContinueBackedge(
        UnitDropPoint::ControlTransfer(changed.arms[0].jump),
        changed.branch_condition(),
    );
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.arms[1].identity = foreign.arms[1].identity.clone();
    assert!(!changed.is_consistent());
    for slot in 0..3 {
        let mut changed = original.clone();
        let arm = &mut changed.arms[1];
        match slot {
            0 => arm.entry = foreign.entry().clone(),
            1 => arm.after_jump = foreign.entry().clone(),
            _ => arm.completion = foreign.entry().clone(),
        }
        assert!(!changed.is_consistent());
    }
    let mut changed = original.clone();
    changed.split_identity = foreign.split_identity.clone();
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.inputs = foreign.inputs.clone();
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.closed.edges.pop();
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    let LoopBodyCompletion::Conditional { branches, .. } = &mut changed.closed.completion else {
        unreachable!()
    };
    for (arm, branch) in changed.arms.iter_mut().zip(branches) {
        branch.exit = LoopBranchExit::Continue(arm.jump);
        arm.edge = Edge::ContinueBackedge(
            UnitDropPoint::ControlTransfer(arm.jump),
            original.condition(),
        );
    }
    changed.closed.edges =
        changed
            .closed
            .completion
            .edges(changed.statement(), changed.condition(), changed.body());
    assert!(
        changed.closed.is_consistent(),
        "fully self-consistent C/C template"
    );
    assert!(
        !changed.is_consistent(),
        "C/C is not the at-least-one-Break capability"
    );
    // Actual C/C edges cannot masquerade as the expected mixed completion.
    let mut changed = original.clone();
    changed.arms[1].edge = Edge::ContinueBackedge(
        UnitDropPoint::ControlTransfer(changed.arms[1].jump),
        changed.condition(),
    );
    assert!(changed.closed.is_consistent());
    assert!(!changed.is_consistent());
    // Conversely, expected B/B with matching structural edges cannot override actual C/B.
    let mut changed = original.clone();
    let LoopBodyCompletion::Conditional { branches, .. } = &mut changed.closed.completion else {
        unreachable!()
    };
    branches[0].exit = LoopBranchExit::Break(changed.arms[0].jump);
    changed.closed.edges =
        changed
            .closed
            .completion
            .edges(changed.statement(), changed.condition(), changed.body());
    assert!(changed.closed.is_consistent());
    assert!(!changed.is_consistent());
    let two_loops = format!("{text}\n{}", text.replace("run", "second"));
    let two_fixture = Fixture::new(&[("two-loops.ko", &two_loops)]);
    let two_owned = two_fixture.shared();
    assert!(two_owned.clone().validate().is_ok());
    assert_eq!(two_owned.owner_observations().len(), 2);
    let other_loop = two_owned.owner_observations()[1]
        .invariant_split_exit_observation_v1()
        .unwrap()
        .statement();
    assert_ne!(other_loop, original.statement());
    let mut changed = original.clone();
    changed.arms[1].edge = Edge::BreakExit(
        UnitDropPoint::ControlTransfer(changed.arms[1].jump),
        UnitDropPoint::LoopExit(other_loop),
    );
    assert!(!changed.is_consistent());
    let mut legacy = owned.owner_observations()[0].clone();
    legacy.invariant_loop_value = again.owner_observations()[0].invariant_loop_value.clone();
    assert!(legacy.invariant_split_exit_observation_v1().is_none());
    let incomplete =
        crate::ownership_checking::compilation_unit::owner_transport::publish_observations(
            owned.owner_observations().to_vec(),
            true,
            false,
        );
    assert!(
        incomplete[0]
            .invariant_split_exit_observation_v1()
            .is_none()
    );
}
