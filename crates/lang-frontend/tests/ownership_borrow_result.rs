//! SPEC-0288 普通借用的实际来源、caller continuation 与权限恢复。

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

fn checked(text: &str) -> [CompilationUnitOwnership; 2] {
    let mut sources = SourceMap::new();
    let source = sources.add_source("borrow_result.ko", text).unwrap();
    let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let (environment, types) = standard_environments();
    [false, true].map(|rooted| {
        let inputs = [if rooted {
            SourceUnitInput::new("root", "borrow_result.ko", source, &file)
        } else {
            SourceUnitInput::memory_document(source, &file)
        }];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let [indexed] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(indexed.source_id(), source);
        match (rooted, indexed.key()) {
            (false, lang_frontend::name_resolution::SourceUnitKey::MemoryDocument) => {}
            (
                true,
                lang_frontend::name_resolution::SourceUnitKey::Rooted { root, logical_path },
            ) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "borrow_result.ko");
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
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
    })
}

#[test]
fn ordinary_borrow_wrapper_and_local_scope_restore_source_permission() {
    for result in checked(
        "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(source)\nfun consume(own source: String) {}\nfun run() { val source = \"kept\"; { borrow val item = wrap(source); println(item) }; consume(source) }",
    ) {
        assert!(
            result.diagnostics().is_empty(),
            "{:?}",
            result.diagnostics()
        );
        assert!(result.deferred().is_empty(), "{:?}", result.deferred());
    }
}

#[test]
fn ordinary_borrow_return_rejects_actual_foreign_and_temporary_origin() {
    for text in [
        "fun view(source: String, other: String): borrow String from source = other",
        "fun view(source: String): borrow String from source = \"temporary\"",
        "fun view(source: String): borrow String from source { val local = \"local\"; return local }",
    ] {
        for result in checked(text) {
            assert!(
                result
                    .diagnostics()
                    .iter()
                    .any(|d| d.code().to_string() == "L0162"),
                "{text}: {:?}",
                result.diagnostics()
            );
        }
    }
}

#[test]
fn ordinary_borrow_result_prevents_source_move_until_its_use() {
    for result in checked(
        "fun view(source: String): borrow String from source = source\nfun consume(own source: String) {}\nfun run() { val source = \"kept\"; borrow val item = view(source); consume(source); println(item) }",
    ) {
        assert!(
            result
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0135"),
            "{:?}",
            result.diagnostics()
        );
    }
}

#[test]
fn ordinary_val_does_not_implicitly_own_a_borrowed_call_result() {
    for result in checked(
        "fun view(source: Int): borrow Int from source = source\nfun observe(own item: Int) {}\nfun run() { val source = 1; val item = view(source); observe(item) }",
    ) {
        assert!(
            result
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0163"),
            "{:?}",
            result.diagnostics()
        );
    }
}

fn checked_single(text: &str) -> OwnershipCheckedFile {
    let mut sources = SourceMap::new();
    let source = sources.add_source("borrow_result.ko", text).unwrap();
    let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &file, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    check_ownership(&sources, &file, &names, &typed).unwrap()
}

#[test]
fn nullable_conversion_cannot_supply_existing_borrow_result_storage() {
    for text in [
        "fun run() { val n: Int = 7; borrow val item: Int? = n }",
        "fun view(source: Int): borrow Int from source = source\nfun run() { var n: Int = 7; borrow val item: Int? = view(n); n = 8 }",
        "fun wrap(source: Int): borrow Int? from source = source",
        "fun view(aux: Int, source: Int?): borrow Int? from source = source\nfun run() { val n: Int = 7; borrow val item = view(source = (n), aux = 0) }",
        "fun view(source: Int?): borrow Int? from source = source\nfun wrap(source: Int): borrow Int? from source = view(source)",
        "enum class E { A, B }\nfun run() { val n = E.A; borrow val item: E? = n }",
        "fun wrap(source: Nothing?): borrow Int? from source = source",
    ] {
        let file = checked_single(text);
        let units = checked(text);
        for diagnostics in
            std::iter::once(file.diagnostics()).chain(units.iter().map(|unit| unit.diagnostics()))
        {
            let [diagnostic] = diagnostics else {
                panic!("{text}: expected a single diagnostic, got {diagnostics:?}");
            };
            assert_eq!(diagnostic.code().to_string(), "L0162", "{text}");
        }
        assert!(file.borrow_return_origins().is_empty(), "{text}");
        assert!(file.borrow_results().bindings().is_empty(), "{text}");
        assert!(
            file.borrow_results().forwarded_source_loans().is_empty(),
            "{text}"
        );
        for unit in units {
            assert!(unit.borrow_return_origins().is_empty(), "{text}");
            assert!(unit.borrow_results().bindings().is_empty(), "{text}");
            assert!(
                unit.borrow_results().forwarded_source_loans().is_empty(),
                "{text}"
            );
        }
    }
}

#[test]
fn actual_nullable_storage_remains_a_borrow_source_after_narrowing() {
    let text = "fun view(source: Int?): borrow Int? from source = source\nfun run() { val n: Int? = 7; if (n != null) { borrow val item = view((n)); borrow val direct: Int? = (n) } }";
    let file = checked_single(text);
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    assert_eq!(file.borrow_results().bindings().len(), 2);
    for unit in checked(text) {
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert_eq!(unit.borrow_results().bindings().len(), 2);
    }
}

#[test]
fn matching_nullable_borrow_sources_still_forward_and_handoff() {
    let text = "fun view(source: Int?): borrow Int? from source = source\nfun wrap(source: Int?): borrow Int? from source { if (source != null) { return view((source)) }; return source }\nfun observe(value: Int?): Unit {}\nfun run(source: Int?) { observe(wrap((source))) }";
    let file = checked_single(text);
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    assert_eq!(file.borrow_results().forwarded_source_loans().len(), 1);
    assert_eq!(file.borrow_results().call_uses().len(), 1);
    for unit in checked(text) {
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert_eq!(unit.borrow_results().forwarded_source_loans().len(), 1);
        assert_eq!(unit.borrow_results().call_uses().len(), 1);
    }
}

#[test]
fn enum_case_read_keeps_its_declared_nullable_borrow_storage() {
    let text = "enum class E { A, B }\nfun run(source: E?): Unit { when (source) { is E.A -> { borrow val item: E? = (source) }; else -> {} } }";
    let file = checked_single(text);
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    assert_eq!(file.borrow_results().bindings().len(), 1);
    for unit in checked(text) {
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert_eq!(unit.borrow_results().bindings().len(), 1);
    }
}

#[test]
fn synchronous_borrow_continuation_cannot_alias_a_promoted_source() {
    let text = "fun view(source: Int?): borrow Int? from source = source\nfun observe(value: Int?): Unit {}\nfun run() { val n: Int = 7; observe(view(n)) }";
    let file = checked_single(text);
    let units = checked(text);
    for diagnostics in
        std::iter::once(file.diagnostics()).chain(units.iter().map(|unit| unit.diagnostics()))
    {
        let [diagnostic] = diagnostics else {
            panic!("expected an unsupported source handoff, got {diagnostics:?}");
        };
        assert_eq!(diagnostic.code().to_string(), "L0164");
    }
    assert!(file.borrow_results().call_uses().is_empty());
    for unit in units {
        assert!(unit.borrow_results().call_uses().is_empty());
    }
}
