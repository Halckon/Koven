use super::*;

#[test]
fn temporary_sources_follow_control_transfer_boundaries() {
    use lang_frontend::ownership_checking::{DropPoint, DropTarget};
    use lang_frontend::parser::{Expression, Statement};
    for (jump, releases) in [("break", true), ("continue", false), ("return", true)] {
        let (_, parsed, owned) = checked(&format!(
            "fun run() {{ for (_ in listOf(1)) {{ {jump} }} }}"
        ));
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
                    fact.target() == DropTarget::Temporary(source)
                        && fact.point() == DropPoint::ControlTransfer(transfer)
                })
                .count(),
            usize::from(releases),
            "{jump}"
        );
    }
}

#[test]
fn conditional_temporary_source_survives_until_each_provider_exit() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, DropPoint, DropTarget, LoanTarget};
    use lang_frontend::parser::{Expression, Statement};
    for text in [
        "fun run(flag: Boolean) { for (_ in if (flag) listOf(1) else listOf(2)) { break } }",
        "fun run(flag: Boolean) { for (_ in when (flag) { true -> listOf(1)\nelse -> listOf(2) }) { break } }",
    ] {
        let (sources, parsed, owned) = checked(text);
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let (statement, source) = parsed
            .ast()
            .statements()
            .iter()
            .find_map(|(id, node)| match node.payload() {
                Statement::For { source, .. } => Some((id, *source)),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            owned.iteration(statement).unwrap().source(),
            &LoanTarget::Temporary(source)
        );
        let transfer = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| matches!(node.payload(), Expression::Break { .. }).then_some(id))
            .unwrap();
        let drops = owned
            .drops()
            .iter()
            .filter(|fact| fact.target() == DropTarget::Temporary(source))
            .collect::<Vec<_>>();
        assert_eq!(
            drops.len(),
            2,
            "zero-round exhaustion and break each release the source"
        );
        let owner = drops[0]
            .owner()
            .expect("the selected source has an owner identity");
        assert_eq!(drops[1].owner(), Some(owner));
        assert!(matches!(
            owned.cleanup_conditions().owner_value(owner),
            Some(CleanupOwnerValue::Expression { expression, .. }) if *expression == source
        ));
        assert!(
            drops
                .iter()
                .any(|fact| fact.point() == DropPoint::LoopExit(statement))
        );
        assert!(
            drops
                .iter()
                .any(|fact| fact.point() == DropPoint::ControlTransfer(transfer))
        );
        assert!(
            owned.drops().iter().all(|fact| match fact.target() {
                DropTarget::Temporary(value) =>
                    sources
                        .slice(parsed.ast().expressions().get(value).unwrap().span())
                        .unwrap()
                        != "listOf(1)"
                        && sources
                            .slice(parsed.ast().expressions().get(value).unwrap().span())
                            .unwrap()
                            != "listOf(2)",
                _ => true,
            }),
            "selected branch temporary transfers into the provider source"
        );
    }
}

