//! 借用已有 carrier metadata 的真实来源和依赖；不构造新的范围描述符。
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, LoanEndPoint, OwnershipCheckedFile,
        check_compilation_unit_ownership, check_ownership,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

fn checked(text: &str) -> (SourceMap, OwnershipCheckedFile, CompilationUnitOwnership) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("range-loans.ko", text).unwrap();
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
        "range-loans.ko",
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
fn existing_metadata_alias_keeps_parent_and_original_call_loan_until_its_end() {
    for element in ["Int", "String", "Item"] {
        let text = format!(
            "class Item(val value: Int) {{}}\nfun alias(source: View<{element}>): borrow View<{element}> from source = source\nfun observe(source: View<{element}>): Unit {{}}\nfun run(source: View<{element}>): Unit {{ borrow val parent = alias(source); borrow val child = alias(parent); observe(child); observe(parent) }}"
        );
        let (_, single, unit) = checked(&text);
        assert!(
            single.diagnostics().is_empty(),
            "{:?}",
            single.diagnostics()
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(single.deferred().is_empty());
        assert!(unit.deferred().is_empty());
        let [parent, child] = single.borrow_results().bindings() else {
            panic!("two metadata aliases")
        };
        assert!(parent.parent().is_none());
        assert_eq!(child.parent(), Some(parent.binding()));
        assert_eq!(parent.origin(), child.origin());
        for binding in [parent, child] {
            let continuation = binding.source_loan().unwrap();
            let loan = single
                .loans()
                .iter()
                .find(|loan| {
                    loan.call() == continuation.call() && loan.argument() == continuation.argument()
                })
                .unwrap();
            assert_eq!(loan.target(), binding.origin());
            assert!(
                !single
                    .loan_ends()
                    .iter()
                    .any(|end| end.call() == continuation.call()
                        && end.argument() == continuation.argument()
                        && end.point() == LoanEndPoint::CallReturn(continuation.call()))
            );
        }
        assert_eq!(
            single
                .borrow_results()
                .ends()
                .iter()
                .map(|e| e.binding())
                .collect::<Vec<_>>(),
            [child.binding(), parent.binding()]
        );
        assert!(single.drops().is_empty(), "Borrow metadata owns no payload");
        let [parent, child] = unit.borrow_results().bindings() else {
            panic!("two unit metadata aliases")
        };
        assert!(parent.parent().is_none());
        assert_eq!(child.parent(), Some(parent.binding()));
        assert_eq!(parent.origin(), child.origin());
        for binding in [parent, child] {
            let continuation = binding.source_loan().unwrap();
            let loan = unit
                .loans()
                .iter()
                .find(|loan| {
                    loan.call() == continuation.call() && loan.argument() == continuation.argument()
                })
                .unwrap();
            assert_eq!(loan.target(), binding.origin());
        }
        assert_eq!(
            unit.borrow_results()
                .ends()
                .iter()
                .map(|e| e.binding())
                .collect::<Vec<_>>(),
            [child.binding(), parent.binding()]
        );
        assert!(unit.drops().is_empty());
    }
}

#[test]
fn metadata_return_cannot_change_the_declared_source_to_a_sibling() {
    let (sources, single, unit) = checked(
        "fun reject(source: View<String>, sibling: View<String>): borrow View<String> from source = sibling",
    );
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        let diagnostic = diagnostics
            .iter()
            .find(|d| d.code().to_string() == "L0162")
            .unwrap();
        assert_eq!(sources.slice(diagnostic.primary_span()).unwrap(), "sibling");
    }
    assert!(single.borrow_return_origins().is_empty());
    assert!(unit.borrow_return_origins().is_empty());
}
