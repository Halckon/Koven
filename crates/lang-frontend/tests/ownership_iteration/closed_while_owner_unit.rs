//! Public two-input contract for real While body collection; no dynamic replay.
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitDropPoint, UnitOwnerLoopEdgeV1 as Edge, UnitOwnerObservationStatus,
        UnitOwnerValueEventV1 as Event, check_compilation_unit_ownership,
    },
    source::SourceMap,
    type_checking::{
        UnitExpressionId, check_compilation_unit_types, complete_compilation_unit_types,
        standard_environments,
    },
};

#[test]
fn closed_while_owner_template_has_the_same_contract_in_both_unit_inputs() {
    for rooted in [false, true] {
        for suffix in ["", "; return", "; break", "; continue"] {
            let text = format!(
                "fun run(flag: Boolean) {{ while ((flag)) {{ var root = \"old\"; val next = \"next\"; val previous = replace(&root, next); val delivered = (previous){suffix} }} }}"
            );
            let mut sources = SourceMap::new();
            let source = sources.add_source("loop.ko", &text).unwrap();
            let parsed = super::parser_test_assertions::parse_file_twice(
                &sources,
                source,
                "closed While owner",
            );
            assert!(parsed.diagnostics().is_empty());
            let inputs = [if rooted {
                SourceUnitInput::new("root", "loop.ko", source, &parsed)
            } else {
                SourceUnitInput::memory_document(source, &parsed)
            }];
            let (environment, types) = standard_environments();
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            assert!(typed.diagnostics().is_empty());
            let complete =
                complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
            assert!(complete.is_same_analysis(&typed));
            let typed = typed.validate().unwrap();
            let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap();
            assert!(owned.is_compatible_with(&typed));
            assert!(owned.clone().validate().is_ok(), "rooted={rooted}: {text}");
            let [legacy] = owned.owner_observations() else {
                panic!("one real callable")
            };
            assert!(matches!(
                legacy.status(),
                UnitOwnerObservationStatus::Unsupported { .. }
            ));
            assert!(legacy.events().is_empty() && legacy.definitions().is_empty());
            assert!(legacy.value_observation_v1().is_none());
            if suffix != "; return" {
                let observation = legacy
                    .loop_observation_v1()
                    .expect("actual closed body completion");
                assert!(observation.is_consistent());
                assert!(matches!(observation.edges()[0], Edge::EnterBody(..)));
                assert!(matches!(
                    observation.edges()[2],
                    Edge::ConditionFalseExit(..)
                ));
                let expected = if suffix.is_empty() {
                    Edge::Backedge(
                        UnitDropPoint::AfterStatement(observation.body()),
                        observation.condition(),
                    )
                } else {
                    let jump = parsed
                        .ast()
                        .expressions()
                        .iter()
                        .find_map(|(id, node)| {
                            (sources.slice(node.span()).unwrap() == suffix.trim_start_matches("; "))
                                .then_some(id)
                        })
                        .expect("actual terminal jump");
                    let point = UnitDropPoint::ControlTransfer(UnitExpressionId::new(
                        observation.condition().source_unit(),
                        jump,
                    ));
                    if suffix == "; continue" {
                        Edge::ContinueBackedge(point, observation.condition())
                    } else {
                        Edge::BreakExit(point, UnitDropPoint::LoopExit(observation.statement()))
                    }
                };
                assert_eq!(observation.edges()[1], expected);
                let graph = observation.body_transport();
                assert_eq!(graph.producers().definitions().len(), 2);
                assert_eq!(
                    graph
                        .prefix()
                        .iter()
                        .filter(|e| matches!(e, Event::Replace { .. }))
                        .count(),
                    1
                );
                assert_eq!(
                    graph
                        .prefix()
                        .iter()
                        .filter(|e| matches!(e, Event::Drop(..)))
                        .count(),
                    2
                );
                assert_eq!(owned.drops().len(), 2);
            } else {
                assert!(
                    legacy.loop_observation_v1().is_none(),
                    "Return is not a loop completion"
                );
            }
        }
    }
}

