//! 普通借用的 caller continuation 与 scope end；双布局 Unit 与实际 loan-end 证据。
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{CompilationUnitOwnership, check_compilation_unit_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

fn checked_entry(body: &str, rooted: bool) -> (SourceMap, CompilationUnitOwnership) {
    let text = format!(
        "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(source)\nfun consume(own source: String) {{}}\n{body}"
    );
    let mut sources = SourceMap::new();
    let source = sources.add_source("continuation.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    let inputs = [if rooted {
        SourceUnitInput::new("root", "continuation.ko", source, &parsed)
    } else {
        SourceUnitInput::memory_document(source, &parsed)
    }];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let [indexed] = index.source_units() else {
        panic!("one original source")
    };
    assert_eq!(indexed.source_id(), source);
    match (rooted, indexed.key()) {
        (false, lang_frontend::name_resolution::SourceUnitKey::MemoryDocument) => {}
        (true, lang_frontend::name_resolution::SourceUnitKey::Rooted { root, logical_path }) => {
            assert_eq!(root.as_str(), "root");
            assert_eq!(logical_path.as_str(), "continuation.ko");
        }
        _ => panic!("requested entry identity"),
    }
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let typed = typed.validate().unwrap();
    let unit = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    (sources, unit)
}

#[test]
fn nested_reborrow_and_alias_scopes_restore_owner_permission() {
    for rooted in [false, true] {
        for body in [
            "fun run() { val source = \"kept\"; block { borrow val item = wrap(source); println(item) }; consume(source) }",
            "fun run() { val source = \"kept\"; block { borrow val item = view(source); block { borrow val alias = item; println(alias) }; println(item) }; consume(source) }",
            "fun run() { val source = \"kept\"; block { borrow val item = view(source); block { borrow val child = view(item); println(child) }; println(item) }; consume(source) }",
            "fun run(flag: Boolean) { val source = \"kept\"; if (flag) { borrow val item = view(source); println(item) } else { borrow val item = source; println(item) }; consume(source) }",
        ] {
            let (_, unit) = checked_entry(body, rooted);
            assert!(
                unit.diagnostics().is_empty(),
                "{body}: {:?}",
                unit.diagnostics()
            );
            assert!(unit.deferred().is_empty());
        }
    }
}

#[test]
fn live_results_and_children_block_source_move_on_every_branch() {
    for rooted in [false, true] {
        for body in [
            "fun run() { val source = \"kept\"; borrow val item = view(source); consume(source); println(item) }",
            "fun run() { val source = \"kept\"; borrow val item = view(source); block { borrow val child = view(item); consume(source); println(child) }; println(item) }",
            "fun run(flag: Boolean) { val source = \"kept\"; borrow val item = view(source); if (flag) { consume(source) }; println(item) }",
            "fun run() { val source = \"kept\"; borrow val item = source; block { borrow val alias = item; println(alias) }; consume(source); println(item) }",
        ] {
            let (_, unit) = checked_entry(body, rooted);
            {
                let diagnostics = unit.diagnostics();
                assert!(
                    diagnostics.iter().any(|d| d.code().to_string() == "L0135"),
                    "{body}: {diagnostics:?}"
                );
            }
        }
    }
}

