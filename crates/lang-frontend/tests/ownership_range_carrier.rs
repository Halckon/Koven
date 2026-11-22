//! 借用已有 carrier metadata 的真实来源和依赖；不构造新的范围描述符。
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{CompilationUnitOwnership, check_compilation_unit_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

// Primary single-document consumer uses source-qualified Unit facts.
fn checked(
    text: &str,
) -> (
    SourceMap,
    CompilationUnitOwnership,
    CompilationUnitOwnership,
) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("range-loans.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let names = names.validate().unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let typed = typed.validate().unwrap();
    let single =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
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
