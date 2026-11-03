//! SPEC-0288: Map 参数交付与 receiver 权限的双入口 Unit 合同。
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{CompilationUnitOwnership, check_compilation_unit_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

// Negative consumers retain their original bytes and both Unit entry modes.
// Successful source-loan/end contracts live in loan_end_unit below.
fn checked_unit(text: &str) -> (CompilationUnitOwnership, CompilationUnitOwnership) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("map.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let file = parse_file(&sources, &lexed).unwrap();
    assert!(file.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let [memory, rooted] = [
        SourceUnitInput::memory_document(source, &file),
        SourceUnitInput::new("root", "map.ko", source, &file),
    ]
    .map(|input| {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let names = names.validate().unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(
            typed.map_require_values().len(),
            1,
            "one original Map operation"
        );
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
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(complete.is_same_analysis(&typed));
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(owned.is_compatible_with(&typed));
        owned
    });
    (memory, rooted)
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
        let (memory, rooted) = checked_unit(&text);
        assert!(
            memory
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == code),
            "{text}: {:?}",
            memory.diagnostics()
        );
        assert!(
            rooted
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == code),
            "{text}: {:?}",
            rooted.diagnostics()
        );
        assert!(memory.borrow_results().bindings().is_empty());
        assert!(rooted.borrow_results().bindings().is_empty());
    }
}

#[path = "ownership_map_require/loan_end_unit.rs"]
mod loan_end_unit;