#[test]
fn results_continue_original_source_loans_and_never_own_the_payload() {
    for rooted in [false, true] {
        use lang_frontend::ownership_checking::{
            UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction, UnitLoanAction,
            UnitLoanTarget,
        };
        let (sources, unit) = checked_entry(
            "fun run() { val source = \"kept\"; block { borrow val item = wrap(source); println(item) } }",
            rooted,
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.deferred().is_empty(), "{:?}", unit.deferred());
        let [binding] = unit.borrow_results().bindings() else {
            panic!("one initialized result binding");
        };
        let source = binding.source_loan().unwrap();
        let UnitLoanTarget::Place(origin) = binding.origin() else {
            panic!("stable caller source");
        };
        let source_loan = unit
            .loans()
            .iter()
            .find(|loan| loan.call() == source.call() && loan.argument() == source.argument())
            .unwrap();
        assert_eq!(source_loan.target(), binding.origin());
        assert_eq!(sources.slice(source_loan.end_span()), Ok("wrap(source)"));
        let [end] = unit.borrow_results().ends() else {
            panic!("one actual result end");
        };
        assert_eq!(end.binding(), binding.binding());
        let [transport] = unit.borrow_binding_transports() else {
            panic!("actual caller source token reaches its result end");
        };
        assert_eq!(transport.source_loan(), source_loan);
        assert_eq!(transport.binding(), binding.binding());
        assert_eq!(
            transport.handoff_point(),
            UnitDropPoint::CallReturn(source.call())
        );
        assert_eq!(transport.point(), end.point());
        assert_ne!(transport.point(), transport.handoff_point());
        let drops = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(origin.root()))
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].point(), end.point());
        assert!(!unit.drops().iter().any(|drop| drop.target()
            == UnitDropTarget::Named(binding.binding())
            || drop.target() == UnitDropTarget::Temporary(binding.initializer())));

        let [forwarded] = unit.borrow_results().forwarded_source_loans() else {
            panic!("one forwarded wrapper source");
        };
        let [forward] = unit.borrow_forward_transports() else {
            panic!("one actual token delivery to the wrapper result");
        };
        let forward_loan = forward.source_loan();
        assert_eq!(forward_loan.call(), forwarded.call());
        assert_eq!(forward_loan.argument(), forwarded.argument());
        assert_eq!(sources.slice(forward_loan.end_span()), Ok("view(source)"));
        assert_ne!(forward_loan.call(), source_loan.call());
        assert_eq!(forward.returned_expression(), forwarded.call());
        assert_eq!(
            forward.handoff_point(),
            UnitDropPoint::CallReturn(forwarded.call())
        );
        assert_eq!(
            forward.point(),
            UnitDropPoint::ControlTransfer(forwarded.call())
        );
        let observations = unit.loan_action_observations();
        let observed: Vec<_> = observations
            .iter()
            .flat_map(|observation| {
                observation
                    .actions()
                    .iter()
                    .map(move |action| (observation, action))
            })
            .filter_map(|(observation, action)| match action {
                UnitLoanAction::BorrowForwardHandoff(handoff) => Some((observation, handoff)),
                _ => None,
            })
            .collect();
        let [(observation, handoff)] = observed.as_slice() else {
            panic!("one observed delivery of the live wrapper token");
        };
        assert_eq!(handoff.transaction(), forward);
        let acquisition = handoff
            .source_begin()
            .expect("exact actual wrapper source acquisition");
        assert_eq!(
            observation.action(&acquisition),
            Some(&UnitLoanAction::Begin(forward_loan.clone()))
        );
        assert!(
            !observations
                .iter()
                .flat_map(|observation| observation.actions())
                .any(|action| matches!(action, UnitLoanAction::Cleanup {
                point,
                action: UnitIterationCleanupAction::EndCallLoan(loan)
            } if loan == forward_loan
                || (loan == source_loan && *point == transport.handoff_point())))
        );
        assert!(unit.validate().is_ok());
    }
}

#[test]
fn children_end_before_parents_and_owner_cleanup_on_scope_and_return_edges() {
    for rooted in [false, true] {
        use lang_frontend::ownership_checking::{UnitDropTarget, UnitLoanTarget};
        for exit in ["", "return"] {
            let (_, unit) = checked_entry(
                &format!(
                    "fun run() {{ val source = \"kept\"; borrow val parent = view(source); borrow val child = view(parent); println(child); {exit} }}"
                ),
                rooted,
            );
            assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
            let bindings = unit.borrow_results().bindings();
            assert_eq!(bindings.len(), 2);
            assert_eq!(bindings[1].parent(), Some(bindings[0].binding()));
            assert_eq!(bindings[1].origin(), bindings[0].origin());
            let ends = unit.borrow_results().ends();
            assert_eq!(
                ends.iter().map(|end| end.binding()).collect::<Vec<_>>(),
                vec![bindings[1].binding(), bindings[0].binding()]
            );
            let UnitLoanTarget::Place(origin) = bindings[0].origin() else {
                panic!("stable unit source");
            };
            assert!(
                unit.drops()
                    .iter()
                    .any(|drop| drop.target() == UnitDropTarget::Named(origin.root())
                        && drop.point() == ends[1].point())
            );
        }
    }
}

