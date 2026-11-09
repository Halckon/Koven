//! Public source-qualified contract for two nonempty Continue arms, not a phi.
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitOwnerLoopEdgeV1 as Edge, UnitOwnerObservationStatus,
        UnitOwnerOrigin, UnitOwnerValueEventV1 as Event, UnitOwnerValueV1 as Value,
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
        let Some(observed) = legacy.invariant_split_continue_observation_v1() else {
            assert!(!admitted);
            continue;
        };
        assert!(admitted && observed.is_consistent());
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
        let mut jumps = Vec::new();
        for (branch, arm_body) in [left, right].into_iter().enumerate() {
            let Statement::ControlBody { elements } =
                ast.statements().get(arm_body).unwrap().payload()
            else {
                panic!("actual arm")
            };
            let [tail] = elements.as_slice() else {
                panic!("actual single jump")
            };
            let Statement::Expression { expression } =
                ast.statements().get(*tail).unwrap().payload()
            else {
                panic!("actual tail")
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
            for binding in [arm.entry(), arm.after_continue(), arm.completion()] {
                assert_eq!(binding, observed.entry());
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
                    .all(|d| d.point() != UnitDropPoint::ControlTransfer(jump))
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
        assert!(graph.is_consistent() && !graph.producers().definitions().is_empty());
        assert!(
            graph
                .producers()
                .definition(observed.entry().owner())
                .is_none()
        );
        for definition in graph.producers().definitions() {
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
                    !jumps
                        .iter()
                        .any(|jump| point == UnitDropPoint::ControlTransfer(*jump))
                );
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
                .invariant_split_continue_observation_v1()
                .unwrap();
            for arm in foreign.arms() {
                assert!(
                    observed
                        .input_producers()
                        .definition(arm.after_continue().owner())
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
fn split_continue_arms_match_actual_sources_in_both_unit_inputs() {
    let tail = "if((other)) { continue } else { continue }";
    for rooted in [false, true] {
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
        for wrong in [
            "if(other) { continue } else { break }",
            "if(other) { break } else { continue }",
            "if(other) { break } else { break }",
            "if(other) {} else { continue }",
            "if(other) {} else {}",
            r#"if(other) { val extra = "extra"; continue } else { continue }"#,
        ] {
            check_split_inputs(
                rooted,
                &format!(r#"val local = "inner"; {wrong}"#),
                "val retired = held",
                false,
            );
        }
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
