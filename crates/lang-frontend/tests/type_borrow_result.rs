//! Guide v0.42 普通借用的唯一来源签名契约。

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};

fn codes(text: &str) -> Vec<String> {
    let mut sources = SourceMap::new();
    let source = sources.add_source("borrow_result.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (names, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &names).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    check_types(&sources, &parsed, &names, &types)
        .unwrap()
        .diagnostics()
        .iter()
        .map(|d| d.code().to_string())
        .collect()
}

#[test]
fn ordinary_borrow_signatures_accept_non_owning_source_and_nullable_object() {
    for text in [
        "fun view(source: String): borrow String from source = source",
        "fun view(source: String?): borrow String? from source = source",
        "fun view(inout source: String): borrow String from source = source",
        "class Record(val text: String) { fun view(): borrow String from this = this.text }",
    ] {
        assert!(codes(text).is_empty(), "{text}: {:?}", codes(text));
    }
}

#[test]
fn ordinary_borrow_signatures_reject_missing_owned_and_foreign_source() {
    for text in [
        "fun view(own source: String): borrow String from source = source",
        "fun view(source: String): borrow String from missing = source",
        "fun view(source: String): borrow String from this = source",
    ] {
        assert!(
            codes(text).iter().any(|c| c == "L0162"),
            "{text}: {:?}",
            codes(text)
        );
    }
}

#[test]
fn unit_ordinary_borrow_contract_validates_source_and_survives_call_mapping() {
    use lang_frontend::type_checking::{BorrowReturnOrigin, check_compilation_unit_types};
    for (text, valid) in [
        (
            "fun view(aux: Int, source: String): borrow String from source = source\nfun run(source: String) { borrow val item = view(source = source, aux = 0); println(item) }",
            true,
        ),
        (
            "fun view(own source: String): borrow String from source = source",
            false,
        ),
        (
            "fun view(source: String): borrow String from missing = source",
            false,
        ),
    ] {
        let mut sources = SourceMap::new();
        let source = sources.add_source("borrow_result.ko", text).unwrap();
        let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        let (environment, types) = standard_environments();
        let inputs = [SourceUnitInput::new(
            "root",
            "borrow_result.ko",
            source,
            &file,
        )];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        if valid {
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            let call = typed
                .calls()
                .iter()
                .find(|c| c.borrow_return().is_some())
                .unwrap();
            assert_eq!(
                call.borrow_return().unwrap().origin(),
                BorrowReturnOrigin::Parameter(1)
            );
            assert_eq!(call.arguments()[0].parameter_index(), 1);
        } else {
            assert!(
                typed
                    .diagnostics()
                    .iter()
                    .any(|d| d.code().to_string() == "L0162"),
                "{:?}",
                typed.diagnostics()
            );
        }
    }
}