#[test]
fn source_mutation_and_exclusive_reborrow_conflicts_keep_real_spans() {
    for rooted in [false, true] {
        for body in [
            "fun run() { var source = \"kept\"; borrow val item = view(source); source = \"changed\"; println(item) }",
            "fun mutate(inout source: String) {}\nfun run() { var source = \"kept\"; borrow val item = view(source); mutate(&source); println(item) }",
            "class Record(var text: String)\nfun run() { val source = Record(\"kept\"); borrow val item = source; item.text = \"changed\"; println(item.text) }",
        ] {
            let (sources, unit) = checked_entry(body, rooted);
            {
                let diagnostics = unit.diagnostics();
                let conflict = diagnostics
                    .iter()
                    .find(|d| d.code().to_string() == "L0135")
                    .unwrap_or_else(|| panic!("{body}: {diagnostics:?}"));
                assert!(!sources.slice(conflict.primary_span()).unwrap().is_empty());
                assert!(!conflict.details().is_empty());
            }
            assert!(unit.borrow_results().bindings().is_empty());
        }
    }
}

#[test]
fn global_borrow_binding_is_rejected_by_parser_at_its_marker() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "global.ko",
            "val globalSource = \"kept\"\nborrow val globalView = globalSource",
        )
        .unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert_eq!(parsed.diagnostics().len(), 1);
    let diagnostic = &parsed.diagnostics()[0];
    assert_eq!(diagnostic.code().to_string(), "L0076");
    assert_eq!(sources.slice(diagnostic.primary_span()).unwrap(), "borrow");
}

fn assert_ownership_rejects(body: &str, code: &str) {
    for rooted in [false, true] {
        let (_, unit) = checked_entry(body, rooted);
        {
            let diagnostics = unit.diagnostics();
            assert!(
                diagnostics.iter().any(|d| d.code().to_string() == code),
                "{body}: {diagnostics:?}"
            );
        }
        assert!(unit.borrow_results().bindings().is_empty());
        assert!(unit.borrow_results().ends().is_empty());
    }
}

#[test]
fn borrowed_result_capture_remains_closed_in_ownership() {
    assert_ownership_rejects(
        "fun run() { val source = \"kept\"; borrow val item = view(source); val saved = { println(item) }; saved() }",
        "L0164",
    );
}

#[test]
fn conditional_borrow_initializer_remains_closed_in_ownership() {
    assert_ownership_rejects(
        "fun run(flag: Boolean) { val source = \"kept\"; borrow val item = if (flag) { source } else { source }; println(item) }",
        "L0164",
    );
}

#[test]
fn nested_borrow_result_call_remains_closed_in_ownership() {
    assert_ownership_rejects(
        "fun run() { val source = \"kept\"; borrow val item = view(view(source)); println(item) }",
        "L0164",
    );
}

#[test]
fn temporary_borrow_source_is_rejected_in_ownership() {
    assert_ownership_rejects(
        "fun run() { borrow val item = view(\"temporary\"); println(item) }",
        "L0162",
    );
}

#[test]
fn loop_borrow_binding_remains_closed_in_ownership() {
    assert_ownership_rejects(
        "fun run() { val source = \"kept\"; loop { borrow val item = view(source); println(item); break } }",
        "L0164",
    );
}

#[test]
fn nullable_generic_and_copyable_bindings_keep_shared_capability() {
    for rooted in [false, true] {
        for body in [
            "fun nullableView(source: String?): borrow String? from source = source\nfun observe(source: String?) {}\nfun run(source: String?) { borrow val item = nullableView(source); observe(item) }",
            "fun <T> genericView(source: T): borrow T from source = source\nclass Resource { deinit() {} }\nfun run() { val source = Resource(); block { borrow val item = genericView(source) } }",
            "fun intView(source: Int): borrow Int from source = source\nfun observe(own item: Int) {}\nfun run(source: Int) { borrow val item = intView(source); observe(item) }",
        ] {
            let (_, unit) = checked_entry(body, rooted);
            assert!(
                unit.diagnostics().is_empty(),
                "{body}: {:?}",
                unit.diagnostics()
            );
            assert_eq!(unit.borrow_results().bindings().len(), 1);
            assert_eq!(unit.borrow_results().ends().len(), 1);
        }
    }
}

