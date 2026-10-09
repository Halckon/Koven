//! SPEC-0288: Map 参数交付与 receiver 权限的单文件/unit 一致性。
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

fn checked(text: &str) -> (OwnershipCheckedFile, CompilationUnitOwnership) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("map.ko", text).unwrap();
    let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).unwrap();
    let typed = check_types(&sources, &file, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    for descriptor in typed.map_require_values() {
        assert_eq!(
            descriptor.borrow_return().origin(),
            lang_frontend::type_checking::BorrowReturnOrigin::Receiver
        );
        assert_eq!(
            sources
                .slice(descriptor.borrow_return().marker_span())
                .unwrap(),
            "requireValue"
        );
    }
    let single = check_ownership(&sources, &file, &names, &typed).unwrap();
    let inputs = [SourceUnitInput::new("root", "map.ko", source, &file)];
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
    (single, unit)
}

#[test]
fn map_require_value_keeps_receiver_loan_and_restores_mutation_at_scope_end() {
    for value in ["String", "Packet", "Resource", "Token?"] {
        let text = format!(
            "value class Packet(val text: String)\nclass Resource {{ deinit() {{}} }}\nclass Token(val n: Int)\nfun observe(item: {value}) {{}}\nfun consume(own m: MutableMap<String, {value}>) {{}}\nfun run() {{ var m = mutableMapOf<String, {value}>(); val key = \"key\"; {{ borrow val item = m.requireValue(key); observe(item); println(key) }}; consume(m) }}"
        );
        let (single, unit) = checked(&text);
        assert!(
            single.diagnostics().is_empty(),
            "{text}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics().is_empty(),
            "{text}: {:?}",
            unit.diagnostics()
        );
        assert_eq!(single.borrow_results().bindings().len(), 1);
        assert_eq!(unit.borrow_results().bindings().len(), 1);
        let source = single.borrow_results().bindings()[0].source_loan().unwrap();
        assert!(
            single
                .loans()
                .iter()
                .any(|loan| loan.call() == source.call() && loan.argument() == source.argument())
        );
    }
}

#[test]
fn map_require_value_rejects_invalidation_and_owned_result_delivery() {
    for (body, code) in [
        (
            "borrow val item = m.requireValue(key); m.put(\"other\", \"new\"); println(item)",
            "L0135",
        ),
        (
            "borrow val item = m.requireValue(key); m.remove(key); println(item)",
            "L0135",
        ),
        (
            "borrow val item = m.requireValue(key); consume(m); println(item)",
            "L0135",
        ),
        ("val item = m.requireValue(key); println(item)", "L0163"),
    ] {
        let text = format!(
            "fun consume(own m: MutableMap<String, String>) {{}}\nfun run() {{ var m = mutableMapOf<String, String>(); val key = \"key\"; {body} }}"
        );
        let (single, unit) = checked(&text);
        assert!(
            single
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == code),
            "{text}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics()
                .iter()
                .any(|d| d.code().to_string() == code),
            "{text}: {:?}",
            unit.diagnostics()
        );
        assert!(single.borrow_results().bindings().is_empty());
        assert!(unit.borrow_results().bindings().is_empty());
    }
}

#[test]
fn readonly_map_require_value_publishes_the_same_receiver_contract() {
    let (single, unit) = checked(
        "fun run(m: Map<String, String>, key: String) { borrow val item = m.requireValue(key); println(item); println(key) }",
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    assert_eq!(single.borrow_results().bindings().len(), 1);
    assert_eq!(unit.borrow_results().bindings().len(), 1);
}