#[test]
fn conditional_while_completions_match_both_unit_inputs() {
    use lang_frontend::{
        parser::{Expression, Statement},
        type_checking::UnitStatementId,
    };
    for rooted in [false, true] {
        for (left, right, admitted) in [
            ("continue", "break", true),
            ("break", "continue", true),
            ("continue", "continue", true),
            ("break", "break", true),
            ("return", "continue", false),
        ] {
            let text = format!(
                r#"fun run(flag: Boolean, exit: Boolean) {{ while ((flag)) {{ var root = "old"; val next = "next"; val previous = replace(&root, next); if ((exit)) {{ {left} }} else {{ {right} }} }} }}"#
            );
            let mut sources = SourceMap::new();
            let source = sources.add_source("conditional.ko", &text).unwrap();
            let parsed = super::parser_test_assertions::parse_file_twice(
                &sources,
                source,
                "conditional While completion",
            );
            assert!(parsed.diagnostics().is_empty());
            let inputs = [if rooted {
                SourceUnitInput::new("root", "conditional.ko", source, &parsed)
            } else {
                SourceUnitInput::memory_document(source, &parsed)
            }];
            let (environment, types) = standard_environments();
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            assert!(typed.diagnostics().is_empty());
            let complete =
                complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
            assert!(complete.is_same_analysis(&typed));
            let typed = typed.validate().unwrap();
            let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap();
            assert!(owned.is_compatible_with(&typed));
            assert!(owned.clone().validate().is_ok(), "rooted={rooted}: {text}");
            let [legacy] = owned.owner_observations() else {
                panic!("one actual callable");
            };
            assert!(matches!(
                legacy.status(),
                UnitOwnerObservationStatus::Unsupported { .. }
            ));
            assert!(legacy.events().is_empty() && legacy.definitions().is_empty());
            assert!(legacy.value_observation_v1().is_none());
            if !admitted {
                assert!(legacy.loop_observation_v1().is_none());
                continue;
            }
            let observation = legacy.loop_observation_v1().expect("actual terminal If");
            assert!(observation.is_consistent());
            let source_unit = index.source_units()[0].id();
            assert_eq!(index.source_units()[0].source_id(), source);
            let (statement, loop_condition, loop_body) = parsed
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
                observation.statement(),
                UnitStatementId::new(source_unit, statement)
            );
            assert_eq!(
                observation.condition(),
                UnitExpressionId::new(source_unit, loop_condition)
            );
            assert_eq!(
                observation.body(),
                UnitStatementId::new(source_unit, loop_body)
            );
            let (control, condition, branches) = parsed
                .ast()
                .expressions()
                .iter()
                .find_map(|(id, node)| match node.payload() {
                    Expression::If {
                        condition,
                        then_branch,
                        else_branch: Some(else_branch),
                        ..
                    } => Some((id, *condition, [*then_branch, *else_branch])),
                    _ => None,
                })
                .unwrap();
            let mut expected = vec![Edge::EnterBody(observation.condition(), observation.body())];
            for (branch, (body, kind)) in branches.into_iter().zip([left, right]).enumerate() {
                let Statement::ControlBody { elements } =
                    parsed.ast().statements().get(body).unwrap().payload()
                else {
                    panic!("actual arm");
                };
                let [statement] = elements.as_slice() else {
                    panic!("one jump");
                };
                let Statement::Expression { expression } =
                    parsed.ast().statements().get(*statement).unwrap().payload()
                else {
                    panic!("jump expression");
                };
                expected.push(Edge::IfBranch {
                    control: UnitExpressionId::new(source_unit, control),
                    condition: UnitExpressionId::new(source_unit, condition),
                    branch,
                    body: UnitStatementId::new(source_unit, body),
                });
                let point =
                    UnitDropPoint::ControlTransfer(UnitExpressionId::new(source_unit, *expression));
                expected.push(if kind == "continue" {
                    Edge::ContinueBackedge(point, observation.condition())
                } else {
                    Edge::BreakExit(point, UnitDropPoint::LoopExit(observation.statement()))
                });
            }
            expected.push(Edge::ConditionFalseExit(
                observation.condition(),
                UnitDropPoint::LoopExit(observation.statement()),
            ));
            assert_eq!(observation.edges(), expected);
            assert_eq!(
                observation.body_transport().producers().definitions().len(),
                2
            );
            assert_eq!(
                observation
                    .body_transport()
                    .prefix()
                    .iter()
                    .filter(|e| matches!(e, Event::Replace { .. }))
                    .count(),
                1
            );
            assert_eq!(owned.drops().len(), 2);
            assert!(owned.drops().iter().all(|drop| !matches!(
                drop.point(),
                UnitDropPoint::ControlTransfer(_)
                    | UnitDropPoint::BranchEntry { .. }
                    | UnitDropPoint::BranchExit { .. }
            )));
        }
    }
}

