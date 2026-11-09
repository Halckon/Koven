//! Public real mixed arm destinations and the one-survivor If join.
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitOwnerLoopEdgeV1 as Edge,
        UnitOwnerMixedArmBoundaryV1 as Arm, UnitOwnerObservationStatus, UnitOwnerOrigin,
        UnitOwnerValueEventV1 as Event, UnitOwnerValueV1 as Value,
        check_compilation_unit_ownership,
    },
    parser::{Expression, Statement},
    source::SourceMap,
    type_checking::{
        UnitExpressionId, UnitStatementId, check_compilation_unit_types,
        complete_compilation_unit_types, standard_environments,
    },
};

fn check_split_inputs(rooted: bool, body: &str, suffix: &str, admitted: bool) {
    let text = format!(
        r#"fun first(flag: Boolean, other: Boolean) {{ val held = "outer"; while((flag)) {{ {body} }}; {suffix} }}"#
    );
    let second = text.replace("first", "second");
    let mut sources = SourceMap::new();
    let ids = [
        sources.add_source("a.ko", &text).unwrap(),
        sources.add_source("b.ko", &second).unwrap(),
    ];
    let parsed = ids.map(|id| {
        super::parser_test_assertions::parse_file_twice(&sources, id, "invariant While owner")
    });
    assert!(parsed.iter().all(|p| p.diagnostics().is_empty()));
    let source_count = if rooted { 2 } else { 1 };
    let inputs = parsed
        .iter()
        .take(source_count)
        .enumerate()
        .map(|(i, p)| {
            if rooted {
                SourceUnitInput::new("root", if i == 0 { "a.ko" } else { "b.ko" }, ids[i], p)
            } else {
                SourceUnitInput::memory_document(ids[i], p)
            }
        })
        .collect::<Vec<_>>();
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty());
    let completed =
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
    assert!(completed.is_same_analysis(&typed));
    let typed = typed.validate().unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    assert!(owned.is_compatible_with(&typed));
    assert!(owned.clone().validate().is_ok(), "rooted={rooted}: {text}");
    assert_eq!(owned.owner_observations().len(), source_count);
    for (i, legacy) in owned.owner_observations().iter().enumerate() {
        let source = index.source_units()[i].id();
        assert_eq!(index.source_units()[i].source_id(), ids[i]);
        let ast = parsed[i].ast();
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
        let statement = UnitStatementId::new(source, statement);
        let body = UnitStatementId::new(source, body);
        let condition = UnitExpressionId::new(source, condition);
        assert_eq!(
            legacy.status(),
            UnitOwnerObservationStatus::Unsupported {
                point: UnitDropPoint::AfterStatement(statement),
                reason: lang_frontend::ownership_checking::UnitOwnerUnsupportedReason::Syntax
            }
        );
        assert!(legacy.events().is_empty() && legacy.definitions().is_empty());
        assert!(legacy.value_observation_v1().is_none() && legacy.loop_observation_v1().is_none());
        assert!(
            legacy.invariant_loop_observation_v1().is_none()
                && legacy.invariant_break_observation_v1().is_none()
        );
        let Some(observed) = legacy.invariant_mixed_join_observation_v1() else {
            assert!(!admitted);
            continue;
        };
        assert!(admitted && observed.is_consistent());
        assert!(legacy.invariant_split_continue_observation_v1().is_none());
        assert!(legacy.invariant_normal_join_observation_v1().is_none());
        assert!(legacy.invariant_split_exit_observation_v1().is_none());
        assert_eq!(
            (observed.statement(), observed.condition(), observed.body()),
            (statement, condition, body)
        );
        let (control, branch_condition, left, right) = ast
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
        let control = UnitExpressionId::new(source, control);
        let branch_condition = UnitExpressionId::new(source, branch_condition);
        assert_eq!(
            (observed.control(), observed.branch_condition()),
            (control, branch_condition)
        );
        let held = names.names().source_units()[source.index()]
            .resolution()
            .symbols()
            .iter()
            .find(|s| s.name() == "held")
            .unwrap();
        assert_eq!(observed.entry().symbol().source_unit(), source);
        assert_eq!(observed.entry().symbol().symbol(), held.id());
        assert_eq!(observed.entry(), observed.split_boundary());
        assert_eq!(observed.entry(), observed.exit());
        assert_eq!(observed.entry(), observed.join_boundary());
        assert_eq!(observed.entry(), observed.backedge());
        let literal = ast
            .expressions()
            .iter()
            .find_map(|(id, node)| {
                (sources.slice(node.span()).unwrap() == "\"outer\"")
                    .then_some(UnitExpressionId::new(source, id))
            })
            .unwrap();
        assert_eq!(observed.input_producers().definitions().len(), 1);
        assert_eq!(
            observed
                .input_producers()
                .definition(observed.entry().owner())
                .unwrap()
                .origin(),
            UnitOwnerOrigin::String(literal)
        );
        let mut edges = vec![Edge::EnterBody(condition, body)];
        let mut normals = 0;
        let mut jumps = 0;
        let mut continues = 0;
        for (branch, arm_body) in [left, right].into_iter().enumerate() {
            let Statement::ControlBody { elements } =
                ast.statements().get(arm_body).unwrap().payload()
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
        assert!(graph.is_consistent() && !graph.producers().definitions().is_empty());
        assert!(
            graph
                .producers()
                .definition(observed.entry().owner())
                .is_none()
        );
        for definition in graph.producers().definitions() {
            let UnitOwnerOrigin::String(expression) = definition.origin() else {
                panic!("actual String producer")
            };
            assert_eq!(expression.source_unit(), source);
            let literal = ast.expressions().get(expression.expression()).unwrap();
            let body_span = ast.statements().get(body.statement()).unwrap().span();
            assert!(matches!(literal.payload(), Expression::String { .. }));
            assert_eq!(literal.span().source_id(), body_span.source_id());
            assert!(
                body_span.start() <= literal.span().start()
                    && literal.span().end() <= body_span.end()
            );
            assert!(graph.prefix().iter().any(|event| matches!(event, Event::Create(id, owner) if *id == expression && *owner == definition.id())));
            assert!(
                observed
                    .input_producers()
                    .definition(definition.id())
                    .is_none()
            );
            let drops = graph
                .prefix()
                .iter()
                .filter_map(|e| match e {
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
                    owned
                        .drops()
                        .iter()
                        .any(|d| d.point() == point && d.target() == UnitDropTarget::Named(symbol))
                );
            }
        }
        if rooted {
            let foreign = owned.owner_observations()[1 - i]
                .invariant_mixed_join_observation_v1()
                .unwrap();
            for arm in foreign.arms() {
                if let Arm::Jump(jump) = arm {
                    assert!(
                        observed
                            .input_producers()
                            .definition(jump.after_jump().owner())
                            .is_none()
                    );
                }
                assert!(
                    observed
                        .input_producers()
                        .definition(arm.completion().owner())
                        .is_none()
                );
            }
            assert!(
                graph
                    .producers()
                    .definition(foreign.body_transport().producers().definitions()[0].id())
                    .is_none()
            );
        }
    }
}

