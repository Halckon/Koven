//! 普通借用的 caller continuation 与 scope end；single/unit 同一源码验证。
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, OwnershipCheckedFile, check_compilation_unit_ownership,
        check_ownership,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

fn checked(body: &str) -> (SourceMap, OwnershipCheckedFile, CompilationUnitOwnership) {
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
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let single = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    let inputs = [SourceUnitInput::new(
        "root",
        "continuation.ko",
        source,
        &parsed,
    )];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let unit = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    (sources, single, unit)
}

#[test]
fn nested_reborrow_and_alias_scopes_restore_owner_permission() {
    for body in [
        "fun run() { val source = \"kept\"; { borrow val item = wrap(source); println(item) }; consume(source) }",
        "fun run() { val source = \"kept\"; { borrow val item = view(source); { borrow val alias = item; println(alias) }; println(item) }; consume(source) }",
        "fun run() { val source = \"kept\"; { borrow val item = view(source); { borrow val child = view(item); println(child) }; println(item) }; consume(source) }",
        "fun run(flag: Boolean) { val source = \"kept\"; if (flag) { borrow val item = view(source); println(item) } else { borrow val item = source; println(item) }; consume(source) }",
    ] {
        let (_, single, unit) = checked(body);
        assert!(
            single.diagnostics().is_empty(),
            "{body}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics().is_empty(),
            "{body}: {:?}",
            unit.diagnostics()
        );
        assert!(single.deferred().is_empty());
        assert!(unit.deferred().is_empty());
    }
}

#[test]
fn live_results_and_children_block_source_move_on_every_branch() {
    for body in [
        "fun run() { val source = \"kept\"; borrow val item = view(source); consume(source); println(item) }",
        "fun run() { val source = \"kept\"; borrow val item = view(source); { borrow val child = view(item); consume(source); println(child) }; println(item) }",
        "fun run(flag: Boolean) { val source = \"kept\"; borrow val item = view(source); if (flag) { consume(source) }; println(item) }",
        "fun run() { val source = \"kept\"; borrow val item = source; { borrow val alias = item; println(alias) }; consume(source); println(item) }",
    ] {
        let (_, single, unit) = checked(body);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            assert!(
                diagnostics.iter().any(|d| d.code().to_string() == "L0135"),
                "{body}: {diagnostics:?}"
            );
        }
    }
}

#[test]
fn results_continue_original_source_loans_and_never_own_the_payload() {
    use lang_frontend::ownership_checking::{
        DropTarget, LoanEndPoint, LoanTarget, UnitDropTarget, UnitLoanTarget,
    };
    let (_, single, unit) = checked(
        "fun run() { val source = \"kept\"; { borrow val item = wrap(source); println(item) } }",
    );
    assert!(single.diagnostics().is_empty());
    assert!(unit.diagnostics().is_empty());
    let binding = &single.borrow_results().bindings()[0];
    let source = binding.source_loan().unwrap();
    let LoanTarget::Place(origin) = binding.origin() else {
        panic!("stable source");
    };
    let source_loan = single
        .loans()
        .iter()
        .find(|loan| loan.call() == source.call() && loan.argument() == source.argument())
        .unwrap();
    assert_eq!(source_loan.target(), binding.origin());
    assert!(
        !single
            .loan_ends()
            .iter()
            .any(|end| end.call() == source.call()
                && end.argument() == source.argument()
                && end.point() == LoanEndPoint::CallReturn(source.call()))
    );
    let ends = single.borrow_results().ends();
    assert_eq!(ends.len(), 1);
    assert_eq!(ends[0].binding(), binding.binding());
    let drops = single
        .drops()
        .iter()
        .filter(|drop| drop.target() == DropTarget::Named(origin.root()))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].point(), ends[0].point());
    assert!(
        !single
            .drops()
            .iter()
            .any(|drop| drop.target() == DropTarget::Named(binding.binding())
                || drop.target() == DropTarget::Temporary(binding.initializer()))
    );
    let binding = &unit.borrow_results().bindings()[0];
    let source = binding.source_loan().unwrap();
    let UnitLoanTarget::Place(origin) = binding.origin() else {
        panic!("stable unit source");
    };
    let source_loan = unit
        .loans()
        .iter()
        .find(|loan| loan.call() == source.call() && loan.argument() == source.argument())
        .unwrap();
    assert_eq!(source_loan.target(), binding.origin());
    let ends = unit.borrow_results().ends();
    assert_eq!(ends.len(), 1);
    assert_eq!(ends[0].binding(), binding.binding());
    let drops = unit
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Named(origin.root()))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].point(), ends[0].point());
    assert!(!unit.drops().iter().any(|drop| drop.target()
        == UnitDropTarget::Named(binding.binding())
        || drop.target() == UnitDropTarget::Temporary(binding.initializer())));
    for forwarded in single.borrow_results().forwarded_source_loans() {
        assert!(!single.loan_ends().iter().any(|end| end.call() == forwarded.call() && end.argument() == forwarded.argument()));
    }
    assert_eq!(single.borrow_results().forwarded_source_loans().len(), 1);
    assert_eq!(unit.borrow_results().forwarded_source_loans().len(), 1);
}