#[test]
fn conditional_temporary_source_replays_each_executed_exit_once() {
    use lang_frontend::ownership_checking::{
        DropTarget, IterationCleanupAction as Action, IterationExitKind, IterationExitPlan,
        LoanTarget,
    };
    use lang_frontend::parser::Statement;

    fn replay(
        exit: &IterationExitPlan,
        source: lang_frontend::ast::ExpressionId,
        element_live: &mut bool,
        provider_live: &mut bool,
        source_loan_live: &mut bool,
        source_owned: &mut bool,
        drops: &mut usize,
    ) {
        for action in exit.actions() {
            match action {
                Action::EndElement(_) => {
                    assert!(*element_live, "element ends only after NextPlace");
                    *element_live = false;
                }
                Action::FinishProvider(_) => {
                    assert!(!*element_live && *provider_live);
                    *provider_live = false;
                }
                Action::EndSource(_) => {
                    assert!(!*provider_live && *source_loan_live);
                    *source_loan_live = false;
                }
                Action::Drop(fact) if fact.target() == DropTarget::Temporary(source) => {
                    assert!(!*source_loan_live && *source_owned);
                    assert!(fact.condition().is_none());
                    *source_owned = false;
                    *drops += 1;
                }
                _ => {}
            }
        }
    }

    for source_text in [
        "if (flag) listOf<Int>() else listOf(1, 2)",
        "when (flag) { true -> listOf<Int>()\nelse -> listOf(1, 2) }",
    ] {
        for (body, rounds, end) in [
            ("", 0, "exhaustion"),
            ("", 2, "exhaustion"),
            ("continue", 2, "exhaustion"),
            ("break", 1, "break"),
            ("return", 1, "return"),
        ] {
            let (_, parsed, owned) = checked(&format!(
                "fun run(flag: Boolean) {{ for (_ in {source_text}) {{ {body} }} }}"
            ));
            assert!(
                owned.diagnostics().is_empty(),
                "{source_text}, {body}: {:?}",
                owned.diagnostics()
            );
            let (statement, source) = parsed
                .ast()
                .statements()
                .iter()
                .find_map(|(id, node)| match node.payload() {
                    Statement::For { source, .. } => Some((id, *source)),
                    _ => None,
                })
                .unwrap();
            let plan = owned.iteration(statement).unwrap();
            assert_eq!(plan.source(), &LoanTarget::Temporary(source));
            let mut element_live = false;
            let mut provider_live = true;
            let mut source_loan_live = true;
            let mut source_owned = true;
            let mut drops = 0;
            for _ in 0..rounds {
                element_live = true;
                let kind = match body {
                    "continue" => "continue",
                    "break" => "break",
                    "return" => "return",
                    _ => "fallthrough",
                };
                let exit = plan
                    .exits()
                    .iter()
                    .find(|exit| {
                        matches!(
                            (kind, exit.kind()),
                            ("fallthrough", IterationExitKind::Fallthrough)
                                | ("continue", IterationExitKind::Continue(_))
                                | ("break", IterationExitKind::Break(_))
                                | ("return", IterationExitKind::Return(_))
                        )
                    })
                    .unwrap();
                replay(
                    exit,
                    source,
                    &mut element_live,
                    &mut provider_live,
                    &mut source_loan_live,
                    &mut source_owned,
                    &mut drops,
                );
                if matches!(
                    exit.kind(),
                    IterationExitKind::Fallthrough | IterationExitKind::Continue(_)
                ) {
                    assert!(provider_live && source_loan_live && source_owned);
                    assert_eq!(drops, 0);
                } else {
                    break;
                }
            }
            if end == "exhaustion" {
                let exit = plan
                    .exits()
                    .iter()
                    .find(|exit| exit.kind() == IterationExitKind::Exhaustion)
                    .unwrap();
                replay(
                    exit,
                    source,
                    &mut element_live,
                    &mut provider_live,
                    &mut source_loan_live,
                    &mut source_owned,
                    &mut drops,
                );
            }
            assert!(!element_live && !provider_live && !source_loan_live && !source_owned);
            assert_eq!(drops, 1, "{source_text}, {body}, {end}");
        }
    }
}

#[test]
fn nested_jumps_release_only_the_sources_they_leave() {
    use lang_frontend::ownership_checking::{DropPoint, DropTarget};
    use lang_frontend::parser::Expression;
    for (jump, expected) in [
        ("break", vec!["listOf(2)"]),
        ("continue", vec![]),
        ("return", vec!["listOf(2)", "listOf(1)"]),
    ] {
        let (sources, parsed, owned) = checked(&format!(
            "fun run() {{ for (_ in listOf(1)) {{ for (_ in listOf(2)) {{ {jump} }} }} }}"
        ));
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
                if fact.point() == DropPoint::ControlTransfer(transfer)
                    && let DropTarget::Temporary(source) = fact.target()
                {
                    Some(
                        sources
                            .slice(parsed.ast().expressions().get(source).unwrap().span())
                            .unwrap(),
                    )
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{jump}");
    }
}

#[test]
fn inner_while_break_keeps_the_outer_temporary_source() {
    use lang_frontend::ownership_checking::DropPoint;
    use lang_frontend::parser::Expression;
    let (_, parsed, owned) =
        checked("fun run(flag: Boolean) { for (_ in listOf(1)) { while (flag) { break } } }");
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let jump = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| matches!(node.payload(), Expression::Break { .. }).then_some(id))
        .unwrap();
    assert!(
        !owned
            .drops()
            .iter()
            .any(|fact| fact.point() == DropPoint::ControlTransfer(jump))
    );
}

