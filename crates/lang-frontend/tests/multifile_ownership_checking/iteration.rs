use super::*;
use lang_frontend::ownership_checking::{
    UnitIterationCleanupAction as Action, UnitIterationExitKind as Exit, UnitIterationSourceAccess,
};

fn analyze(provider_text: &str, consumer_text: &str) -> CompilationUnitOwnership {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(&mut sources, "p/provider.ko", provider_text);
    let (consumer_source, consumer) = parsed(&mut sources, "q/consumer.ko", consumer_text);
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
        .expect("unit ownership checking")
}

#[test]
fn active_provider_prevents_moving_source_even_before_break() {
    let owned = analyze(
        "package p\nfun take(own xs: Array<Int>): Unit {}",
        "package q\nfun run(own xs: Array<Int>) { for (_ in xs) { p.take(xs); break } }",
    );
    assert_eq!(diagnostic_codes(&owned), ["L0135"]);
    assert!(owned.validate().is_err());
}

#[test]
fn three_containers_and_five_source_kinds_publish_valid_shared_plans() {
    for (container, factory) in [
        ("Array", "arrayOf"),
        ("List", "listOf"),
        ("MutableList", "mutableListOf"),
    ] {
        let provider = format!(
            "package p\nclass Holder(val xs: {container}<Int>)\nfun make(): {container}<Int> = {factory}(1, 2)"
        );
        for (parameter, expression, access) in [
            (
                format!("own xs: {container}<Int>"),
                "xs",
                UnitIterationSourceAccess::Owned,
            ),
            (
                format!("xs: {container}<Int>"),
                "xs",
                UnitIterationSourceAccess::Shared,
            ),
            (
                format!("inout xs: {container}<Int>"),
                "xs",
                UnitIterationSourceAccess::Exclusive,
            ),
            (
                "holder: p.Holder".to_owned(),
                "holder.xs",
                UnitIterationSourceAccess::Shared,
            ),
            (
                String::new(),
                "p.make()",
                UnitIterationSourceAccess::Temporary,
            ),
        ] {
            let owned = analyze(
                &provider,
                &format!(
                    "package q\nfun run({parameter}) {{ for (item in {expression}) {{ val n: Int = item }} }}"
                ),
            );
            assert!(
                owned.diagnostics().is_empty(),
                "{container}/{expression}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
            assert_eq!(owned.iterations().len(), 1);
            let plan = &owned.iterations()[0];
            assert_eq!(plan.source_access(), access);
            assert_eq!(plan.bindings().len(), 1);
            assert!(
                plan.bindings()
                    .iter()
                    .all(|binding| binding.kind() == OwnershipBindingKind::Shared)
            );
            assert_eq!(owned.iteration(plan.descriptor().statement()), Some(plan));
            assert_eq!(plan.exits().len(), 2);
            assert!(
                owned.validate().is_ok(),
                "{container}/{expression}: complete frontend facts must validate"
            );
        }
    }
}

#[test]
fn source_loan_conflicts_precede_borrowed_move_and_end_after_provider() {
    for (parameter, body) in [
        ("xs: List<Int>", "p.take(xs)\nbreak"),
        ("inout xs: List<Int>", "p.change(&xs)\nbreak"),
        ("own holder: p.Holder", "p.take(holder.xs)\nbreak"),
    ] {
        let expression = if parameter.contains("holder") {
            "holder.xs"
        } else {
            "xs"
        };
        let owned = analyze(
            "package p\nclass Holder(val xs: List<Int>)\nfun take(own xs: List<Int>) {}\nfun change(inout xs: List<Int>) {}",
            &format!("package q\nfun run({parameter}) {{ for (_ in {expression}) {{ {body} }} }}"),
        );
        assert_eq!(diagnostic_codes(&owned), ["L0135"], "{parameter}/{body}");
        assert!(owned.iterations().is_empty());
        assert!(owned.drops().is_empty());
    }
    let owned = analyze(
        "package p\nfun read(xs: List<Int>) {}",
        "package q\nfun run(own xs: List<Int>): List<Int> { for (item in xs) { p.read(xs)\nfor (_ in xs) {} }\nreturn xs }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.iterations().len(), 2);
    assert!(owned.validate().is_ok());
}

#[test]
fn temporary_sources_release_only_on_terminal_exits() {
    for (jump, terminal) in [("continue", false), ("break", true), ("return", true)] {
        let owned = analyze(
            "package p\nfun make(): List<Int> = listOf(1, 2)",
            &format!("package q\nfun run() {{ for (item in p.make()) {{ {jump} }} }}"),
        );
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let plan = &owned.iterations()[0];
        let UnitLoanTarget::Temporary(source) = plan.source() else {
            panic!("temporary source");
        };
        let exit = plan
            .exits()
            .iter()
            .find(|exit| !matches!(exit.kind(), Exit::Exhaustion))
            .unwrap();
        let statement = plan.descriptor().statement();
        assert_eq!(exit.actions().iter().filter(|action| matches!(action, Action::Drop(fact) if fact.target() == UnitDropTarget::Temporary(*source))).count(), usize::from(terminal), "{jump}");
        assert_eq!(
            exit.actions().contains(&Action::FinishProvider(statement)),
            terminal
        );
        let element = exit
            .actions()
            .iter()
            .position(|action| *action == Action::EndElement(statement))
            .unwrap();
        if terminal {
            let finish = exit
                .actions()
                .iter()
                .position(|action| *action == Action::FinishProvider(statement))
                .unwrap();
            let end_source = exit
                .actions()
                .iter()
                .position(|action| *action == Action::EndSource(statement))
                .unwrap();
            let drop_source = exit.actions().iter().position(|action| matches!(action, Action::Drop(fact) if fact.target() == UnitDropTarget::Temporary(*source))).unwrap();
            assert!(element < finish && finish < end_source && end_source < drop_source);
        }
        assert!(owned.validate().is_ok(), "{jump}");
    }
}

#[test]
fn borrowed_components_reject_move_inout_and_owned_capture() {
    for (binding, element, body, code) in [
        ("item", "String", "p.take(item)", "L0133"),
        ("(item, _)", "p.Pair", "p.take(item)", "L0133"),
        ("item", "String", "p.change(&item)", "L0134"),
        ("item", "String", "val f = move { println(item) }", "L0138"),
    ] {
        let owned = analyze(
            "package p\nvalue class Pair(val text: String, val count: Int)\nfun take(own text: String) {}\nfun change(inout text: String) {}",
            &format!(
                "package q\nfun run(xs: List<{element}>) {{ for ({binding} in xs) {{ {body} }} }}"
            ),
        );
        assert_eq!(diagnostic_codes(&owned), [code], "{binding}/{body}");
        assert!(owned.iterations().is_empty());
    }
}

#[test]
fn nested_return_cleans_body_then_inner_provider_then_outer_provider_and_scope() {
    let owned = analyze(
        "package p\nclass Guard { deinit() {} }\nfun guard(): Guard = Guard()\nfun make(): List<Int> = listOf(1)",
        "package q\nfun run() { val outside = p.guard()\nfor (a in p.make()) { val outer = p.guard()\nfor (b in p.make()) { val inner = p.guard()\nreturn } } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert_eq!(owned.iterations().len(), 2);
    let return_exits = owned
        .iterations()
        .iter()
        .flat_map(|plan| plan.exits())
        .filter(|exit| matches!(exit.kind(), Exit::Return(_)))
        .collect::<Vec<_>>();
    assert_eq!(return_exits.len(), 2);
    assert_eq!(return_exits[0].actions(), return_exits[1].actions());
    let actions = return_exits[0].actions();
    let providers = actions
        .iter()
        .filter_map(|action| match action {
            Action::FinishProvider(statement) => Some(*statement),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(providers.len(), 2);
    let inner = owned.iteration(providers[0]).unwrap();
    let outer = owned.iteration(providers[1]).unwrap();
    assert!(
        inner.descriptor().source().expression().index()
            > outer.descriptor().source().expression().index()
    );
    let drops = actions
        .iter()
        .enumerate()
        .filter_map(|(i, action)| match action {
            Action::Drop(fact) if matches!(fact.target(), UnitDropTarget::Named(_)) => Some(i),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        drops.len(),
        3,
        "three distinct guards must each be released"
    );
    let inner_end = actions
        .iter()
        .position(|action| *action == Action::EndElement(providers[0]))
        .unwrap();
    let outer_end = actions
        .iter()
        .position(|action| *action == Action::EndElement(providers[1]))
        .unwrap();
    let outer_source_end = actions
        .iter()
        .position(|action| *action == Action::EndSource(providers[1]))
        .unwrap();
    assert!(
        drops[0] < inner_end
            && inner_end < drops[1]
            && drops[1] < outer_end
            && outer_source_end < drops[2]
    );
    assert!(owned.validate().is_ok());
}

#[test]
fn returns_stop_at_nearest_callable_and_source_return_precedes_acquire() {
    let owned = analyze(
        "package p\nfun make(): List<Int> = listOf(1)",
        "package q\nfun run(flag: Boolean) { for (outer in p.make()) { val action: () -> Unit = { for (inner in p.make()) { return } }\naction()\nfor (_ in if (flag) { return } else p.make()) {} } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.iterations().len(), 3);
    let returns = owned
        .iterations()
        .iter()
        .map(|plan| {
            plan.exits()
                .iter()
                .filter_map(|exit| match exit.kind() {
                    Exit::Return(expression) => Some(expression),
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        returns.iter().filter(|exits| exits.is_empty()).count(),
        1,
        "source return occurs before its own provider exists"
    );
    let nonempty = returns
        .iter()
        .filter(|exits| !exits.is_empty())
        .collect::<Vec<_>>();
    assert_eq!(nonempty.len(), 2);
    assert_eq!(nonempty[0].len(), 1);
    assert_eq!(nonempty[1].len(), 1);
    assert_ne!(
        nonempty[0][0], nonempty[1][0],
        "lambda return cannot clean the outer callable's provider"
    );
    assert!(owned.validate().is_ok());
}

#[test]
fn nested_while_jumps_preserve_outer_provider_and_abort_has_no_cleanup_edge() {
    let owned = analyze(
        "package p\nclass Guard { deinit() {} }\nfun guard(): Guard = Guard()\nfun make(): List<Int> = listOf(1)",
        "package q\nfun run(flag: Boolean) { for (item in p.make()) { while(flag) { if(flag) {continue}\nbreak }\nval guard = p.guard()\nerror(\"stop\") } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let plan = &owned.iterations()[0];
    assert_eq!(plan.exits().len(), 1);
    assert_eq!(plan.exits()[0].kind(), Exit::Exhaustion);
    assert!(
        owned
            .drops()
            .iter()
            .all(|fact| !matches!(fact.point(), UnitDropPoint::ControlTransfer(_))),
        "Abort and inner-loop jumps do not unwind the provider or its later guard"
    );
    assert!(owned.validate().is_ok());
}

#[test]
fn named_source_has_no_early_drop_when_loop_is_its_last_explicit_read() {
    let owned = analyze(
        "package p\nfun read(value: Int) {}",
        "package q\nfun run(own xs: List<Int>) { for (item in xs) { p.read(item) } }",
    );
    let plan = &owned.iterations()[0];
    let UnitLoanTarget::Place(source) = plan.source() else {
        panic!("named source");
    };
    let drops = owned
        .drops()
        .iter()
        .filter(|fact| fact.target() == UnitDropTarget::Named(source.root()))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1);
    assert_eq!(
        drops[0].point(),
        UnitDropPoint::LoopExit(plan.descriptor().statement())
    );
    let cleanup = owned.iteration_cleanup_at(drops[0].point()).unwrap();
    let end = cleanup
        .iter()
        .position(|action| *action == Action::EndSource(plan.descriptor().statement()))
        .unwrap();
    let release = cleanup
        .iter()
        .position(|action| matches!(action, Action::Drop(fact) if fact == drops[0]))
        .unwrap();
    assert!(
        end < release,
        "the final explicit source read does not shorten its provider loan"
    );
    assert!(owned.validate().is_ok());
}

#[test]
fn return_ends_pending_element_argument_loan_before_element_and_source() {
    let owned = analyze(
        "package p\nfun use(text: String, count: Int) {}\nfun make(): List<String> = listOf(\"value\")",
        "package q\nfun run(flag: Boolean) { for (item in p.make()) { p.use(item, if(flag) {return} else 1) } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let plan = &owned.iterations()[0];
    let exit = plan
        .exits()
        .iter()
        .find(|exit| matches!(exit.kind(), Exit::Return(_)))
        .unwrap();
    let end_call = exit
        .actions()
        .iter()
        .position(|action| matches!(action, Action::EndCallLoan(_)))
        .expect("abandoned call prefix must end its element loan");
    let end_element = exit
        .actions()
        .iter()
        .position(|action| matches!(action, Action::EndElement(_)))
        .unwrap();
    assert!(end_call < end_element);
    assert!(owned.validate().is_ok());
}

#[test]
fn return_ends_shared_capture_before_element_and_keeps_closure_drop_unique() {
    let owned = analyze(
        "package p\nfun make(): List<String> = listOf(\"value\")",
        "package q\nfun run(flag: Boolean) { for (item in p.make()) { val f: () -> Unit = { println(item) }\nif(flag) {return}\nf() } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let plan = &owned.iterations()[0];
    let exit = plan
        .exits()
        .iter()
        .find(|exit| matches!(exit.kind(), Exit::Return(_)))
        .unwrap();
    let end_capture = exit
        .actions()
        .iter()
        .position(|action| matches!(action, Action::EndCaptureLoan { .. }))
        .expect("live local closure releases its shared capture on return");
    let end_element = exit
        .actions()
        .iter()
        .position(|action| matches!(action, Action::EndElement(_)))
        .unwrap();
    assert!(end_capture < end_element);
    assert_eq!(exit.actions().iter().filter(|action| matches!(action, Action::Drop(fact) if matches!(fact.target(), UnitDropTarget::Named(_)))).count(), 1);
    assert!(owned.validate().is_ok());
}

#[test]
fn field_sources_preserve_the_receiver_access_and_temporary_owner() {
    for (provider, consumer, expected) in [
        (
            "package p\nclass Holder(val xs: List<Int>) { fun scan() { for (_ in this.xs) {} } }",
            "package q\nfun run() {}",
            UnitIterationSourceAccess::Shared,
        ),
        (
            "package p\nclass Holder(val xs: List<Int>)\nfun make(): Holder = Holder(listOf(1))",
            "package q\nfun run() { for (_ in p.make().xs) {} }",
            UnitIterationSourceAccess::Temporary,
        ),
        (
            "package p\nfun use(f: () -> Unit) { f() }",
            "package q\nfun run(own xs: List<Int>) { val scan: () -> Unit = { for (_ in xs) {} }\np.use(scan) }",
            UnitIterationSourceAccess::Shared,
        ),
    ] {
        let owned = analyze(provider, consumer);
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert_eq!(owned.iterations().len(), 1);
        assert_eq!(owned.iterations()[0].source_access(), expected);
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn return_only_ends_already_evaluated_argument_loans() {
    let owned = analyze(
        "package p\nfun use(first: String, count: Int, last: String) {}",
        "package q\nfun run(xs: List<String>, flag: Boolean) { for (item in xs) { p.use(item, if(flag) {return} else 1, item) } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let exit = owned.iterations()[0]
        .exits()
        .iter()
        .find(|exit| matches!(exit.kind(), Exit::Return(_)))
        .unwrap();
    assert_eq!(
        exit.actions()
            .iter()
            .filter(|action| matches!(action, Action::EndCallLoan(_)))
            .count(),
        1,
        "the suffix argument was not evaluated before this return"
    );
    assert!(owned.validate().is_ok());
}

#[test]
fn return_ends_pending_member_receiver_before_its_iteration_element() {
    let owned = analyze(
        "package p\nclass Item { fun read(n: Int) {} }",
        "package q\nfun run(xs: List<p.Item>, flag: Boolean) { for (item in xs) { item.read(if(flag) {return} else 1) } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let exit = owned.iterations()[0]
        .exits()
        .iter()
        .find(|exit| matches!(exit.kind(), Exit::Return(_)))
        .unwrap();
    let end_receiver = exit
        .actions()
        .iter()
        .position(|action| matches!(action, Action::EndReceiverLoan(_)))
        .expect("the abandoned member call must end its already evaluated receiver loan");
    let end_element = exit
        .actions()
        .iter()
        .position(|action| matches!(action, Action::EndElement(_)))
        .unwrap();
    assert!(end_receiver < end_element);
    assert!(owned.validate().is_ok());
}

#[test]
fn abandoned_temporary_closure_ends_capture_before_element() {
    let owned = analyze(
        "package p\nfun use(f: () -> Unit, count: Int) {}",
        "package q\nfun run(xs: List<String>, flag: Boolean) { for (item in xs) { p.use({ println(item) }, if(flag) {return} else 1) } }",
    );
    let exit = owned.iterations()[0]
        .exits()
        .iter()
        .find(|exit| matches!(exit.kind(), Exit::Return(_)))
        .unwrap();
    let capture = exit
        .actions()
        .iter()
        .position(|action| matches!(action, Action::EndCaptureLoan { .. }))
        .expect("abandoned temporary closure ends its shared capture");
    let element = exit
        .actions()
        .iter()
        .position(|action| matches!(action, Action::EndElement(_)))
        .unwrap();
    assert!(capture < element);
    assert!(owned.validate().is_ok());
}

#[test]
fn abandoned_temporary_move_closure_drops_owned_slots_before_element() {
    let owned = analyze(
        "package p\nfun use(f: () -> Unit, count: Int) {}",
        "package q\nfun run(xs: List<Int>, flag: Boolean) { for (item in xs) {\n\
         val first = \"first\"\nval second = \"second\"\n\
         p.use(move { println(first); println(second) }, if(flag) {return} else 1)\n} }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let captures = owned.captures();
    assert_eq!(captures.len(), 2);
    assert!(
        captures
            .iter()
            .all(|capture| capture.mode() == ClosureCaptureMode::Owned
                && capture.effect() == ClosureCaptureEffect::Move)
    );
    let closure = captures[0].lambda();
    assert_eq!(captures[1].lambda(), closure);
    let exit = owned.iterations()[0]
        .exits()
        .iter()
        .find(|exit| matches!(exit.kind(), Exit::Return(_)))
        .expect("return abandons already evaluated callback");
    let expected = [
        UnitDropTarget::Captured {
            closure,
            source: captures[1].source(),
        },
        UnitDropTarget::Captured {
            closure,
            source: captures[0].source(),
        },
        UnitDropTarget::Temporary(closure),
    ];
    let drops = exit
        .actions()
        .iter()
        .enumerate()
        .filter_map(|(index, action)| {
            let Action::Drop(fact) = action else {
                return None;
            };
            expected.contains(&fact.target()).then_some((index, *fact))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        drops
            .iter()
            .map(|(_, fact)| fact.target())
            .collect::<Vec<_>>(),
        expected,
        "iteration cleanup releases both owned captures before the temporary environment"
    );
    let end_call = exit
        .actions()
        .iter()
        .position(|action| matches!(action, Action::EndCallLoan(_)))
        .expect("pending callback borrow ends first");
    let end_element = exit
        .actions()
        .iter()
        .position(|action| matches!(action, Action::EndElement(_)))
        .expect("iteration element ends after callback cleanup");
    assert!(end_call < drops[0].0 && drops[2].0 < end_element);
    for (_, fact) in &drops {
        assert_eq!(fact.point(), exit.point());
        assert_eq!(
            owned.drops().iter().filter(|flat| *flat == fact).count(),
            1,
            "ordered action reuses the same flat drop identity exactly once"
        );
    }
    assert!(owned.validate().is_ok());
}

#[test]
fn unreachable_lambda_does_not_require_an_unacquired_iteration_provider() {
    let owned = analyze(
        "package p\nfun noop() {}",
        "package q\nfun run() { return\nval dead: () -> Unit = { for (_ in listOf(1)) {} } }",
    );
    assert!(owned.diagnostics().is_empty());
    assert!(owned.iterations().is_empty());
    assert!(owned.validate().is_ok());
}

#[test]
fn returning_source_index_does_not_acquire_its_provider() {
    let owned = analyze(
        "package p\nfun noop() {}",
        "package q\nfun run(xs: List<List<Int>>, flag: Boolean) { for (_ in xs[if(flag) {return} else {return}]) {} }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.iterations().is_empty());
    assert!(owned.validate().is_ok());
}

#[test]
fn returning_source_index_drops_only_its_evaluated_backing_owner() {
    let owned = analyze(
        "package p\nfun make(): List<List<Int>> = listOf(listOf(1))",
        "package q\nfun run(flag: Boolean) { for (_ in p.make()[if(flag) {return} else {return}]) {} }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.iterations().is_empty());
    assert_eq!(
        owned
            .drops()
            .iter()
            .filter(|fact| matches!(fact.target(), UnitDropTarget::Temporary(_))
                && matches!(fact.point(), UnitDropPoint::ControlTransfer(_)))
            .count(),
        2,
        "each branch releases the evaluated backing owner without acquiring a provider"
    );
    assert!(owned.validate().is_ok());
}

#[test]
fn conditional_receiver_cleanup_follows_its_formation_scope() {
    for (body, inside) in [
        (
            "for (_ in xs) { consume(if(flag) {return} else {return}) }",
            true,
        ),
        (
            "consume(if(flag) { for (_ in xs) {return}\n1 } else 1)",
            false,
        ),
    ] {
        let owned = analyze(
            &format!(
                "package p\ninterface Relay {{ own fun consume(n: Int): Unit {{}}\nown fun run(xs: List<Int>, flag: Boolean): Unit {{ {body} }} }}"
            ),
            "package q\nfun noop() {}",
        );
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let plan = &owned.iterations()[0];
        assert_eq!(
            plan.exits()
                .iter()
                .filter(|exit| matches!(exit.kind(), Exit::Return(_)))
                .count(),
            if inside { 2 } else { 1 },
            "every source return must publish a cleanup plan before checking its ordering"
        );
        for exit in plan
            .exits()
            .iter()
            .filter(|exit| matches!(exit.kind(), Exit::Return(_)))
        {
            let fact = owned
                .conditional_receiver_drops()
                .iter()
                .find(|fact| fact.point() == exit.point())
                .unwrap();
            assert_eq!(
                fact.preceding_drops(),
                0,
                "flat drop ordinal alone cannot distinguish these two orders"
            );
            let drop = exit
                .actions()
                .iter()
                .position(|action| *action == Action::DropConditionalReceiver(*fact))
                .expect(
                    "the complete iteration sequence must include conditional receiver destruction",
                );
            let boundary = exit
                .actions()
                .iter()
                .position(|action| {
                    *action
                        == if inside {
                            Action::EndElement(plan.descriptor().statement())
                        } else {
                            Action::EndSource(plan.descriptor().statement())
                        }
                })
                .unwrap();
            assert_eq!(
                drop < boundary,
                inside,
                "body pending receiver precedes element end; enclosing pending receiver follows source end"
            );
        }
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn conditional_receiver_between_nested_providers_cleans_between_their_exits() {
    let owned = analyze(
        "package p\ninterface Relay { own fun consume(n: Int): Unit {}\nown fun run(xs: List<Int>, flag: Boolean): Unit { for (_ in xs) { consume(if(flag) { for (_ in xs) {return}\nreturn } else {return}) } } }",
        "package q\nfun noop() {}",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let exit = owned
        .iterations()
        .iter()
        .flat_map(|plan| plan.exits())
        .find(|exit| {
            matches!(exit.kind(), Exit::Return(_))
                && exit
                    .actions()
                    .iter()
                    .filter(|action| matches!(action, Action::EndSource(_)))
                    .count()
                    == 2
        })
        .unwrap();
    let inner_source_end = exit
        .actions()
        .iter()
        .position(|action| matches!(action, Action::EndSource(_)))
        .unwrap();
    let receiver_drop = exit
        .actions()
        .iter()
        .position(|action| matches!(action, Action::DropConditionalReceiver(_)))
        .unwrap();
    let outer_element_end = exit
        .actions()
        .iter()
        .rposition(|action| matches!(action, Action::EndElement(_)))
        .unwrap();
    assert!(
        inner_source_end < receiver_drop && receiver_drop < outer_element_end,
        "a receiver formed between providers must be released after the inner source and before the outer element"
    );
    assert!(owned.validate().is_ok());
}