#[test]
fn empty_while_arms_share_one_actual_body_backedge_in_both_inputs() {
    use lang_frontend::{
        parser::{Expression, Statement},
        type_checking::UnitStatementId,
    };
    for rooted in [false, true] {
        for (left, right, admitted) in [
            ("", "break", true),
            ("break", "", true),
            ("", "continue", true),
            ("continue", "", true),
            ("", "", true),
            (r#"val branch = "branch""#, "break", false),
            ("", "(continue)", false),
        ] {
            let text = format!(
                r#"fun run(flag: Boolean, exit: Boolean) {{ while ((flag)) {{ var root = "old"; val next = "next"; val previous = replace(&root, next); if ((exit)) {{ {left} }} else {{ {right} }} }} }}"#
            );
            let mut sources = SourceMap::new();
            let source = sources.add_source("conditional.ko", &text).unwrap();
            let parsed = super::parser_test_assertions::parse_file_twice(
                &sources,
                source,
                "empty While arm completion",
            );
            assert!(parsed.diagnostics().is_empty());
            let inputs = [if rooted {
                SourceUnitInput::new("root", "conditional.ko", source, &parsed)
            } else {
                SourceUnitInput::memory_document(source, &parsed)
            }];
            let (environment, types) = standard_environments();
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            assert!(typed.diagnostics().is_empty());
            let complete =
                complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
            assert!(complete.is_same_analysis(&typed));
            let typed = typed.validate().unwrap();
            let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap();
            assert!(owned.is_compatible_with(&typed));
            assert!(owned.clone().validate().is_ok(), "rooted={rooted}: {text}");
            let [legacy] = owned.owner_observations() else {
                panic!("one actual callable");
            };
            assert!(matches!(
                legacy.status(),
                UnitOwnerObservationStatus::Unsupported { .. }
            ));
            assert!(legacy.events().is_empty() && legacy.definitions().is_empty());
            assert!(legacy.value_observation_v1().is_none());
            if !admitted {
                assert!(legacy.loop_observation_v1().is_none());
                continue;
            }
            let observation = legacy
                .loop_observation_v1()
                .expect("actual normal arm completion");
            assert!(observation.is_consistent());
            let source_unit = index.source_units()[0].id();
            assert_eq!(index.source_units()[0].source_id(), source);
            let (statement, loop_condition, loop_body) = parsed
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
                observation.statement(),
                UnitStatementId::new(source_unit, statement)
            );
            assert_eq!(
                observation.condition(),
                UnitExpressionId::new(source_unit, loop_condition)
            );
            assert_eq!(
                observation.body(),
                UnitStatementId::new(source_unit, loop_body)
            );
            let (control, condition, branches) = parsed
                .ast()
                .expressions()
                .iter()
                .find_map(|(id, node)| match node.payload() {
                    Expression::If {
                        condition,
                        then_branch,
                        else_branch: Some(else_branch),
                        ..
                    } => Some((id, *condition, [*then_branch, *else_branch])),
                    _ => None,
                })
                .unwrap();
            let mut expected = vec![Edge::EnterBody(observation.condition(), observation.body())];
            for (branch, (body, kind)) in branches.into_iter().zip([left, right]).enumerate() {
                let Statement::ControlBody { elements } =
                    parsed.ast().statements().get(body).unwrap().payload()
                else {
                    panic!("actual arm");
                };
                expected.push(Edge::IfBranch {
                    control: UnitExpressionId::new(source_unit, control),
                    condition: UnitExpressionId::new(source_unit, condition),
                    branch,
                    body: UnitStatementId::new(source_unit, body),
                });
                if kind.is_empty() {
                    assert!(elements.is_empty());
                    let no_scope_drop =
                        UnitDropPoint::AfterStatement(UnitStatementId::new(source_unit, body));
                    assert!(
                        owned
                            .drops()
                            .iter()
                            .all(|drop| drop.point() != no_scope_drop)
                    );
                    assert!(observation.body_transport().prefix().iter().all(|event| !matches!(event, Event::Drop(point, _, _) if *point == no_scope_drop)));
                    expected.push(Edge::IfFallthrough(
                        UnitStatementId::new(source_unit, body),
                        UnitExpressionId::new(source_unit, control),
                    ));
                    continue;
                }
                let [statement] = elements.as_slice() else {
                    panic!("one jump");
                };
                let Statement::Expression { expression } =
                    parsed.ast().statements().get(*statement).unwrap().payload()
                else {
                    panic!("jump expression");
                };

                let point =
                    UnitDropPoint::ControlTransfer(UnitExpressionId::new(source_unit, *expression));
                expected.push(if kind == "continue" {
                    Edge::ContinueBackedge(point, observation.condition())
                } else {
                    Edge::BreakExit(point, UnitDropPoint::LoopExit(observation.statement()))
                });
            }
            expected.push(Edge::Backedge(
                UnitDropPoint::AfterStatement(observation.body()),
                observation.condition(),
            ));
            expected.push(Edge::ConditionFalseExit(
                observation.condition(),
                UnitDropPoint::LoopExit(observation.statement()),
            ));
            assert_eq!(observation.edges(), expected);
            assert_eq!(observation.edges().len(), 7);
            assert_eq!(
                observation
                    .edges()
                    .iter()
                    .filter(|edge| matches!(edge, Edge::Backedge(..)))
                    .count(),
                1
            );
            assert_eq!(
                observation
                    .edges()
                    .iter()
                    .filter(|edge| matches!(edge, Edge::IfFallthrough(..)))
                    .count(),
                [left, right].iter().filter(|kind| kind.is_empty()).count()
            );
            assert_eq!(
                observation.body_transport().producers().definitions().len(),
                2
            );
            assert_eq!(
                observation
                    .body_transport()
                    .prefix()
                    .iter()
                    .filter(|e| matches!(e, Event::Replace { .. }))
                    .count(),
                1
            );
            assert_eq!(owned.drops().len(), 2);
            assert!(owned.drops().iter().all(|drop| !matches!(
                drop.point(),
                UnitDropPoint::ControlTransfer(_)
                    | UnitDropPoint::BranchEntry { .. }
                    | UnitDropPoint::BranchExit { .. }
            )));
        }
    }
}