#[test]
fn source_evaluation_exit_never_creates_a_provider_owner() {
    for condition in ["return", "error(\"stop\")"] {
        let (_, _, owned) = checked(&format!(
            "fun run() {{ for (_ in if ({condition}) listOf(1) else listOf(2)) {{}} }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.drops().is_empty(), "{condition}: {:?}", owned.drops());
    }
}

#[test]
fn when_source_return_does_not_clean_up_an_uncreated_provider() {
    use lang_frontend::ownership_checking::{DropPoint, DropTarget};
    use lang_frontend::parser::{Expression, Statement};
    let (_, parsed, owned) = checked(
        "fun run(flag: Boolean) { for (_ in when (flag) { true -> return\nelse -> listOf(2) }) {} }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let (statement, source) = parsed
        .ast()
        .statements()
        .iter()
        .find_map(|(id, node)| match node.payload() {
            Statement::For { source, .. } => Some((id, *source)),
            _ => None,
        })
        .unwrap();
    let transfer = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| matches!(node.payload(), Expression::Return { .. }).then_some(id))
        .unwrap();
    assert!(
        owned.iteration(statement).is_some(),
        "else builds a provider"
    );
    assert!(owned.drops().iter().any(|fact| {
        fact.target() == DropTarget::Temporary(source)
            && fact.point() == DropPoint::LoopExit(statement)
    }));
    assert!(
        owned
            .drops()
            .iter()
            .all(|fact| fact.point() != DropPoint::ControlTransfer(transfer)),
        "the return edge owns neither the provider nor its selected source"
    );
}

#[test]
fn iteration_plans_publish_ordered_and_distinct_exit_paths() {
    use lang_frontend::ownership_checking::{
        IterationCleanupAction as Action, IterationExitKind, LoanTarget,
    };
    let (_, _, owned) = checked(
        "fun run(flag: Boolean) { for (n in listOf(1)) { if (flag) { continue } else { break } } }",
    );
    let plan = &owned.iterations()[0];
    assert!(matches!(plan.source(), LoanTarget::Temporary(_)));
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
}

#[test]
fn iteration_ownership_facts_are_deterministic_across_analyses() {
    let text = "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>, gate: Boolean) {
        var f: () -> Unit = {}
        for (_ in flags) {
            val xs = listOf(1)
            if (gate) {
                f = ({ read(xs) })
                continue
            }
            f = ({ read(xs) })
            break
        }
        val used = f()
    }";
    let mut sources = SourceMap::new();
    let source = sources.add_source("iteration.ko", text).unwrap();
    let parsed = parser_test_assertions::parse_file_twice(&sources, source, "iteration ownership");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (names, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &names).unwrap();
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let first = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    let second = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(first.diagnostics().is_empty(), "{:?}", first.diagnostics());
    assert_eq!(first.iterations().len(), 1);
    assert!(!first.iterations()[0].closure_phi_incomings().is_empty());
    assert_eq!(first.iterations(), second.iterations());
    assert_eq!(first.cleanup_conditions(), second.cleanup_conditions());
    assert_eq!(first.cleanup_steps(), second.cleanup_steps());
    assert_eq!(first.drops(), second.drops());
    assert_eq!(first.loan_ends(), second.loan_ends());
    assert_eq!(first.loans(), second.loans());
    assert_eq!(first.captures(), second.captures());
}

#[test]
fn iteration_plan_publication_is_atomic_on_error_and_skips_unreachable_source() {
    let (_, _, bad) = checked(
        "fun consume(own xs: List<Int>) {}\nfun run(own xs: List<Int>) { for (_ in listOf(1)) {} for (_ in xs) { consume(xs)\nbreak } }",
    );
    assert!(!bad.diagnostics().is_empty());
    assert!(bad.iterations().is_empty());
    let (_, _, unreachable) =
        checked("fun run() { for (_ in if (return) listOf(1) else listOf(2)) {} }");
    assert!(unreachable.iterations().is_empty());
}

#[test]
fn nested_return_plan_orders_each_scope_before_its_provider() {
    use lang_frontend::ownership_checking::{IterationCleanupAction as Action, IterationExitKind};
    let (sources, _, owned) = checked(
        "class Node {}\nfun touch(n: Node) {}\nfun run(own outer: Node, flag: Boolean) { for (_ in listOf(1)) { val local = Node()\nfor (_ in listOf(2)) { if (flag) { return }\ntouch(local) } } touch(outer) }",
    );
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
    let drops = returns[0]
        .actions()
        .iter()
        .filter_map(|action| match action {
            Action::Drop(fact) => Some(sources.slice(fact.value_origin()).unwrap()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(drops, ["listOf(2)", "local", "listOf(1)", "outer"]);
}

#[test]
fn named_source_break_has_its_own_dead_owner_cleanup() {
    use lang_frontend::ownership_checking::{
        DropTarget, IterationCleanupAction as Action, IterationExitKind,
    };
    let (_, _, owned) = checked("fun run(own xs: List<Int>) { for (_ in xs) { break } }");
    let exit = owned.iterations()[0]
        .exits()
        .iter()
        .find(|exit| matches!(exit.kind(), IterationExitKind::Break(_)))
        .unwrap();
    assert!(
        matches!(exit.actions().last(), Some(Action::Drop(fact)) if matches!(fact.target(), DropTarget::Named(_)))
    );
}

#[test]
fn closure_derived_loan_ends_before_iteration_binding() {
    use lang_frontend::ownership_checking::{IterationCleanupAction as Action, IterationExitKind};
    let (_, _, owned) = checked(
        "class Node {}\nfun touch(n: Node) {}\nfun run(xs: List<Node>, flag: Boolean) { for (n in xs) { val f = { touch(n) }\nif (flag) { return }\nf() } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let plan = &owned.iterations()[0];
    let actions = plan
        .exits()
        .iter()
        .find(|exit| matches!(exit.kind(), IterationExitKind::Return(_)))
        .unwrap()
        .actions();
    let capture = actions
        .iter()
        .position(|action| matches!(action, Action::EndCaptureLoan { .. }))
        .unwrap();
    let binding = actions
        .iter()
        .position(|action| matches!(action, Action::EndBinding { .. }))
        .unwrap();
    assert!(capture < binding);
}

#[test]
fn aborting_while_condition_has_no_iteration_backedge_cleanup() {
    use lang_frontend::ownership_checking::IterationExitKind;
    let (_, _, owned) =
        checked("fun run() { for (_ in listOf(1)) { while (error(\"stop\")) {} } }");
    let exits = owned.iterations()[0].exits();
    assert_eq!(exits.len(), 1, "{exits:?}");
    assert_eq!(exits[0].kind(), IterationExitKind::Exhaustion);
}

#[test]
fn lambda_iteration_plans_use_their_own_callable_boundary() {
    use lang_frontend::ownership_checking::{IterationCleanupAction as Action, IterationExitKind};
    let (_, _, owned) = checked(
        "fun run() { for (_ in listOf(1)) { val f = { for (_ in listOf(2)) { return } }\nf() } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.iterations().len(), 2);
    let returning = owned
        .iterations()
        .iter()
        .find(|plan| {
            plan.exits()
                .iter()
                .any(|exit| matches!(exit.kind(), IterationExitKind::Return(_)))
        })
        .unwrap();
    let exit = returning
        .exits()
        .iter()
        .find(|exit| matches!(exit.kind(), IterationExitKind::Return(_)))
        .unwrap();
    assert_eq!(
        exit.actions()
            .iter()
            .filter(|action| matches!(action, Action::FinishProvider(_)))
            .count(),
        1
    );
    let other = owned
        .iterations()
        .iter()
        .find(|plan| plan.descriptor().statement() != returning.descriptor().statement())
        .unwrap();
    assert!(
        other
            .exits()
            .iter()
            .all(|exit| !matches!(exit.kind(), IterationExitKind::Return(_)))
    );
}

#[test]
fn lambda_owned_source_survives_provider_and_returned_owner_is_not_dropped() {
    use lang_frontend::ownership_checking::{
        DropTarget, IterationCleanupAction as Action, IterationExitKind,
    };
    let (_, _, owned) = checked(
        "fun run() { val f: (own List<Int>) -> List<Int> = { xs -> for (_ in xs) {}\nxs } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.iterations().len(), 1);
    let plan = &owned.iterations()[0];
    let exit = plan
        .exits()
        .iter()
        .find(|exit| exit.kind() == IterationExitKind::Exhaustion)
        .unwrap();
    assert!(!exit.actions().iter().any(|action| matches!(action, Action::Drop(fact) if matches!(fact.target(), DropTarget::Named(_)))));
}
