//! Result fact transport is immutable and stays empty until the complete permission producer exists.
use lang_frontend::{
    ast::ExpressionId,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SymbolId, UnitSymbolId, index_compilation_unit,
        resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        BorrowResultFacts, DropPoint, LoanTarget, UnitDropPoint, UnitLoanTarget,
        check_compilation_unit_ownership, check_ownership,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        UnitExpressionId, check_compilation_unit_types, check_types, standard_environments,
    },
};

#[test]
fn empty_facts_cannot_claim_a_binding_source_handoff_or_end_edge() {
    let file: BorrowResultFacts<ExpressionId, SymbolId, LoanTarget, DropPoint> = Default::default();
    let unit: BorrowResultFacts<UnitExpressionId, UnitSymbolId, UnitLoanTarget, UnitDropPoint> =
        Default::default();
    assert!(
        file.bindings().is_empty()
            && file.ends().is_empty()
            && file.forwarded_source_loans().is_empty()
    );
    assert!(
        unit.bindings().is_empty()
            && unit.ends().is_empty()
            && unit.forwarded_source_loans().is_empty()
    );
    assert_eq!(file, file.clone());
    assert_eq!(unit, unit.clone());
}

#[test]
fn owned_results_and_synchronous_borrow_loans_do_not_acquire_result_continuation() {
    for text in [
        "fun observe(item: String) {}\nfun run() { val source = \"kept\"; observe(source) }",
        "fun make(): String = \"owned\"\nfun consume(own source: String) {}\nfun run() { consume(make()) }",
        "fun <T> identity(own item: T): T = item\nfun run() { val source = identity(\"owned\"); println(source) }",
    ] {
        let mut sources = SourceMap::new();
        let source = sources.add_source("facts.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (ne, te) = standard_environments();
        let names = resolve_names(&sources, &parsed, &ne).unwrap();
        assert!(names.diagnostics().is_empty());
        let typed = check_types(&sources, &parsed, &names, &te).unwrap();
        assert!(
            typed.diagnostics().is_empty(),
            "{text}: {:?}",
            typed.diagnostics()
        );
        let file = check_ownership(&sources, &parsed, &names, &typed).unwrap();
        assert!(file.diagnostics().is_empty());
        let facts: &BorrowResultFacts<ExpressionId, SymbolId, LoanTarget, DropPoint> =
            file.borrow_results();
        assert!(
            facts.bindings().is_empty()
                && facts.ends().is_empty()
                && facts.forwarded_source_loans().is_empty()
        );
        let inputs = [SourceUnitInput::new("root", "facts.ko", source, &parsed)];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &te)
            .unwrap()
            .validate()
            .unwrap();
        let unit =
            check_compilation_unit_ownership(&sources, &inputs, &names, &te, &typed).unwrap();
        assert!(unit.diagnostics().is_empty());
        let facts: &BorrowResultFacts<
            UnitExpressionId,
            UnitSymbolId,
            UnitLoanTarget,
            UnitDropPoint,
        > = unit.borrow_results();
        assert!(
            facts.bindings().is_empty()
                && facts.ends().is_empty()
                && facts.forwarded_source_loans().is_empty()
        );
        assert!(unit.deferred().is_empty());
        assert!(unit.clone().validate().is_ok());
        if text.contains("observe(source)") {
            assert_eq!(file.loans().len(), 1);
            assert_eq!(unit.loans().len(), 1);
            assert_eq!(file.loan_ends().len(), 1);
            assert_eq!(unit.loans()[0].end_span().source_id(), source);
        }
    }
}

#[test]
fn origin_facts_do_not_mint_caller_continuation_or_validated_type_capability() {
    let text = "fun view(source: String): borrow String from source = source";
    let mut sources = SourceMap::new();
    let source = sources.add_source("facts.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    let (ne, te) = standard_environments();
    let names = resolve_names(&sources, &parsed, &ne).unwrap();
    let typed = check_types(&sources, &parsed, &names, &te).unwrap();
    assert_eq!(typed.diagnostics().len(), 1);
    assert_eq!(typed.diagnostics()[0].code().to_string(), "L0164");
    let file = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(file.diagnostics().is_empty());
    assert_eq!(file.borrow_return_origins().len(), 1);
    assert!(file.borrow_results().bindings().is_empty());
    assert!(file.borrow_results().ends().is_empty());
    assert!(file.borrow_results().forwarded_source_loans().is_empty());
    let inputs = [SourceUnitInput::new("root", "facts.ko", source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &te).unwrap();
    assert!(typed.validate().is_err());
}
