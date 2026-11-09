//! Original control-exit oracles through both real Unit inputs.
//! Named-owner cases supplement their Single witnesses until owner identity can migrate.
use lang_frontend::{
    name_resolution::{
        SourceUnitId, SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{CompilationUnitOwnership, check_compilation_unit_ownership},
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{
        UnitExpressionId, check_compilation_unit_types, complete_compilation_unit_types,
        standard_environments,
    },
};

fn with_units(
    text: &str,
    mut check: impl FnMut(&SourceMap, &ParsedFile, SourceUnitId, &CompilationUnitOwnership),
) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("iteration.ko", text).unwrap();
    let parsed =
        super::parser_test_assertions::parse_file_twice(&sources, source, "Unit control exits");
    assert!(
        parsed.diagnostics().is_empty(),
        "{text}: {:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::memory_document(source, &parsed),
        SourceUnitInput::new("root", "iteration.ko", source, &parsed),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let [indexed] = index.source_units() else {
            panic!("one source")
        };
        assert_eq!(indexed.source_id(), source);
        let unit = indexed.id();
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(
            names.diagnostics().is_empty(),
            "{text}: {:?}",
            names.diagnostics()
        );
        let names = names.validate().unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(
            typed.diagnostics().is_empty(),
            "{text}: {:?}",
            typed.diagnostics()
        );
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(complete.is_same_analysis(&typed));
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(
            owned.diagnostics().is_empty(),
            "{text}: {:?}",
            owned.diagnostics()
        );
        assert!(
            owned.deferred().is_empty(),
            "{text}: {:?}",
            owned.deferred()
        );
        assert!(owned.is_compatible_with(&typed));
        assert!(owned.clone().validate().is_ok());
        assert_eq!(
            owned,
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
        );
        check(&sources, &parsed, unit, &owned);
    }
}

