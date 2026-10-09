//! producer 必须证明实际新 descriptor 返回，caller 继承根 loan 而非借用 callee 局部。
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        BorrowBindingStorage, CompilationUnitOwnership, OwnershipCheckedFile,
        check_compilation_unit_ownership, check_ownership,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

fn checked(text: &str) -> (SourceMap, OwnershipCheckedFile, CompilationUnitOwnership) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("producer.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, mut types) = standard_environments();
    types.authorize_range_source(&sources, source).unwrap();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let single = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    let inputs = [SourceUnitInput::new("std", "producer.ko", source, &parsed)];
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
fn direct_generic_producer_continues_root_and_releases_before_owner_move() {
    for (element, value) in [("String", "\"kept\""), ("Item", "Item(7)")] {
        let (_, single, unit) = checked(&format!(
            "class Item(val number: Int) {{}}\nfun <T> prefix(source: List<T>, count: Int): View<T> from source {{ return rangeView(source, 0, count) }}\nfun observe(source: View<{element}>): Unit {{}}\nfun consume(own source: List<{element}>): Unit {{}}\nfun run(): Unit {{ val source = listOf({value}); borrow val part = prefix(source, 1); observe(part); consume(source) }}"
        ));
        assert!(
            single.diagnostics().is_empty(),
            "{:?}",
            single.diagnostics()
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        let [binding] = single.borrow_results().bindings() else {
            panic!("single result")
        };
        assert_eq!(binding.storage(), BorrowBindingStorage::NewRangeDescriptor);
        let loan = binding.source_loan().unwrap();
        assert_eq!(
            single
                .loans()
                .iter()
                .find(|l| l.call() == loan.call() && l.argument() == loan.argument())
                .unwrap()
                .target(),
            binding.origin()
        );
        assert_eq!(
            single.borrow_results().ends()[0].binding(),
            binding.binding()
        );
        assert_eq!(single.borrow_results().forwarded_source_loans().len(), 1);
        assert_eq!(single.borrow_results().range_return_origins().len(), 1);
        assert!(
            single.borrow_return_origins().is_empty(),
            "new descriptor must not use borrow-return ABI"
        );
        let [binding] = unit.borrow_results().bindings() else {
            panic!("unit result")
        };
        assert_eq!(binding.storage(), BorrowBindingStorage::NewRangeDescriptor);
        let loan = binding.source_loan().unwrap();
        assert_eq!(
            unit.loans()
                .iter()
                .find(|l| l.call() == loan.call() && l.argument() == loan.argument())
                .unwrap()
                .target(),
            binding.origin()
        );
        assert_eq!(unit.borrow_results().ends()[0].binding(), binding.binding());
        assert_eq!(unit.borrow_results().forwarded_source_loans().len(), 1);
        assert_eq!(unit.borrow_results().range_return_origins().len(), 1);
        assert!(unit.borrow_return_origins().is_empty());
    }
}

#[test]
fn proven_forwarding_is_independent_of_declaration_order_and_keeps_root_live() {
    for run in [
        "observe(part); consume(source)",
        "consume(source); observe(part)",
    ] {
        let (_, single, unit) = checked(&format!(
            "fun forward(source: List<String>): View<String> from source = prefix(source, 1)\nfun observe(source: View<String>): Unit {{}}\nfun consume(own source: List<String>): Unit {{}}\nfun run(): Unit {{ val source = listOf(\"kept\"); borrow val part = forward(source); {run} }}\nfun prefix(source: List<String>, count: Int): View<String> from source = rangeView(source, 0, count)"
        ));
        if run.starts_with("observe") {
            assert!(
                single.diagnostics().is_empty(),
                "{:?}",
                single.diagnostics()
            );
            assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
            assert_eq!(single.borrow_results().forwarded_source_loans().len(), 2);
            assert_eq!(single.borrow_results().range_return_origins().len(), 2);
            assert_eq!(unit.borrow_results().forwarded_source_loans().len(), 2);
            assert_eq!(unit.borrow_results().range_return_origins().len(), 2);
        } else {
            for diagnostics in [single.diagnostics(), unit.diagnostics()] {
                assert!(
                    diagnostics.iter().any(|d| d.code().to_string() == "L0135"),
                    "{diagnostics:?}"
                );
            }
            assert!(single.borrow_results().bindings().is_empty());
            assert!(unit.borrow_results().bindings().is_empty());
        }
    }
}

#[test]
fn producer_cannot_substitute_a_sibling_or_local_root() {
    for text in [
        "fun wrong(source: List<String>, sibling: List<String>): View<String> from source = rangeView(sibling, 0, 0)",
        "fun wrong(source: List<String>): View<String> from source { val local = listOf(\"local\"); return rangeView(local, 0, 0) }",
    ] {
        let (sources, single, unit) = checked(text);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            let d = diagnostics
                .iter()
                .find(|d| d.code().to_string() == "L0162")
                .unwrap_or_else(|| panic!("{diagnostics:?}"));
            assert!(
                sources
                    .slice(d.primary_span())
                    .unwrap()
                    .starts_with("rangeView(")
            );
        }
        assert!(single.borrow_results().forwarded_source_loans().is_empty());
        assert!(unit.borrow_results().forwarded_source_loans().is_empty());
    }
}

#[test]
fn declarations_and_recursive_forwarding_do_not_fabricate_construction_proof() {
    for text in [
        "fun left(source: List<String>): View<String> from source = right(source)\nfun right(source: List<String>): View<String> from source = left(source)",
        "fun metadata(source: View<String>): View<String> from source = source",
    ] {
        let (_, single, unit) = checked(text);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            assert!(
                diagnostics.iter().any(|d| d.code().to_string() == "L0164"),
                "{diagnostics:?}"
            );
        }
        assert!(single.borrow_results().forwarded_source_loans().is_empty());
        assert!(unit.borrow_results().forwarded_source_loans().is_empty());
    }
}