#[test]
fn children_end_before_parents_and_owner_cleanup_on_scope_and_return_edges() {
    use lang_frontend::ownership_checking::{
        DropTarget, LoanTarget, UnitDropTarget, UnitLoanTarget,
    };
    for exit in ["", "return"] {
        let (_, single, unit) = checked(&format!(
            "fun run() {{ val source = \"kept\"; borrow val parent = view(source); borrow val child = view(parent); println(child); {exit} }}"
        ));
        assert!(
            single.diagnostics().is_empty(),
            "{:?}",
            single.diagnostics()
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        let bindings = single.borrow_results().bindings();
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[1].parent(), Some(bindings[0].binding()));
        assert_eq!(bindings[1].origin(), bindings[0].origin());
        let ends = single.borrow_results().ends();
        assert_eq!(
            ends.iter().map(|end| end.binding()).collect::<Vec<_>>(),
            vec![bindings[1].binding(), bindings[0].binding()]
        );
        let LoanTarget::Place(origin) = bindings[0].origin() else {
            panic!("stable source");
        };
        assert!(
            single
                .drops()
                .iter()
                .any(|drop| drop.target() == DropTarget::Named(origin.root())
                    && drop.point() == ends[1].point())
        );
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

#[test]
fn source_mutation_and_exclusive_reborrow_conflicts_keep_real_spans() {
    for body in [
        "fun run() { var source = \"kept\"; borrow val item = view(source); source = \"changed\"; println(item) }",
        "fun mutate(inout source: String) {}\nfun run() { var source = \"kept\"; borrow val item = view(source); mutate(&source); println(item) }",
        "class Record(var text: String)\nfun run() { val source = Record(\"kept\"); borrow val item = source; item.text = \"changed\"; println(item.text) }",
    ] {
        let (sources, single, unit) = checked(body);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            let conflict = diagnostics
                .iter()
                .find(|d| d.code().to_string() == "L0135")
                .unwrap_or_else(|| panic!("{body}: {diagnostics:?}"));
            assert!(!sources.slice(conflict.primary_span()).unwrap().is_empty());
            assert!(!conflict.details().is_empty());
        }
        assert!(single.borrow_results().bindings().is_empty());
        assert!(unit.borrow_results().bindings().is_empty());
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
    let (_, single, unit) = checked(body);
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        assert!(
            diagnostics.iter().any(|d| d.code().to_string() == code),
            "{body}: {diagnostics:?}"
        );
    }
    assert!(single.borrow_results().bindings().is_empty());
    assert!(unit.borrow_results().bindings().is_empty());
    assert!(single.borrow_results().ends().is_empty());
    assert!(unit.borrow_results().ends().is_empty());
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
    for body in [
        "fun nullableView(source: String?): borrow String? from source = source\nfun observe(source: String?) {}\nfun run(source: String?) { borrow val item = nullableView(source); observe(item) }",
        "fun <T> genericView(source: T): borrow T from source = source\nclass Resource { deinit() {} }\nfun run() { val source = Resource(); { borrow val item = genericView(source) } }",
        "fun intView(source: Int): borrow Int from source = source\nfun observe(own item: Int) {}\nfun run(source: Int) { borrow val item = intView(source); observe(item) }",
    ] {
        let (_, single, unit) = checked(body);
        assert!(
            single.diagnostics().is_empty(),
            "{body}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics().is_empty(),
            "{body}: {:?}",
            unit.diagnostics()
        );
        assert_eq!(single.borrow_results().bindings().len(), 1);
        assert_eq!(unit.borrow_results().bindings().len(), 1);
        assert_eq!(single.borrow_results().ends().len(), 1);
        assert_eq!(unit.borrow_results().ends().len(), 1);
    }
}

#[test]
fn named_source_continues_only_its_loan_and_other_arguments_end_at_call_return() {
    use lang_frontend::ownership_checking::LoanEndPoint;
    let (_, single, unit) = checked(
        "fun select(other: String, source: String): borrow String from source = source\nfun run() { val source = \"kept\"; val other = \"other\"; borrow val item = select(source = source, other = other); println(item) }",
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    let source = single.borrow_results().bindings()[0].source_loan().unwrap();
    let ends = single
        .loan_ends()
        .iter()
        .filter(|end| end.call() == source.call())
        .collect::<Vec<_>>();
    assert_eq!(ends.len(), 1);
    assert_ne!(ends[0].argument(), source.argument());
    assert_eq!(ends[0].point(), LoanEndPoint::CallReturn(source.call()));
    assert_eq!(single.borrow_results().ends().len(), 1);
    assert_eq!(unit.borrow_results().ends().len(), 1);
    let (_, single, unit) = checked(
        "fun select(other: String, source: String): borrow String from source = source\nfun run() { val source = \"kept\"; val other = \"other\"; borrow val item = select(source = source, other = other); consume(source); println(item) }",
    );
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        assert!(
            diagnostics.iter().any(|d| d.code().to_string() == "L0135"),
            "{diagnostics:?}"
        );
    }
}

#[test]
fn abandoned_initializer_ends_source_call_without_ending_an_unestablished_result() {
    use lang_frontend::ownership_checking::{DropPoint, LoanEndPoint, UnitDropPoint};
    let (_, single, unit) = checked(
        "fun select(source: String, other: String): borrow String from source = source\nfun run(flag: Boolean) { val source = \"kept\"; borrow val item = select(source, if (flag) { return } else { \"other\" }); println(item) }",
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    let source = single.borrow_results().bindings()[0].source_loan().unwrap();
    assert!(
        single
            .loan_ends()
            .iter()
            .any(|end| end.call() == source.call()
                && end.argument() == source.argument()
                && matches!(end.point(), LoanEndPoint::ControlTransfer(_)))
    );
    assert_eq!(single.borrow_results().ends().len(), 1);
    assert!(matches!(
        single.borrow_results().ends()[0].point(),
        DropPoint::AfterStatement(_)
    ));
    assert_eq!(unit.borrow_results().ends().len(), 1);
    assert!(matches!(
        unit.borrow_results().ends()[0].point(),
        UnitDropPoint::AfterStatement(_)
    ));
}