#[test]
fn temporary_sources_follow_control_transfer_boundaries() {
    use lang_frontend::ownership_checking::{
        UnitDropPoint as DropPoint, UnitDropTarget as DropTarget,
    };
    use lang_frontend::parser::{Expression, Statement};
    for (jump, releases) in [("break", true), ("continue", false), ("return", true)] {
        with_units(
            &format!("fun run() {{ for (_ in listOf(1)) {{ {jump} }} }}"),
            |_sources, parsed, unit, owned| {
                assert!(
                    owned.drops().iter().all(|fact| !matches!(
                        fact.target(),
                        lang_frontend::ownership_checking::UnitDropTarget::Named(_)
                    )),
                    "this migrated source has no named-owner identity obligation"
                );
                assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
                let source = parsed
                    .ast()
                    .statements()
                    .iter()
                    .find_map(|(_, node)| {
                        if let Statement::For { source, .. } = node.payload() {
                            Some(*source)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                let transfer = parsed
                    .ast()
                    .expressions()
                    .iter()
                    .find_map(|(id, node)| {
                        matches!(
                            node.payload(),
                            Expression::Break { .. }
                                | Expression::Continue { .. }
                                | Expression::Return { .. }
                        )
                        .then_some(id)
                    })
                    .unwrap();
                assert_eq!(
                    owned
                        .drops()
                        .iter()
                        .filter(|fact| {
                            fact.target()
                                == DropTarget::Temporary(UnitExpressionId::new(unit, source))
                                && fact.point()
                                    == DropPoint::ControlTransfer(UnitExpressionId::new(
                                        unit, transfer,
                                    ))
                        })
                        .count(),
                    usize::from(releases),
                    "{jump}"
                );
            },
        );
    }
}

#[test]
fn nested_jumps_release_only_the_sources_they_leave() {
    use lang_frontend::ownership_checking::{
        UnitDropPoint as DropPoint, UnitDropTarget as DropTarget,
    };
    use lang_frontend::parser::Expression;
    for (jump, expected) in [
        ("break", vec!["listOf(2)"]),
        ("continue", vec![]),
        ("return", vec!["listOf(2)", "listOf(1)"]),
    ] {
        with_units(
            &format!(
                "fun run() {{ for (_ in listOf(1)) {{ for (_ in listOf(2)) {{ {jump} }} }} }}"
            ),
            |sources, parsed, unit, owned| {
                assert!(
                    owned.drops().iter().all(|fact| !matches!(
                        fact.target(),
                        lang_frontend::ownership_checking::UnitDropTarget::Named(_)
                    )),
                    "this migrated source has no named-owner identity obligation"
                );
                assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
                let transfer = parsed
                    .ast()
                    .expressions()
                    .iter()
                    .find_map(|(id, node)| {
                        matches!(
                            node.payload(),
                            Expression::Break { .. }
                                | Expression::Continue { .. }
                                | Expression::Return { .. }
                        )
                        .then_some(id)
                    })
                    .unwrap();
                let actual = owned
                    .drops()
                    .iter()
                    .filter_map(|fact| {
                        if fact.point()
                            == DropPoint::ControlTransfer(UnitExpressionId::new(unit, transfer))
                            && let DropTarget::Temporary(source) = fact.target()
                        {
                            assert_eq!(source.source_unit(), unit);
                            Some(
                                sources
                                    .slice(
                                        parsed
                                            .ast()
                                            .expressions()
                                            .get(source.expression())
                                            .unwrap()
                                            .span(),
                                    )
                                    .unwrap(),
                            )
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                assert_eq!(actual, expected, "{jump}");
            },
        );
    }
}

#[test]
fn inner_while_break_keeps_the_outer_temporary_source() {
    use lang_frontend::ownership_checking::UnitDropPoint as DropPoint;
    use lang_frontend::parser::Expression;
    with_units(
        "fun run(flag: Boolean) { for (_ in listOf(1)) { while (flag) { break } } }",
        |_sources, parsed, unit, owned| {
            assert!(
                owned.drops().iter().all(|fact| !matches!(
                    fact.target(),
                    lang_frontend::ownership_checking::UnitDropTarget::Named(_)
                )),
                "this migrated source has no named-owner identity obligation"
            );
            assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
            let jump = parsed
                .ast()
                .expressions()
                .iter()
                .find_map(|(id, node)| {
                    matches!(node.payload(), Expression::Break { .. }).then_some(id)
                })
                .unwrap();
            assert!(!owned.drops().iter().any(|fact| fact.point()
                == DropPoint::ControlTransfer(UnitExpressionId::new(unit, jump))));
        },
    );
}

#[test]
fn iteration_plans_publish_ordered_and_distinct_exit_paths() {
    use lang_frontend::ownership_checking::{
        UnitIterationCleanupAction as Action, UnitIterationExitKind as IterationExitKind,
        UnitLoanTarget as LoanTarget,
    };
    with_units(
        "fun run(flag: Boolean) { for (n in listOf(1)) { if (flag) { continue } else { break } } }",
        |_sources, _parsed, _unit, owned| {
            assert!(
                owned.drops().iter().all(|fact| !matches!(
                    fact.target(),
                    lang_frontend::ownership_checking::UnitDropTarget::Named(_)
                )),
                "this migrated source has no named-owner identity obligation"
            );
            let plan = &owned.iterations()[0];
            assert!(matches!(plan.source(), Some(LoanTarget::Temporary(_))));
            assert_eq!(plan.bindings().len(), 1);
            assert_eq!(owned.iteration(plan.descriptor().statement()), Some(plan));
            for exit in plan.exits() {
                let actions = exit.actions();
                match exit.kind() {
                    IterationExitKind::Continue(_) => assert!(matches!(
                        actions,
                        [Action::EndBinding { .. }, Action::EndElement(_)]
                    )),
                    IterationExitKind::Break(_) => assert!(matches!(
                        actions,
                        [
                            Action::EndBinding { .. },
                            Action::EndElement(_),
                            Action::FinishProvider(_),
                            Action::EndSource(_),
                            Action::Drop(_)
                        ]
                    )),
                    IterationExitKind::Exhaustion => assert!(matches!(
                        actions,
                        [
                            Action::FinishProvider(_),
                            Action::EndSource(_),
                            Action::Drop(_)
                        ]
                    )),
                    kind => panic!("unexpected exit {kind:?}"),
                }
            }
            assert_eq!(plan.exits().len(), 3);
        },
    );
}

#[test]
fn nested_return_plan_orders_each_scope_before_its_provider() {
    use lang_frontend::ownership_checking::{
        UnitIterationCleanupAction as Action, UnitIterationExitKind as IterationExitKind,
    };
    with_units(
        "class Node {}\nfun touch(n: Node) {}\nfun run(own outer: Node, flag: Boolean) { for (_ in listOf(1)) { val local = Node()\nfor (_ in listOf(2)) { if (flag) { return }\ntouch(local) } }; touch(outer) }",
        |sources, parsed, unit, owned| {
            let transfer = parsed
                .ast()
                .expressions()
                .iter()
                .find_map(|(id, node)| {
                    matches!(
                        node.payload(),
                        lang_frontend::parser::Expression::Return { .. }
                    )
                    .then_some(id)
                })
                .unwrap();
            let transfer = UnitExpressionId::new(unit, transfer);
            assert_eq!(owned.iterations().len(), 2);
            let returns = owned
                .iterations()
                .iter()
                .map(|plan| {
                    plan.exits()
                        .iter()
                        .find(|exit| matches!(exit.kind(), IterationExitKind::Return(_)))
                        .unwrap()
                })
                .collect::<Vec<_>>();
            assert_eq!(returns[0].actions(), returns[1].actions());
            for exit in &returns {
                assert_eq!(exit.kind(), IterationExitKind::Return(transfer));
                assert_eq!(
                    exit.point(),
                    lang_frontend::ownership_checking::UnitDropPoint::ControlTransfer(transfer)
                );
            }
            assert_eq!(
                owned.iteration_cleanup_at(returns[0].point()),
                Some(returns[0].actions())
            );
            let drops = returns[0]
                .actions()
                .iter()
                .filter_map(|action| match action {
                    Action::Drop(fact) => Some(sources.slice(fact.value_origin()).unwrap()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(drops, ["listOf(2)", "local", "listOf(1)", "outer"]);
        },
    );
}

#[test]
fn named_source_break_has_its_own_dead_owner_cleanup() {
    use lang_frontend::ownership_checking::{
        UnitDropTarget as DropTarget, UnitIterationCleanupAction as Action,
        UnitIterationExitKind as IterationExitKind,
    };
    with_units(
        "fun run(own xs: List<Int>) { for (_ in xs) { break } }",
        |_sources, _parsed, _unit, owned| {
            let exit = owned.iterations()[0]
                .exits()
                .iter()
                .find(|exit| matches!(exit.kind(), IterationExitKind::Break(_)))
                .unwrap();
            assert!(
                matches!(exit.actions().last(), Some(Action::Drop(fact)) if matches!(fact.target(), DropTarget::Named(_)))
            );
        },
    );
}
