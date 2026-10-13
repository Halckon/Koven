//! SPEC-0306 boundaries use ordinary source only; no malformed SSA or mutation.
use super::{
    LoweringErrorKind, borrow_result_contract_tests::single, borrow_storage_contract_tests::unit,
};
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{check_compilation_unit_ownership, check_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

fn assert_frontend_rejection(text: &str, code: &str, primary: &str) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("mixed-boundary.ko", text).unwrap();
    let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &file, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &file, &names, &typed).unwrap();
    assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
    assert_eq!(owned.diagnostics()[0].code().to_string(), code);
    assert_eq!(
        sources
            .slice(owned.diagnostics()[0].primary_span())
            .unwrap(),
        primary
    );
    assert_eq!(
        super::lower_frontend::orchestrate::lower_scalar_file(
            &sources, &file, &names, &typed, &owned
        )
        .err()
        .unwrap()
        .kind,
        LoweringErrorKind::FrontendDiagnostics
    );

    // Exercise the real source-qualified checker before validation can discard its diagnostic.
    let (provider, entry) = text.split_once("fun entry()").unwrap();
    let mut sources = SourceMap::new();
    let (p, provider) = super::unit_lower_test_support::parsed(
        &mut sources,
        "provider.ko",
        &format!("package p\n{provider}"),
    );
    let (q, entry) = super::unit_lower_test_support::parsed(
        &mut sources,
        "entry.ko",
        &format!("package q\nimport p.*\nfun entry(){entry}"),
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", p, &provider),
        SourceUnitInput::new("root", "q/entry.ko", q, &entry),
    ];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
    assert_eq!(owned.diagnostics()[0].code().to_string(), code);
    assert_eq!(
        sources
            .slice(owned.diagnostics()[0].primary_span())
            .unwrap(),
        primary
    );
    assert!(
        owned.validate().is_err(),
        "invalid source cannot obtain a validated ownership unit"
    );
}

#[test]
fn mixed_value_borrow_wrong_owner_overlap_and_consumed_value_rejected_at_frontend() {
    for (source, code, primary) in [
        (
            include_str!("../../tests/fixtures/mixed_borrow/wrong_origin_owned.ko"),
            "L0162",
            "other",
        ),
        (
            include_str!("../../tests/fixtures/mixed_borrow/source_live_move.ko"),
            "L0135",
            "source",
        ),
        (
            include_str!("../../tests/fixtures/mixed_borrow/owned_aux_reuse.ko"),
            "L0131",
            "other",
        ),
        (
            include_str!("../../tests/fixtures/mixed_borrow/overlap_owned_after_borrow.ko"),
            "L0135",
            "source",
        ),
        (
            include_str!("../../tests/fixtures/mixed_borrow/overlap_owned_before_borrow.ko"),
            "L0131",
            "source",
        ),
    ] {
        assert_frontend_rejection(source, code, primary);
    }
}

#[test]
fn mixed_value_borrow_inout_source_remains_frontend_deferred() {
    assert_frontend_rejection(
        include_str!("../../tests/fixtures/mixed_borrow/inout_result_source_boundary.ko"),
        "L0164",
        "borrow",
    );
}

#[test]
fn mixed_value_borrow_non_source_inout_remains_lowering_unsupported() {
    for source in [
        include_str!("../../tests/fixtures/mixed_borrow/non_source_inout.ko"),
        include_str!("../../tests/fixtures/mixed_borrow/non_source_inout_writeback.ko"),
    ] {
        for lowered in [single(source), unit(source)] {
            assert_eq!(
                lowered.err().unwrap().kind,
                LoweringErrorKind::UnsupportedNode
            );
        }
    }
}

#[test]
fn mixed_value_borrow_nested_result_binding_remains_frontend_deferred() {
    let source = "fun view(source: String, own other: Int): borrow String from source = source\nfun entry() { val source = \"kept\".clone(); borrow val item = view(view(source, 7), 8); println(item) }";
    assert_frontend_rejection(source, "L0164", "view(source, 7)");
}

#[test]
fn mixed_value_borrow_callable_value_remains_lowering_unsupported() {
    let source = "fun <T> view(source: String, own other: T): borrow String from source = source\nfun <T> inspect(source: String, own other: T) { borrow val item = view(source, other); println(item) }\nfun entry() { val source = \"kept\".clone(); val action: () -> Unit = { println(\"called\") }; inspect(source, action) }";
    for lowered in [single(source), unit(source)] {
        assert_eq!(
            lowered.err().unwrap().kind,
            LoweringErrorKind::UnsupportedNode
        );
    }
}

#[test]
fn mixed_value_borrow_wrapper_control_prefix_remains_lowering_unsupported() {
    for argument in ["-n", "if (true) { 1 } else { 2 }", "n + 1"] {
        let source = format!(
            "fun view(own other: Int, source: String): borrow String from source = source\nfun wrap(source: String, n: Int): borrow String from source = view({argument}, source)\nfun entry() {{ val source = \"kept\"; borrow val item = wrap(source, 7); println(item) }}"
        );
        for lowered in [single(&source), unit(&source)] {
            assert_eq!(
                lowered.err().unwrap().kind,
                LoweringErrorKind::UnsupportedNode
            );
        }
    }
}

#[test]
fn mixed_value_borrow_caller_cfg_remains_lowering_unsupported() {
    let source = "fun view(source: String, own other: Int): borrow String from source = source\nfun entry() { val source = \"kept\"; borrow val item = view(source, 7); if (true) { println(item) }; println(item) }";
    for lowered in [single(source), unit(source)] {
        assert_eq!(
            lowered.err().unwrap().kind,
            LoweringErrorKind::UnsupportedNode
        );
    }
}
