//! SPEC-0288 普通借用的实际来源、caller continuation 与权限恢复。

use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{CompilationUnitOwnership, check_compilation_unit_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
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
