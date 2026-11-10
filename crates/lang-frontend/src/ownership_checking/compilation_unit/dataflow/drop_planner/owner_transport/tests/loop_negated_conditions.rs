//! Negation preserves the actual Boolean condition identity, not its operand identity.
use super::*;
pub(super) const ORIGINAL: &str = r#"fun run(flag: Boolean) { while (!flag) { val local = "x" } }"#;
#[test]
fn negated_parameter_retains_actual_loop_condition() {
    let fixture = Fixture::new(&[("root.ko", ORIGINAL)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observed = owned.owner_observations()[0]
        .loop_observation_v1()
        .expect("closed negated condition");
    assert!(observed.is_consistent());
    assert_eq!(
        observed.condition(),
        UnitExpressionId::new(fixture.source(0), fixture.expression(0, "!flag"))
    );
    assert_eq!(observed.body_transport().producers().definitions().len(), 1);
    compare(&fixture, &fixture.parts(&owned));
}

fn source(invariant: bool, condition: &str, tail: &str, body: &str) -> String {
    let parameters = if tail.starts_with("if") {
        "flag: Boolean, other: Boolean"
    } else {
        "flag: Boolean"
    };
    let prefix = if invariant {
        r#"val held = "outer"; "#
    } else {
        ""
    };
    let suffix = if invariant {
        "; val retired = held"
    } else {
        ""
    };
    format!("fun run({parameters}) {{ {prefix}while ({condition}) {{ {body}{tail} }}{suffix} }}")
}
fn closed(
    observation: &crate::ownership_checking::UnitOwnerObservation,
) -> Option<&crate::ownership_checking::UnitOwnerLoopObservationV1> {
    use crate::ownership_checking::compilation_unit::owner_transport::InvariantLoopValueV1 as I;
    if let Some(value) = observation.loop_observation_v1() {
        return Some(value);
    }
    Some(match observation.invariant_loop_value.as_deref()? {
        I::Backedge(value) => {
            assert!(observation.invariant_loop_observation_v1().is_some());
            &value.evidence.closed
        }
        I::Break(value) => {
            assert!(observation.invariant_break_observation_v1().is_some());
            &value.evidence.closed
        }
        I::SplitContinue(value) => {
            assert!(
                observation
                    .invariant_split_continue_observation_v1()
                    .is_some()
            );
            &value.closed
        }
        I::SplitExit(value) => {
            assert!(observation.invariant_split_exit_observation_v1().is_some());
            &value.closed
        }
        I::NormalJoin(value) => {
            assert!(observation.invariant_normal_join_observation_v1().is_some());
            &value.closed
        }
        I::MixedJoin(value) => {
            assert!(observation.invariant_mixed_join_observation_v1().is_some());
            &value.closed
        }
    })
}
fn check(
    fixture: &Fixture,
    index: usize,
    owned: &crate::ownership_checking::CompilationUnitOwnership,
) {
    use crate::ownership_checking::{UnitDropPoint as Point, UnitOwnerLoopEdgeV1 as Edge};
    use crate::parser::{Expression, Statement};
    use crate::type_checking::UnitStatementId;
    let observation = &owned.owner_observations()[index];
    assert!(observation.value_observation_v1().is_none());
    assert!(observation.events().is_empty() && observation.definitions().is_empty());
    if let Some(boundary) = observation.invariant_loop_observation_v1() {
        assert!(boundary.post_loop_transport_v1().is_some());
    }
    if let Some(boundary) = observation.invariant_break_observation_v1() {
        assert!(boundary.post_loop_transport_v1().is_some());
    }
    let value = closed(observation).expect("actual condition and body evidence");
    assert!(value.is_consistent());
    let ast = fixture.parsed[index].ast();
    let source = fixture.source(index);
    let (statement, condition, body) = ast
        .statements()
        .iter()
        .find_map(|(id, node)| {
            if let Statement::While {
                condition, body, ..
            } = node.payload()
            {
                Some((
                    UnitStatementId::new(source, id),
                    UnitExpressionId::new(source, *condition),
                    UnitStatementId::new(source, *body),
                ))
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        (value.statement(), value.condition(), value.body()),
        (statement, condition, body)
    );
    let mut edges = vec![Edge::EnterBody(condition, body)];
    if let Some((control, branch_condition, left, right)) =
        ast.expressions().iter().find_map(|(id, node)| {
            if let Expression::If {
                condition: branch_condition,
                then_branch,
                else_branch: Some(right),
                ..
            } = node.payload()
            {
                Some((
                    UnitExpressionId::new(source, id),
                    UnitExpressionId::new(source, *branch_condition),
                    *then_branch,
                    *right,
                ))
            } else {
                None
            }
        })
    {
        let mut normal = false;
        for (branch, arm) in [left, right].into_iter().enumerate() {
            let arm_id = UnitStatementId::new(source, arm);
            edges.push(Edge::IfBranch {
                control,
                condition: branch_condition,
                branch,
                body: arm_id,
            });
            let Statement::ControlBody { elements } = ast.statements().get(arm).unwrap().payload()
            else {
                panic!("actual arm")
            };
            if elements.is_empty() {
                edges.push(Edge::IfFallthrough(arm_id, control));
                normal = true;
            } else {
                let [tail] = elements.as_slice() else {
                    panic!("one jump")
                };
                let Statement::Expression { expression } =
                    ast.statements().get(*tail).unwrap().payload()
                else {
                    panic!("actual jump")
                };
                let jump = UnitExpressionId::new(source, *expression);
                edges.push(
                    match ast.expressions().get(*expression).unwrap().payload() {
                        Expression::Continue { .. } => {
                            Edge::ContinueBackedge(Point::ControlTransfer(jump), condition)
                        }
                        Expression::Break { .. } => Edge::BreakExit(
                            Point::ControlTransfer(jump),
                            Point::LoopExit(statement),
                        ),
                        _ => panic!("bare jump"),
                    },
                );
            }
        }
        if normal {
            edges.push(Edge::Backedge(Point::AfterStatement(body), condition));
        }
    } else if let Some((jump, is_break)) =
        ast.expressions()
            .iter()
            .find_map(|(id, node)| match node.payload() {
                Expression::Break { .. } => Some((UnitExpressionId::new(source, id), true)),
                Expression::Continue { .. } => Some((UnitExpressionId::new(source, id), false)),
                _ => None,
            })
    {
        edges.push(if is_break {
            Edge::BreakExit(Point::ControlTransfer(jump), Point::LoopExit(statement))
        } else {
            Edge::ContinueBackedge(Point::ControlTransfer(jump), condition)
        });
    } else {
        edges.push(Edge::Backedge(Point::AfterStatement(body), condition));
    }
    edges.push(Edge::ConditionFalseExit(
        condition,
        Point::LoopExit(statement),
    ));
    assert_eq!(value.edges(), edges);
    assert!(value.body_transport().is_consistent());
    for definition in value.body_transport().producers().definitions() {
        assert!(
            matches!(definition.origin(),crate::ownership_checking::UnitOwnerOrigin::String(id) if id.source_unit()==source)
        );
    }
}
#[test]
fn negated_while_and_if_preserve_all_real_completion_edges() {
    let mut count = 0;
    for invariant in [false, true] {
        for (condition, branch) in [
            ("!flag", "!other"),
            ("!flag", "other"),
            ("(!flag)", "!(other)"),
            ("!(!((flag)))", "!(!(!other))"),
        ] {
            let mut tails = vec![String::new(), "; continue".into(), "; break".into()];
            for left in ["", "continue", "break"] {
                for right in ["", "continue", "break"] {
                    tails.push(format!("if ({branch}) {{ {left} }} else {{ {right} }}"));
                }
            }
            for tail in tails {
                for body in [
                    r#"val local = "inner"; "#,
                    r#"var root = "old"; val incoming = "next"; val previous = replace(&root, incoming); "#,
                ] {
                    let text = source(invariant, condition, &tail, body);
                    let fixture = Fixture::new(&[("root.ko", &text)]);
                    let owned = fixture.shared();
                    assert!(owned.clone().validate().is_ok(), "{text}");
                    check(&fixture, 0, &owned);
                    compare(&fixture, &fixture.parts(&owned));
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 192);
}
#[test]
fn negated_conditions_keep_unproved_sources_and_identity_closed() {
    let unrelated = r#"fun run(flag: Boolean) { val local = "x"; if (!flag) {} else {} }"#;
    let fixture = Fixture::new(&[("ordinary.ko", unrelated)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let observation = &owned.owner_observations()[0];
    assert_eq!(
        observation.status(),
        crate::ownership_checking::UnitOwnerObservationStatus::Unsupported {
            point: crate::ownership_checking::UnitDropPoint::AfterExpression(
                UnitExpressionId::new(fixture.source(0), fixture.expression(0, "!flag"))
            ),
            reason: crate::ownership_checking::UnitOwnerUnsupportedReason::Syntax,
        }
    );
    assert!(observation.events().is_empty() && observation.definitions().is_empty());
    assert!(observation.value_observation_v1().is_none() && closed(observation).is_none());
    compare(&fixture, &fixture.parts(&owned));
    for condition in [
        "!true",
        "!(false)",
        "!(flag && flag)",
        "!(flag || flag)",
        "!(flag == flag)",
        "!predicate()",
        "!(if(flag) true else false)",
    ] {
        let text = format!(
            "{}\nfun predicate(): Boolean = true",
            source(false, condition, "", r#"val local = "inner"; "#)
        );
        let fixture = Fixture::new(&[("negative.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok(), "{text}");
        assert!(closed(&owned.owner_observations()[0]).is_none());
        compare(&fixture, &fixture.parts(&owned));
    }
    for (condition, branch) in [
        ("!flag", "flag"),
        ("flag", "!flag"),
        ("!(!flag)", "!(!(!flag))"),
    ] {
        for invariant in [false, true] {
            let text = source(
                invariant,
                condition,
                &format!("if ({branch}) {{ break }} else {{ continue }}"),
                r#"val local = "inner"; "#,
            );
            let fixture = Fixture::new(&[("same.ko", &text)]);
            let owned = fixture.shared();
            assert!(owned.clone().validate().is_ok());
            assert!(closed(&owned.owner_observations()[0]).is_none());
            compare(&fixture, &fixture.parts(&owned));
        }
    }
    for text in [
        ORIGINAL.replace("flag: Boolean", "inout flag: Boolean"),
        ORIGINAL
            .replace("flag: Boolean", "flag: Boolean?")
            .replace("!flag", "!(flag!!)"),
        ORIGINAL
            .replace("{ while", "{ val localFlag = flag; while")
            .replace("!flag", "!localFlag"),
    ] {
        let fixture = Fixture::new(&[("negative.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok(), "{text}");
        assert!(closed(&owned.owner_observations()[0]).is_none());
        compare(&fixture, &fixture.parts(&owned));
    }
    let first = source(
        true,
        "!flag",
        "if (!other) {} else { break }",
        r#"val local = "inner"; "#,
    );
    let second = first.replace("run", "second");
    let fixture = Fixture::new(&[("a.ko", &first), ("b.ko", &second)]);
    let owned = fixture.shared();
    let again = fixture.shared();
    let first_condition = closed(&owned.owner_observations()[0]).unwrap().condition();
    let second_condition = closed(&owned.owner_observations()[1]).unwrap().condition();
    assert_eq!(first_condition.expression(), second_condition.expression());
    assert_ne!(
        first_condition.source_unit(),
        second_condition.source_unit()
    );

    assert!(owned.clone().validate().is_ok() && again.clone().validate().is_ok());
    for index in 0..2 {
        check(&fixture, index, &owned);
        let original = closed(&owned.owner_observations()[index]).unwrap();
        for foreign in [
            closed(&owned.owner_observations()[1 - index]).unwrap(),
            closed(&again.owner_observations()[index]).unwrap(),
        ] {
            for definition in foreign.body_transport().producers().definitions() {
                assert!(
                    original
                        .body_transport()
                        .producers()
                        .definition(definition.id())
                        .is_none()
                );
            }
        }
    }
    for invalid in [
        first.replace("val local = \"inner\"", "val local = held"),
        format!("{first}\nfun bad(input: String) {{ val moved = input }}"),
    ] {
        let fixture = Fixture::new(&[("failed.ko", &invalid)]);
        let failed = fixture.shared();
        assert!(failed.clone().validate().is_err());
        assert!(failed.owner_observations().is_empty());
    }
}
