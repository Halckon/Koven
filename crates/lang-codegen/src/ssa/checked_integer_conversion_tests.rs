//! Ordinary source only: Phase 2 conversion identity is not an implemented backend operation.
use super::{
    LoweringErrorKind,
    lower_frontend::orchestrate::lower_scalar_file,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, resolve_names},
    ownership_checking::check_ownership,
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        check_types, complete_compilation_unit_types, complete_file_types, standard_environments,
    },
};

#[test]
fn single_conversion_identity_stops_before_unimplemented_lowering() {
    for target in ["Int", "Long"] {
        let root = format!("value.to{target}()");
        let text = format!("fun entry(value: Int): {target} = {root}");
        let mut sources = SourceMap::new();
        let source = sources.add_source("conversion.ko", &text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        assert!(lexed.diagnostics().is_empty());
        let file = parse_file(&sources, &lexed).unwrap();
        assert!(file.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        let names = resolve_names(&sources, &file, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let typed = check_types(&sources, &file, &names, &types).unwrap();
        complete_file_types(&sources, &file, &names, &types, &typed).unwrap();
        let owned = check_ownership(&sources, &file, &names, &typed).unwrap();
        assert!(owned.diagnostics().is_empty());
        let error = lower_scalar_file(&sources, &file, &names, &typed, &owned)
            .err()
            .expect("conversion lowering is not implemented");
        println!("single {root}: {error:?}");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
        assert_eq!(sources.slice(error.span.unwrap()).unwrap(), root);
    }
}

#[test]
fn unit_conversion_identity_stops_before_unimplemented_lowering() {
    for target in ["Int", "Long"] {
        let root = format!("value.to{target}()");
        let text = format!("package p\nfun entry(value: Int): {target} = {root}");
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "p/conversion.ko", &text);
        let inputs = [SourceUnitInput::new(
            "root",
            "p/conversion.ko",
            source,
            &file,
        )];
        let (environment, types) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &environment, &types);
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
        let error = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types,
            &typed,
            &owned,
            declaration(&names, "p", "entry"),
        )
        .err()
        .expect("conversion lowering is not implemented");
        println!("unit {root}: {error:?}");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
        assert_eq!(sources.slice(error.span.unwrap()).unwrap(), root);
    }
}
