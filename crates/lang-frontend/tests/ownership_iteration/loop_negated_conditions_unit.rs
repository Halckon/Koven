//! Public inputs preserve full negated condition identities and original branch order.
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitDropPoint as Point, UnitOwnerLoopEdgeV1 as Edge, UnitOwnerObservation,
        UnitOwnerValueObservationV1, check_compilation_unit_ownership,
    },
    parser::{Expression, Statement},
    source::SourceMap,
    type_checking::{
        UnitExpressionId, UnitStatementId, check_compilation_unit_types,
        complete_compilation_unit_types, standard_environments,
    },
};
type Evidence<'a> = (
    UnitStatementId,
    UnitExpressionId,
    UnitStatementId,
    &'a [Edge],
    &'a UnitOwnerValueObservationV1,
);
fn evidence(observation: &UnitOwnerObservation) -> Option<Evidence<'_>> {
    macro_rules! view {
        ($value:expr) => {{
            let value = $value;
            assert!(value.is_consistent());
            (
                value.statement(),
                value.condition(),
                value.body(),
                value.edges(),
                value.body_transport(),
            )
        }};
    }
    macro_rules! invariant {
        ($getter:ident) => {
            if let Some(value) = observation.$getter() {
                assert_eq!(value.entry(), value.exit());
                assert!(
                    value
                        .input_producers()
                        .definition(value.entry().owner())
                        .is_some()
                );
                assert!(
                    value
                        .body_transport()
                        .producers()
                        .definition(value.entry().owner())
                        .is_none()
                );
                return Some(view!(value));
            }
        };
    }
    if let Some(value) = observation.loop_observation_v1() {
        return Some(view!(value));
    }
    invariant!(invariant_loop_observation_v1);
    invariant!(invariant_break_observation_v1);
    invariant!(invariant_split_continue_observation_v1);
    invariant!(invariant_split_exit_observation_v1);
    invariant!(invariant_normal_join_observation_v1);
    invariant!(invariant_mixed_join_observation_v1);
    None
}
fn check(text: &str, rooted: bool, admitted: bool) {
    let second = text.replace("run", "second");
    let mut sources = SourceMap::new();
    let ids = [
        sources.add_source("a.ko", text).unwrap(),
        sources.add_source("b.ko", &second).unwrap(),
    ];
    let parsed = ids.map(|id| {
        super::parser_test_assertions::parse_file_twice(&sources, id, "negated loop condition")
    });
    assert!(parsed.iter().all(|p| p.diagnostics().is_empty()));
    let count = if rooted { 2 } else { 1 };
    let inputs = parsed
        .iter()
        .take(count)
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
    assert!(typed.diagnostics().is_empty(), "{text}");
    let completed =
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
    assert!(completed.is_same_analysis(&typed));
    let typed = typed.validate().unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    assert!(owned.is_compatible_with(&typed));
    assert!(owned.clone().validate().is_ok(), "{text}");
    assert_eq!(owned.owner_observations().len(), count);
    for (i, observation) in owned.owner_observations().iter().enumerate() {
        assert!(observation.value_observation_v1().is_none());
        assert!(observation.events().is_empty() && observation.definitions().is_empty());
        let actual = evidence(observation);
        assert_eq!(actual.is_some(), admitted, "rooted={rooted}: {text}");
        let Some((statement, condition, body, actual_edges, graph)) = actual else {
            continue;
        };
        let source = index.source_units()[i].id();
        assert_eq!(source.index(), i);
        assert_eq!(index.source_units()[i].source_id(), ids[i]);
        let ast = parsed[i].ast();
        let (expected_statement, expected_condition, expected_body) = ast
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
            (statement, condition, body),
            (expected_statement, expected_condition, expected_body)
        );
        assert!(matches!(
            ast.expressions()
                .get(condition.expression())
                .unwrap()
                .payload(),
            Expression::Prefix { .. } | Expression::Group { .. }
        ));
        let mut edges = vec![Edge::EnterBody(condition, body)];
        if let Some((control, branch_condition, left, right)) =
            ast.expressions().iter().find_map(|(id, node)| {
                if let Expression::If {
                    condition,
                    then_branch,
                    else_branch: Some(right),
                    ..
                } = node.payload()
                {
                    Some((
                        UnitExpressionId::new(source, id),
                        UnitExpressionId::new(source, *condition),
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
                let Statement::ControlBody { elements } =
                    ast.statements().get(arm).unwrap().payload()
                else {
                    panic!("arm")
                };
                if elements.is_empty() {
                    normal = true;
                    edges.push(Edge::IfFallthrough(arm_id, control));
                } else {
                    let [tail] = elements.as_slice() else {
                        panic!("one jump")
                    };
                    let Statement::Expression { expression } =
                        ast.statements().get(*tail).unwrap().payload()
                    else {
                        panic!("jump")
                    };
                    let jump = UnitExpressionId::new(source, *expression);
                    edges.push(
                        match ast.expressions().get(*expression).unwrap().payload() {
                            Expression::Break { .. } => Edge::BreakExit(
                                Point::ControlTransfer(jump),
                                Point::LoopExit(statement),
                            ),
                            Expression::Continue { .. } => {
                                Edge::ContinueBackedge(Point::ControlTransfer(jump), condition)
                            }
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
        assert_eq!(actual_edges, edges);
        assert!(graph.is_consistent());
        assert_eq!(graph.producers().definitions().len(), 2);
        for definition in graph.producers().definitions() {
            let lang_frontend::ownership_checking::UnitOwnerOrigin::String(id) =
                definition.origin()
            else {
                panic!("body String")
            };
            assert_eq!(id.source_unit(), source);
            if rooted {
                let foreign = evidence(&owned.owner_observations()[1 - i]).unwrap().4;
                assert!(foreign.producers().definition(definition.id()).is_none());
            }
        }
    }
}
#[test]
fn negated_conditions_match_both_public_unit_inputs() {
    let mut cases = Vec::new();
    for invariant in [false, true] {
        let mut tails = vec![String::new(), "continue".into(), "break".into()];
        for left in ["", "continue", "break"] {
            for right in ["", "continue", "break"] {
                tails.push(format!("if (!(!(!other))) {{ {left} }} else {{ {right} }}"));
            }
        }
        for tail in tails {
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
            cases.push((format!(r#"fun run({parameters}) {{ {prefix}while (!(!((flag)))) {{ var root = "old"; val incoming = "next"; val previous = replace(&root, incoming); {tail} }}{suffix} }}"#),true));
        }
    }
    let base = r#"fun run(flag: Boolean, other: Boolean) { val held = "outer"; while (!flag) { val local = "inner"; if (!other) { break } else { continue } }; val retired = held }"#;
    for text in [
        base.replace("!flag", "!true"),
        base.replace("!other", "!(flag)"),
        base.replace("!other", "!(other && other)"),
        base.replace("!flag", "!(flag == flag)"),
        base.replace("other: Boolean", "borrow other: Boolean"),
        base.replace("val held", "var held"),
    ] {
        cases.push((text, false));
    }
    assert_eq!(cases.len(), 30);
    for (text, admitted) in cases {
        for rooted in [false, true] {
            check(&text, rooted, admitted);
        }
    }
}