#[test]
fn an_invalid_producer_body_atomically_revokes_caller_and_return_facts() {
    let (_, single, unit) = checked(
        "fun consume(own source: List<String>): Unit {}\nfun inspect(source: View<String>): Unit {}\nfun invalid(source: List<String>): View<String> from source { consume(source); return rangeView(source, 0, 0) }\nfun run(): Unit { val source = listOf(\"kept\"); borrow val part = invalid(source); inspect(part) }",
    );
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        assert!(
            diagnostics.iter().any(|d| d.code().to_string() == "L0133"),
            "{diagnostics:?}"
        );
    }
    assert!(single.borrow_results().bindings().is_empty());
    assert!(single.borrow_results().range_return_origins().is_empty());
    assert!(single.borrow_results().forwarded_source_loans().is_empty());
    assert!(unit.borrow_results().bindings().is_empty());
    assert!(unit.borrow_results().range_return_origins().is_empty());
    assert!(unit.borrow_results().forwarded_source_loans().is_empty());
}

#[test]
fn qualified_unit_forwarding_preserves_origin_across_sources_and_input_order() {
    let mut sources = SourceMap::new();
    let mut parsed = Vec::new();
    for (path, text) in [
        (
            "lib/Producer.ko",
            "package lib\nfun prefix(source: List<String>): View<String> from source = rangeView(source, 0, 1)",
        ),
        (
            "api/Wrapper.ko",
            "package api\nimport lib.prefix\nfun forward(source: List<String>): View<String> from source = prefix(source)",
        ),
        (
            "app/Main.ko",
            "package app\nimport api.forward\nfun inspect(source: View<String>): Unit {}\nfun run(): Unit { val source = listOf(\"kept\"); borrow val part = forward(source); inspect(part) }",
        ),
    ] {
        let source = sources.add_source(path, text).unwrap();
        parsed.push((
            path,
            source,
            parse_file(&sources, &lex(&sources, source).unwrap()).unwrap(),
        ));
    }
    let (environment, mut types) = standard_environments();
    for (_, source, _) in &parsed[..2] {
        types.authorize_range_source(&sources, *source).unwrap();
    }
    for reverse in [false, true] {
        let mut inputs: Vec<_> = parsed
            .iter()
            .map(|(path, source, parsed)| SourceUnitInput::new("root", path, *source, parsed))
            .collect();
        if reverse {
            inputs.reverse();
        }
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        let unit =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert_eq!(unit.borrow_results().range_return_origins().len(), 2);
        let [binding] = unit.borrow_results().bindings() else {
            panic!("caller")
        };
        let source = binding.source_loan().unwrap();
        assert_eq!(
            unit.loans()
                .iter()
                .find(|l| l.call() == source.call() && l.argument() == source.argument())
                .unwrap()
                .target(),
            binding.origin()
        );
        assert_eq!(unit.borrow_results().ends()[0].binding(), binding.binding());
        for fact in unit.borrow_results().range_return_origins() {
            assert_eq!(sources.slice(fact.declaration_span()).unwrap(), "from");
        }
    }
}