#[test]
fn mixed_join_destinations_match_both_public_unit_inputs() {
    for rooted in [false, true] {
        for (left, right) in [
            ("", "continue"),
            ("continue", ""),
            ("", "break"),
            ("break", ""),
        ] {
            let tail = format!("if((other)) {{ {left} }} else {{ {right} }}");
            for prefix in [
                r#"val local = "inner""#,
                r#"val local = "inner"; val alias = (local)"#,
                r#"var root = "old"; val incoming = "next"; val previous = replace(&root, incoming)"#,
                r#"var left = "left"; var right = "right"; val previous = replace(&left, replace(&right, "next"))"#,
            ] {
                check_split_inputs(
                    rooted,
                    &format!("{prefix}; {tail}"),
                    "val retired = held",
                    true,
                );
            }
        }
        for wrong in [
            "if(other) {} else {}",
            "if(other) { continue } else { continue }",
            "if(other) { continue } else { break }",
            "if(other) { break } else { continue }",
            "if(other) { break } else { break }",
            "if(flag) {} else { continue }",
            "if(other) { (continue) } else {}",
            r#"if(other) { val extra = "extra" } else { break }"#,
            "if(other) { continue }",
        ] {
            check_split_inputs(
                rooted,
                &format!(r#"val local = "inner"; {wrong}"#),
                "val retired = held",
                false,
            );
        }
        let tail = "if(other) {} else { continue }";
        check_split_inputs(
            rooted,
            &format!(r#"val local = held + "inner"; {tail}"#),
            "val retired = held",
            false,
        );
        check_split_inputs(
            rooted,
            &format!(r#"val local = "inner"; {tail}"#),
            "val retired: String? = held",
            false,
        );
    }
}
