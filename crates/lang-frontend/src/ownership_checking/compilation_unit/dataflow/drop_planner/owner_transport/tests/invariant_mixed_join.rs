//! Only the actual normal survivor reaches the join; the jump keeps its own boundary.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerLoopEdgeV1 as Edge, UnitOwnerMixedArmBoundaryV1 as Arm,
    UnitOwnerOrigin, UnitOwnerValueEventV1 as Event, UnitOwnerValueV1 as Value,
};
use crate::parser::{Expression, Statement};
use crate::type_checking::UnitStatementId;

const TAIL: &str = "if(other) {} else { continue }";
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
    assert!(legacy.value_observation_v1().is_none() && legacy.loop_observation_v1().is_none());
    assert!(
        legacy.invariant_loop_observation_v1().is_none()
            && legacy.invariant_break_observation_v1().is_none()
    );
    assert!(legacy.invariant_split_continue_observation_v1().is_none());
    assert!(legacy.invariant_normal_join_observation_v1().is_none());
    assert!(legacy.invariant_split_exit_observation_v1().is_none());
    assert!(legacy.definitions().is_empty() && legacy.events().is_empty());
    let observed = legacy
        .invariant_mixed_join_observation_v1()
        .expect("actual surviving join and distinct jump witness");
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
    let mut normals = 0;
    let mut jumps = 0;
    let mut continues = 0;
    for (branch, arm_body) in [then_body, else_body].into_iter().enumerate() {
        let Statement::ControlBody { elements } = ast.statements().get(arm_body).unwrap().payload()
        else {
            panic!("actual arm")
        };
        let arm_body = UnitStatementId::new(source, arm_body);
        let arm = &observed.arms()[branch];
        assert_eq!(
            (arm.control(), arm.branch(), arm.body()),
            (control, branch, arm_body)
        );
        assert_eq!(arm.entry(), observed.entry());
        assert_eq!(arm.completion(), observed.entry());
        let edge = if elements.is_empty() {
            assert!(matches!(arm, Arm::Normal(_)));
            normals += 1;
            assert_eq!(arm.completion(), observed.join_boundary());
            Edge::IfFallthrough(arm_body, control)
        } else {
            let [tail] = elements.as_slice() else {
                panic!("one actual jump")
            };
            let Statement::Expression { expression } =
                ast.statements().get(*tail).unwrap().payload()
            else {
                panic!("actual tail")
            };
            let jump = UnitExpressionId::new(source, *expression);
            let Arm::Jump(actual) = arm else {
                panic!("actual jump variant")
            };
            assert_eq!(actual.jump(), jump);
            assert_eq!(actual.after_jump(), observed.entry());
            jumps += 1;
            match ast.expressions().get(*expression).unwrap().payload() {
                Expression::Continue { .. } => {
                    continues += 1;
                    Edge::ContinueBackedge(UnitDropPoint::ControlTransfer(jump), condition)
                }
                Expression::Break { .. } => Edge::BreakExit(
                    UnitDropPoint::ControlTransfer(jump),
                    UnitDropPoint::LoopExit(statement),
                ),
                _ => panic!("bare Continue or Break"),
            }
        };
        assert_eq!(arm.edge(), edge);
        edges.push(Edge::IfBranch {
            control,
            condition: branch_condition,
            branch,
            body: arm_body,
        });
        edges.push(edge);
    }
    assert_eq!((normals, jumps), (1, 1));
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
    assert_eq!(
        observed
            .edges()
            .iter()
            .filter(|edge| matches!(edge, Edge::ContinueBackedge(..)))
            .count(),
        continues
    );
    assert_eq!(
        observed
            .edges()
            .iter()
            .filter(|edge| matches!(edge, Edge::BreakExit(..)))
            .count(),
        1 - continues
    );
    assert_eq!(
        observed
            .edges()
            .iter()
            .filter(|edge| matches!(edge, Edge::Backedge(..)))
            .count(),
        1
    );
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
        .invariant_mixed_join_observation_v1()
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
fn actual_mixed_arms_preserve_the_surviving_join_and_jump() {
    let original = r#"fun run(flag: Boolean, other: Boolean) { val held = "outer"; while(flag) { val local = "inner"; if(other) {} else { continue } }; val retired = held }"#;
    assert_source(original, 1, 0);
    for (left, right) in [
        ("", "continue"),
        ("continue", ""),
        ("", "break"),
        ("break", ""),
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
            .invariant_mixed_join_observation_v1()
            .is_none(),
        "{text}"
    );
    compare(&fixture, &fixture.parts(&owned));
}
#[test]
fn mixed_join_rejects_unproved_paths_and_preserves_prior_capabilities() {
    for tail in [
        "if(other) { continue } else { continue }",
        "if(other) { continue } else { break }",
        "if(other) { break } else { continue }",
        "if(other) { break } else { break }",
        "if(other) {} else {}",
        "if(flag) {} else { continue }",
        "if(other) { (continue) } else {}",
        "if(other) {} else { (break) }",
        r#"if(other) { val arm = "arm"; continue } else {}"#,
        r#"if(other) { break } else { val arm = "arm" }"#,
        r#"if(other) {} else { continue }; val tail = "tail""#,
        "if(other) { if(flag) {} else {} } else { continue }",
        "if(other) { while(flag) { break } } else { continue }",
        "if(other) { continue }",
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
        assert!(old.invariant_mixed_join_observation_v1().is_none());
        assert_eq!(
            old.invariant_split_continue_observation_v1().is_some(),
            left == "continue" && right == "continue"
        );
        assert_eq!(
            old.invariant_split_exit_observation_v1().is_some(),
            left == "break" || right == "break"
        );
    }
    let normal = source(r#"val local = "inner"; if(other) {} else {}"#);
    let fixture = Fixture::new(&[("normal.ko", &normal)]);
    let owned = fixture.shared();
    assert!(
        owned.owner_observations()[0]
            .invariant_normal_join_observation_v1()
            .is_some()
    );
    assert!(
        owned.owner_observations()[0]
            .invariant_mixed_join_observation_v1()
            .is_none()
    );
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
fn mixed_join_sources_and_failures_keep_capabilities_separate() {
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
            .invariant_mixed_join_observation_v1()
            .unwrap();
        for foreign in [
            owned.owner_observations()[1 - index]
                .invariant_mixed_join_observation_v1()
                .unwrap(),
            again.owner_observations()[index]
                .invariant_mixed_join_observation_v1()
                .unwrap(),
        ] {
            for arm in foreign.arms() {
                if let Arm::Jump(jump) = arm {
                    assert!(
                        value
                            .input_producers()
                            .definition(jump.after_jump().owner())
                            .is_none()
                    );
                }
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
                .all(|o| o.invariant_mixed_join_observation_v1().is_none())
        );
    }
    for tail in [
        "if(other) {} else { val moved = held; continue }",
        "if(other) { val moved = held; break } else {}",
    ] {
        let text = source(&format!(r#"val local = "inner"; {tail}"#));
        let fixture = Fixture::new(&[("arm-error.ko", &text)]);
        let failed = fixture.shared();
        assert!(failed.clone().validate().is_err(), "{text}");
        assert!(
            failed
                .owner_observations()
                .iter()
                .all(|o| o.invariant_mixed_join_observation_v1().is_none())
        );
    }
    let bad = format!("{first}\nfun bad(input: String) {{ val moved = input }}");
    let fixture = Fixture::new(&[("sibling-error.ko", &bad)]);
    let failed = fixture.shared();
    assert!(failed.clone().validate().is_err() && failed.owner_observations().is_empty());
}
// Compile-only guards; the ordinary source tests above prove actual producer wiring.
#[test]
fn mixed_join_seal_rejects_wrong_variants_and_foreign_snapshots() {
    use crate::ownership_checking::compilation_unit::owner_transport::{
        LoopBodyCompletion, LoopBranchExit,
    };
    use crate::ownership_checking::{UnitOwnerJumpArmBoundaryV1, UnitOwnerNormalArmBoundaryV1};
    let text = format!(
        "{}\n{}",
        source(&format!(r#"val local = "inner"; {TAIL}"#)),
        r#"fun terminal(flag: Boolean) { while(flag) { val local = "jump"; break } }"#
    );
    let fixture = Fixture::new(&[("root.ko", &text)]);
    let owned = fixture.shared();
    let again = fixture.shared();
    let original = owned.owner_observations()[0]
        .invariant_mixed_join_observation_v1()
        .unwrap();
    let foreign = again.owner_observations()[0]
        .invariant_mixed_join_observation_v1()
        .unwrap();
    for slot in 0..9 {
        let mut changed = original.clone();
        match slot {
            0 => changed.join = foreign.join.clone(),
            1 => changed.backedge = foreign.backedge.clone(),
            2 => changed.join_identity = foreign.join_identity.clone(),
            3 => changed.split_identity = foreign.split_identity.clone(),
            4 => changed.inputs = foreign.inputs.clone(),
            5 => changed.arms[0] = foreign.arms[0].clone(),
            6 => changed.arms[1] = foreign.arms[1].clone(),
            7 => changed.arms.swap(0, 1),
            _ => changed.arms[1] = changed.arms[0].clone(),
        }
        assert!(!changed.is_consistent());
    }
    for slot in 0..5 {
        let mut changed = original.clone();
        let Arm::Jump(jump) = &mut changed.arms[1] else {
            unreachable!()
        };
        match slot {
            0 => jump.after_jump = foreign.entry().clone(),
            1 => jump.completion = foreign.entry().clone(),
            2 => jump.jump = original.control(),
            3 => {
                jump.edge = Edge::ContinueBackedge(
                    UnitDropPoint::ControlTransfer(jump.jump),
                    original.branch_condition(),
                )
            }
            _ => {
                jump.edge = Edge::BreakExit(
                    UnitDropPoint::ControlTransfer(jump.jump),
                    UnitDropPoint::LoopExit(original.body()),
                )
            }
        }
        assert!(!changed.is_consistent());
    }
    let mut changed = original.clone();
    let Arm::Normal(normal) = &mut changed.arms[0] else {
        unreachable!()
    };
    normal.edge = Edge::IfFallthrough(normal.body, original.branch_condition());
    assert!(!changed.is_consistent());
    let mut changed = original.clone();
    changed.closed.edges.pop();
    assert!(!changed.is_consistent());
    // Self-consistent NN must not become a mixed capability.
    let mut changed = original.clone();
    let Arm::Jump(jump) = changed.arms[1].clone() else {
        unreachable!()
    };
    changed.arms[1] = Arm::Normal(UnitOwnerNormalArmBoundaryV1 {
        identity: jump.identity,
        control: jump.control,
        branch: jump.branch,
        body: jump.body,
        edge: Edge::IfFallthrough(jump.body, jump.control),
        entry: jump.entry,
        completion: jump.completion,
    });
    let LoopBodyCompletion::Conditional { branches, .. } = &mut changed.closed.completion else {
        unreachable!()
    };
    branches[1].exit = LoopBranchExit::Fallthrough;
    changed.closed.edges =
        changed
            .closed
            .completion
            .edges(changed.statement(), changed.condition(), changed.body());
    assert!(changed.closed.is_consistent());
    assert!(!changed.is_consistent());
    // Self-consistent two-terminal completion is a different capability too.
    let mut changed = original.clone();
    let Arm::Normal(normal) = changed.arms[0].clone() else {
        unreachable!()
    };
    let jump = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "break"));
    changed.arms[0] = Arm::Jump(UnitOwnerJumpArmBoundaryV1 {
        identity: normal.identity,
        control: normal.control,
        branch: normal.branch,
        body: normal.body,
        jump,
        edge: Edge::BreakExit(
            UnitDropPoint::ControlTransfer(jump),
            UnitDropPoint::LoopExit(original.statement()),
        ),
        entry: normal.entry,
        after_jump: original.entry().clone(),
        completion: normal.completion,
    });
    let LoopBodyCompletion::Conditional { branches, .. } = &mut changed.closed.completion else {
        unreachable!()
    };
    branches[0].exit = LoopBranchExit::Break(jump);
    changed.closed.edges =
        changed
            .closed
            .completion
            .edges(changed.statement(), changed.condition(), changed.body());
    assert!(changed.closed.is_consistent());
    assert!(!changed.is_consistent());
    // Expected Break edges cannot change an actually observed Continue.
    let mut changed = original.clone();
    let Arm::Jump(actual) = &changed.arms[1] else {
        unreachable!()
    };
    let LoopBodyCompletion::Conditional { branches, .. } = &mut changed.closed.completion else {
        unreachable!()
    };
    branches[1].exit = LoopBranchExit::Break(actual.jump);
    changed.closed.edges =
        changed
            .closed
            .completion
            .edges(changed.statement(), changed.condition(), changed.body());
    assert!(changed.closed.is_consistent());
    assert!(!changed.is_consistent());
    // A genuine Break arm with only its destination changed to another real While.
    let breaks = source(r#"val local = "inner"; if(other) {} else { break }"#);
    let two = format!("{breaks}\n{}", breaks.replace("run", "second"));
    let breaks_fixture = Fixture::new(&[("breaks.ko", &two)]);
    let breaks_owned = breaks_fixture.shared();
    assert!(breaks_owned.clone().validate().is_ok());
    let original_break = breaks_owned.owner_observations()[0]
        .invariant_mixed_join_observation_v1()
        .unwrap();
    let other_loop = breaks_owned.owner_observations()[1]
        .invariant_mixed_join_observation_v1()
        .unwrap()
        .statement();
    assert_ne!(original_break.statement(), other_loop);
    let mut changed = original_break.clone();
    let Arm::Jump(jump) = &mut changed.arms[1] else {
        unreachable!()
    };
    jump.edge = Edge::BreakExit(
        UnitDropPoint::ControlTransfer(jump.jump),
        UnitDropPoint::LoopExit(other_loop),
    );
    assert!(changed.closed.is_consistent());
    assert!(!changed.is_consistent());
    let mut legacy = owned.owner_observations()[0].clone();
    legacy.invariant_loop_value = again.owner_observations()[0].invariant_loop_value.clone();
    assert!(legacy.invariant_mixed_join_observation_v1().is_none());
    let incomplete =
        crate::ownership_checking::compilation_unit::owner_transport::publish_observations(
            owned.owner_observations().to_vec(),
            true,
            false,
        );
    assert!(
        incomplete[0]
            .invariant_mixed_join_observation_v1()
            .is_none()
    );
}
