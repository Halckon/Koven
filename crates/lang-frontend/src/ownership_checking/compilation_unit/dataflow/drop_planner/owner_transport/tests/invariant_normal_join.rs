//! Real normal arm completion, merge and body cleanup are separate boundaries.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerLoopEdgeV1 as Edge, UnitOwnerOrigin,
    UnitOwnerValueEventV1 as Event, UnitOwnerValueV1 as Value,
};
use crate::parser::{Expression, Statement};
use crate::type_checking::UnitStatementId;

const TAIL: &str = "if(other) {} else {}";
fn source(body: &str) -> String {
    format!(
        r#"fun run(flag: Boolean, other: Boolean) {{ val held = "outer"; while(flag) {{ {body} }}; val retired = held }}"#
    )
}
pub(super) fn check(
    fixture: &Fixture,
    index: usize,
    owned: &crate::ownership_checking::CompilationUnitOwnership,
) {
    let source = fixture.source(index);
    let legacy = &owned.owner_observations()[index];
    assert!(legacy.invariant_mixed_join_observation_v1().is_none());
    assert!(legacy.value_observation_v1().is_none() && legacy.loop_observation_v1().is_none());
    assert!(
        legacy.invariant_loop_observation_v1().is_none()
            && legacy.invariant_break_observation_v1().is_none()
    );
    assert!(legacy.invariant_split_continue_observation_v1().is_none());
    assert!(legacy.invariant_split_exit_observation_v1().is_none());
    assert!(legacy.definitions().is_empty() && legacy.events().is_empty());
    let observed = legacy
        .invariant_normal_join_observation_v1()
        .expect("actual two-arm normal join witness");
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
    assert_eq!(observed.entry(), observed.join_boundary());
    assert_eq!(observed.entry(), observed.backedge());
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
    for (branch, arm_body) in [then_body, else_body].into_iter().enumerate() {
        let Statement::ControlBody { elements } = ast.statements().get(arm_body).unwrap().payload()
        else {
            panic!("actual empty arm")
        };
        assert!(elements.is_empty());
        let arm_body = UnitStatementId::new(source, arm_body);
        let arm = &observed.arms()[branch];
        assert_eq!(
            (arm.control(), arm.branch(), arm.body()),
            (control, branch, arm_body)
        );
        assert_eq!(arm.entry(), observed.entry());
        assert_eq!(arm.completion(), observed.entry());
        let edge = Edge::IfFallthrough(arm_body, control);
        assert_eq!(arm.edge(), edge);
        edges.push(Edge::IfBranch {
            control,
            condition: branch_condition,
            branch,
            body: arm_body,
        });
        edges.push(edge);
    }
    assert_ne!(observed.arms()[0].body(), observed.arms()[1].body());
    edges.push(Edge::Backedge(
        UnitDropPoint::AfterStatement(body),
        condition,
    ));
    edges.push(Edge::ConditionFalseExit(
        condition,
        UnitDropPoint::LoopExit(statement),
    ));
    assert_eq!(observed.edges(), edges);
    assert_eq!(observed.edges().len(), 7);
    assert!(
        owned
            .drops()
            .iter()
            .all(|drop| !matches!(drop.point(), UnitDropPoint::ControlTransfer(_)))
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
        .invariant_normal_join_observation_v1()
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
fn actual_normal_arms_preserve_owner_through_the_real_join() {
    let original = r#"fun run(flag: Boolean, other: Boolean) { val held = "outer"; while(flag) { val local = "inner"; if(other) {} else {} }; val retired = held }"#;
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
                        r#"fun run(flag: Boolean, other: Boolean){result} {{ val held = "outer"; while({condition}) {{ {body}; if({branch_condition}) {{}} else {{}} }}; val retired = held }}"#
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
            .invariant_normal_join_observation_v1()
            .is_none(),
        "{text}"
    );
    compare(&fixture, &fixture.parts(&owned));
}
#[test]
fn normal_join_rejects_unproved_paths_and_preserves_prior_capabilities() {
    for tail in [
        "if(other) { continue } else { continue }",
        "if(other) { continue } else { break }",
        "if(other) { break } else { continue }",
        "if(other) { break } else { break }",
        "if(other) { continue } else {}",
        "if(other) {} else { continue }",
        "if(other) { break } else {}",
        "if(other) {} else { break }",
        "if(flag) {} else {}",
        r#"if(other) { val arm = "arm" } else {}"#,
        r#"if(other) {} else { val arm = "arm" }"#,
        r#"if(other) {} else {}; val tail = "tail""#,
        "if(other) { if(flag) {} else {} } else {}",
        "if(other) { while(flag) { break } } else {}",
        "if(other) {}",
        "if(other) { return } else {}",
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
    for (left, right) in [
        ("continue", "continue"),
        ("continue", "break"),
        ("break", "continue"),
        ("break", "break"),
    ] {
        let old = source(&format!(
            r#"val local = "inner"; if(other) {{ {left} }} else {{ {right} }}"#
        ));
        let fixture = Fixture::new(&[("old-split.ko", &old)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok());
        let old = &owned.owner_observations()[0];
        assert!(old.invariant_normal_join_observation_v1().is_none());
        assert_eq!(
            old.invariant_split_continue_observation_v1().is_some(),
            left == "continue" && right == "continue"
        );
        assert_eq!(
            old.invariant_split_exit_observation_v1().is_some(),
            left == "break" || right == "break"
        );
    }
    let empty = r#"fun run(flag: Boolean, other: Boolean) { while(flag) { val local = "inner"; if(other) {} else {} } }"#;
    reject(empty);
    let fixture = Fixture::new(&[("empty.ko", empty)]);
    let owned = fixture.shared();
    assert!(
        owned.owner_observations()[0]
            .loop_observation_v1()
            .is_some()
    );
}

#[test]
fn normal_join_sources_and_failures_keep_capabilities_separate() {
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
            .invariant_normal_join_observation_v1()
            .unwrap();
        for foreign in [
            owned.owner_observations()[1 - index]
                .invariant_normal_join_observation_v1()
                .unwrap(),
            again.owner_observations()[index]
                .invariant_normal_join_observation_v1()
                .unwrap(),
        ] {
            for arm in foreign.arms() {
                assert!(
                    value
                        .input_producers()
                        .definition(arm.completion().owner())
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
                .all(|o| o.invariant_normal_join_observation_v1().is_none())
        );
    }
    for tail in [
        "if(other) {} else { val moved = held }",
        "if(other) { val moved = held } else {}",
    ] {
        let text = source(&format!(r#"val local = "inner"; {tail}"#));
        let fixture = Fixture::new(&[("arm-error.ko", &text)]);
        let failed = fixture.shared();
        assert!(failed.clone().validate().is_err(), "{text}");
        assert!(
            failed
                .owner_observations()
                .iter()
                .all(|o| o.invariant_normal_join_observation_v1().is_none())
        );
    }
    let bad = format!("{first}\nfun bad(input: String) {{ val moved = input }}");
    let fixture = Fixture::new(&[("sibling-error.ko", &bad)]);
    let failed = fixture.shared();
    assert!(failed.clone().validate().is_err() && failed.owner_observations().is_empty());
}

// Compile-only guards; real source tests above supply the production evidence.
#[test]
fn normal_join_seal_rejects_wrong_paths_and_foreign_snapshots() {
    use crate::ownership_checking::compilation_unit::owner_transport::{
        LoopBodyCompletion, LoopBranchExit,
    };
    let text = format!(
        "{}\n{}",
        source(&format!(r#"val local = "inner"; {TAIL}"#)),
        r#"fun terminal(flag: Boolean) { while(flag) { val local = "jump"; continue } }"#
    );
    let fixture = Fixture::new(&[("root.ko", &text)]);
    let owned = fixture.shared();
    let again = fixture.shared();
    let original = owned.owner_observations()[0]
        .invariant_normal_join_observation_v1()
        .unwrap();
    let foreign = again.owner_observations()[0]
        .invariant_normal_join_observation_v1()
        .unwrap();
    for slot in 0..10 {
        let mut changed = original.clone();
        match slot {
            0 => changed.join = foreign.join.clone(),
            1 => changed.backedge = foreign.backedge.clone(),
            2 => changed.join_identity = foreign.join_identity.clone(),
            3 => changed.split_identity = foreign.split_identity.clone(),
            4 => changed.arms[0].entry = foreign.arms[0].entry.clone(),
            5 => changed.arms[1].completion = foreign.arms[1].completion.clone(),
            6 => changed.arms[1].identity = foreign.arms[1].identity.clone(),
            7 => changed.inputs = foreign.inputs.clone(),
            8 => changed.arms.swap(0, 1),
            _ => changed.arms[1] = changed.arms[0].clone(),
        }
        assert!(!changed.is_consistent());
    }
    let mut changed = original.clone();
    changed.arms[0].edge = Edge::IfFallthrough(changed.arms[0].body(), changed.branch_condition());
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.closed.edges.pop();
    assert!(!changed.is_consistent());
    // A self-consistent different completion may not upgrade this exact normal capability.
    let mut changed = original.clone();
    let LoopBodyCompletion::Conditional { branches, .. } = &mut changed.closed.completion else {
        unreachable!()
    };
    branches[0].exit = LoopBranchExit::Continue(UnitExpressionId::new(
        fixture.source(0),
        fixture.expression(0, "continue"),
    ));
    changed.closed.edges =
        changed
            .closed
            .completion
            .edges(changed.statement(), changed.condition(), changed.body());
    assert!(changed.closed.is_consistent());
    assert!(!changed.is_consistent());
    let mut legacy = owned.owner_observations()[0].clone();
    legacy.invariant_loop_value = again.owner_observations()[0].invariant_loop_value.clone();
    assert!(legacy.invariant_normal_join_observation_v1().is_none());
    let incomplete =
        crate::ownership_checking::compilation_unit::owner_transport::publish_observations(
            owned.owner_observations().to_vec(),
            true,
            false,
        );
    assert!(
        incomplete[0]
            .invariant_normal_join_observation_v1()
            .is_none()
    );
}
