//! Two actual Continue paths retain independent nonempty owner evidence.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerLoopEdgeV1 as Edge, UnitOwnerOrigin,
    UnitOwnerValueEventV1 as Event, UnitOwnerValueV1 as Value,
};
use crate::parser::{Expression, Statement};
use crate::type_checking::UnitStatementId;

const TAIL: &str = "if (other) { continue } else { continue }";
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
    assert!(legacy.invariant_split_exit_observation_v1().is_none());
    assert!(legacy.value_observation_v1().is_none() && legacy.loop_observation_v1().is_none());
    assert!(
        legacy.invariant_loop_observation_v1().is_none()
            && legacy.invariant_break_observation_v1().is_none()
    );
    assert!(legacy.definitions().is_empty() && legacy.events().is_empty());
    let observed = legacy
        .invariant_split_continue_observation_v1()
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
            Expression::Continue { .. }
        ));
        let jump = UnitExpressionId::new(source, *expression);
        let arm_body = UnitStatementId::new(source, arm_body);
        let arm = &observed.arms()[branch];
        assert_eq!(
            (arm.control(), arm.branch(), arm.body(), arm.jump()),
            (control, branch, arm_body, jump)
        );
        for snapshot in [arm.entry(), arm.after_continue(), arm.completion()] {
            assert_eq!(snapshot, observed.entry());
        }
        edges.push(Edge::IfBranch {
            control,
            condition: branch_condition,
            branch,
            body: arm_body,
        });
        edges.push(Edge::ContinueBackedge(
            UnitDropPoint::ControlTransfer(jump),
            condition,
        ));
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
        .invariant_split_continue_observation_v1()
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
fn actual_split_continues_preserve_each_nonempty_arm_before_discard() {
    // Keep the exact first red source as a permanent ordinary producer witness.
    let original = "fun run(flag: Boolean, other: Boolean) { val held = \"outer\"; while (flag) { val local = \"body\"; if (other) { continue } else { continue } }; val retired = held }";
    assert_source(original, 1, 0);
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
                        r#"fun run(flag: Boolean, other: Boolean){result} {{ val held = "outer"; while({condition}) {{ {body}; if({branch_condition}) {{ continue }} else {{ continue }} }}; val retired = held }}"#
                    );
                    assert_source(&text, producers, replacements);
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
            .invariant_split_continue_observation_v1()
            .is_none(),
        "{text}"
    );
    compare(&fixture, &fixture.parts(&owned));
}
#[test]
fn split_continues_reject_unproved_paths_and_input_shapes() {
    for tail in [
        "if(other) { continue } else { break }",
        "if(other) { break } else { continue }",
        "if(other) { break } else { break }",
        "if(other) {} else { continue }",
        "if(other) { continue } else {}",
        "if(other) {} else {}",
        "if(flag) { continue } else { continue }",
        "if(other) { (continue) } else { continue }",
        r#"if(other) { val arm = "arm"; continue } else { continue }"#,
        r#"if(other) { continue } else { continue }; val tail = "tail""#,
        "if(other) { if(flag) { continue } else { continue } } else { continue }",
        "if(other) { while(flag) { continue }; continue } else { continue }",
        "if(other) { continue }",
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
}

#[test]
fn split_continue_sources_and_failures_keep_capabilities_separate() {
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
            .invariant_split_continue_observation_v1()
            .unwrap();
        for foreign in [
            owned.owner_observations()[1 - index]
                .invariant_split_continue_observation_v1()
                .unwrap(),
            again.owner_observations()[index]
                .invariant_split_continue_observation_v1()
                .unwrap(),
        ] {
            for arm in foreign.arms() {
                assert!(
                    value
                        .input_producers()
                        .definition(arm.after_continue().owner())
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
                .all(|o| o.invariant_split_continue_observation_v1().is_none())
        );
    }
    for tail in [
        "if(other) { continue } else { val moved = held; continue }",
        "if(other) { val moved = held; continue } else { continue }",
    ] {
        let text = source(&format!(r#"val local = "inner"; {tail}"#));
        let fixture = Fixture::new(&[("arm-error.ko", &text)]);
        let failed = fixture.shared();
        assert!(failed.clone().validate().is_err(), "{text}");
        assert!(
            failed
                .owner_observations()
                .iter()
                .all(|o| o.invariant_split_continue_observation_v1().is_none())
        );
    }
    let bad = format!("{first}\nfun bad(input: String) {{ val moved = input }}");
    let fixture = Fixture::new(&[("sibling-error.ko", &bad)]);
    let failed = fixture.shared();
    assert!(failed.clone().validate().is_err() && failed.owner_observations().is_empty());
}

// Compile-only local guards. Real source tests above establish the producer wiring.
#[test]
fn split_continue_seal_rejects_foreign_or_reordered_arm_evidence() {
    let text = source(&format!(r#"val local = "inner"; {TAIL}"#));
    let fixture = Fixture::new(&[("root.ko", &text)]);
    let owned = fixture.shared();
    let again = fixture.shared();
    let original = owned.owner_observations()[0]
        .invariant_split_continue_observation_v1()
        .unwrap();
    let foreign = again.owner_observations()[0]
        .invariant_split_continue_observation_v1()
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
    changed.arms[0].identity = foreign.arms[0].identity.clone();
    assert!(!changed.is_consistent());
    for slot in 0..3 {
        let mut changed = original.clone();
        let arm = &mut changed.arms[1];
        match slot {
            0 => arm.entry = foreign.entry().clone(),
            1 => arm.after_continue = foreign.entry().clone(),
            _ => arm.completion = foreign.entry().clone(),
        }
        assert!(!changed.is_consistent());
    }
    let mut changed = original.clone();
    changed.split_identity = foreign.split_identity.clone();
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.split = foreign.split.clone();
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.inputs = foreign.inputs.clone();
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.closed.edges.pop();
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.control = changed.condition();
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    let crate::ownership_checking::compilation_unit::owner_transport::LoopBodyCompletion::Conditional { branches, .. } = &mut changed.closed.completion else { unreachable!() };
    branches[1].exit =
        crate::ownership_checking::compilation_unit::owner_transport::LoopBranchExit::Break(
            changed.arms[1].jump,
        );
    changed.closed.edges =
        changed
            .closed
            .completion
            .edges(changed.statement(), changed.condition(), changed.body());
    assert!(
        changed.closed.is_consistent(),
        "a self-consistent mixed control template"
    );
    assert!(
        !changed.is_consistent(),
        "a mixed template is not two Continue arms"
    );
    let mut legacy = owned.owner_observations()[0].clone();
    legacy.invariant_loop_value = again.owner_observations()[0].invariant_loop_value.clone();
    assert!(legacy.invariant_split_continue_observation_v1().is_none());
    let incomplete =
        crate::ownership_checking::compilation_unit::owner_transport::publish_observations(
            owned.owner_observations().to_vec(),
            true,
            false,
        );
    assert!(
        incomplete[0]
            .invariant_split_continue_observation_v1()
            .is_none()
    );
}
