//! SPEC-0249 普通 owned-unit 工厂的来源顺序与只读借用合同。

use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        OwnedCompilationUnitViewError, check_compilation_unit_ownership,
        owned_compilation_unit_view,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

#[test]
fn owned_view_borrows_the_exact_validated_products() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("app/main.ko", "package app\nfun entry(): Unit {}")
        .expect("source");
    let lexed = lex(&sources, source).expect("lex");
    let parsed = parse_file(&sources, &lexed).expect("parse");
    let inputs = [SourceUnitInput::new("root", "app/main.ko", source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).expect("index");
    let (name_environment, environment) = standard_environments();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .expect("names")
        .validate()
        .expect("validated names");
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .expect("types")
        .validate()
        .expect("validated types");
    let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
        .expect("ownership")
        .validate()
        .expect("validated ownership");
    let view = owned_compilation_unit_view(&sources, &inputs, &names, &environment, &typed, &owned)
        .expect("matching view");
    assert!(std::ptr::eq(view.sources(), &sources));
    assert!(std::ptr::eq(view.inputs(), inputs.as_slice()));
    assert!(std::ptr::eq(view.names(), &names));
    assert!(std::ptr::eq(view.types(), typed.types()));
    assert!(std::ptr::eq(view.ownership(), owned.ownership()));

    let duplicate = [inputs[0], inputs[0]];
    let changed = [SourceUnitInput::new(
        "other",
        "app/main.ko",
        source,
        &parsed,
    )];
    let foreign = SourceMap::new();
    for (map, candidate, expected) in [
        (
            &foreign,
            inputs.as_slice(),
            OwnedCompilationUnitViewError::MismatchedSource,
        ),
        (
            &sources,
            duplicate.as_slice(),
            OwnedCompilationUnitViewError::MismatchedSource,
        ),
        (
            &sources,
            changed.as_slice(),
            OwnedCompilationUnitViewError::MismatchedAnalysis,
        ),
        (
            &sources,
            [].as_slice(),
            OwnedCompilationUnitViewError::MismatchedAnalysis,
        ),
    ] {
        assert_eq!(
            owned_compilation_unit_view(map, candidate, &names, &environment, &typed, &owned)
                .err()
                .expect("mismatch rejected"),
            expected
        );
    }
    assert_eq!(
        OwnedCompilationUnitViewError::MismatchedSource.to_string(),
        "owned compilation-unit view has mismatched source inputs"
    );
    assert_eq!(
        OwnedCompilationUnitViewError::MismatchedAnalysis.to_string(),
        "owned compilation-unit view has mismatched analysis identity"
    );
}