#[test]
fn named_source_continues_only_its_loan_and_other_arguments_end_at_call_return() {
    for rooted in [false, true] {
        use lang_frontend::ownership_checking::{LoanKind, UnitDropPoint};
        let (sources, unit) = checked_entry(
            "fun select(other: String, source: String): borrow String from source = source\nfun run() { val source = \"kept\"; val other = \"other\"; borrow val item = select(source = source, other = other); println(item) }",
            rooted,
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.deferred().is_empty(), "{:?}", unit.deferred());
        let [binding] = unit.borrow_results().bindings() else {
            panic!("one initialized result binding");
        };
        let source = binding.source_loan().unwrap();
        let [handoff] = unit.borrow_call_handoffs() else {
            panic!("one actual initializer handoff");
        };
        let [auxiliary] = handoff.auxiliary_loan_ends() else {
            panic!("only the other argument ends at the initializer return");
        };
        let source_loan = handoff.source_loan();
        assert_eq!(handoff.binding(), binding.binding());
        assert_eq!(source_loan.call(), source.call());
        assert_eq!(source_loan.argument(), source.argument());
        assert_eq!(source_loan.target(), binding.origin());
        assert_ne!(auxiliary.argument(), source.argument());
        assert_eq!(auxiliary.call(), source.call());
        assert_eq!(source_loan.kind(), LoanKind::Shared);
        assert_eq!(auxiliary.kind(), LoanKind::Shared);
        assert_eq!(sources.slice(source_loan.begin_span()), Ok("source"));
        assert_eq!(sources.slice(auxiliary.begin_span()), Ok("other"));
        assert_eq!(handoff.point(), UnitDropPoint::CallReturn(source.call()));
        let call_loans = unit
            .loans()
            .iter()
            .filter(|loan| loan.call() == source.call())
            .collect::<Vec<_>>();
        assert_eq!(call_loans, [source_loan, auxiliary]);
        assert!(
            unit.direct_call_loan_ends()
                .iter()
                .all(|end| end.loan().call() != source.call())
        );

        // The handoff really removes the auxiliary pending token at CallReturn. The
        // source is instead carried intact to the actual later result termination.
        let [transport] = unit.borrow_binding_transports() else {
            panic!("one actual source-token transport");
        };
        let [end] = unit.borrow_results().ends() else {
            panic!("one actual result end");
        };
        assert_eq!(transport.source_loan(), source_loan);
        assert_eq!(end.binding(), binding.binding());
        assert_eq!(transport.binding(), end.binding());
        assert_eq!(transport.handoff_point(), handoff.point());
        assert_eq!(transport.point(), end.point());
        assert_ne!(transport.point(), handoff.point());
        assert!(unit.validate().is_ok());
        let (_, unit) = checked_entry(
            "fun select(other: String, source: String): borrow String from source = source\nfun run() { val source = \"kept\"; val other = \"other\"; borrow val item = select(source = source, other = other); consume(source); println(item) }",
            rooted,
        );
        {
            let diagnostics = unit.diagnostics();
            assert!(
                diagnostics.iter().any(|d| d.code().to_string() == "L0135"),
                "{diagnostics:?}"
            );
        }
    }
}

#[test]
fn abandoned_initializer_ends_source_call_without_ending_an_unestablished_result() {
    for rooted in [false, true] {
        use lang_frontend::ownership_checking::{LoanKind, UnitDropPoint};
        let (sources, unit) = checked_entry(
            "fun select(source: String, other: String): borrow String from source = source\nfun run(flag: Boolean) { val source = \"kept\"; borrow val item = select(source, if (flag) { return } else { \"other\" }); println(item) }",
            rooted,
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.deferred().is_empty());
        let [binding] = unit.borrow_results().bindings() else {
            panic!("one binding on the continuing branch")
        };
        let source = binding.source_loan().unwrap();
        let [event] = unit.borrow_call_prefix_ends() else {
            panic!("one abandoned initializer source")
        };
        assert_eq!(event.loan().call(), source.call());
        assert_eq!(event.loan().argument(), source.argument());
        assert_eq!(event.loan().kind(), LoanKind::Shared);
        assert!(unit.loans().contains(event.loan()));
        assert_eq!(sources.slice(event.loan().begin_span()), Ok("source"));
        assert_eq!(
            sources.slice(event.loan().end_span()),
            Ok("select(source, if (flag) { return } else { \"other\" })")
        );
        assert_eq!(
            sources.slice(event.loan().parameter_span().unwrap()),
            Ok("source: String")
        );
        let UnitDropPoint::ControlTransfer(transfer) = event.point() else {
            panic!("actual abandoned-source control transfer")
        };
        assert_eq!(transfer.source_unit(), source.call().source_unit());
        assert_eq!(event.loan().target(), binding.origin());
        unit.clone().validate().unwrap();
        assert_eq!(unit.borrow_results().ends().len(), 1);
        assert!(matches!(
            unit.borrow_results().ends()[0].point(),
            UnitDropPoint::AfterStatement(_)
        ));
    }
}
