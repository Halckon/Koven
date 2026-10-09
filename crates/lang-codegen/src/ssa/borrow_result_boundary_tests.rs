//! 合法普通借用声明在结果 ABI 接通前必须结构化拒绝；不生成故障 IR。
use super::{LoweringErrorKind, lower_frontend::orchestrate::lower_scalar_file};

#[test]
fn multi_parameter_borrow_return_single_lowering_remains_unsupported() {
    use lang_frontend::{
        lexer::lex,
        name_resolution::resolve_names,
        ownership_checking::check_ownership,
        parser::parse_file,
        source::SourceMap,
        type_checking::{check_types, standard_environments},
    };
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "borrow_result.ko",
            "fun view(source: Int, other: Int): borrow Int from source = source\nfun entry() {}",
        )
        .unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(owned.diagnostics().is_empty());
    assert_eq!(owned.borrow_return_origins().len(), 1);
    let error = lower_scalar_file(&sources, &parsed, &names, &typed, &owned)
        .err()
        .expect("ordinary borrow ABI remains unsupported");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(sources.slice(error.span.unwrap()).unwrap(), "borrow");
}

#[test]
fn multi_parameter_borrow_return_unit_lowering_remains_unsupported() {
    use super::{
        unit_lower::lower_scalar_unit_with_entry,
        unit_lower_test_support::{analyze as unit_analysis, declaration, parsed},
    };
    use lang_frontend::{
        name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
    };
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "entry.ko",
        "package app\nfun view(source: Int, other: Int): borrow Int from source = source\nfun entry() {}",
    );
    let inputs = [SourceUnitInput::new("root", "app/entry.ko", source, &file)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) = unit_analysis(&sources, &inputs, &environment, &types);
    let error = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        declaration(&names, "app", "entry"),
    )
    .err()
    .expect("ordinary borrow ABI remains unsupported");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(sources.slice(error.span.unwrap()).unwrap(), "borrow");
}

#[test]
fn stable_place_root_single_lowering_preserves_real_storage() {
    use lang_frontend::{
        lexer::lex,
        name_resolution::resolve_names,
        ownership_checking::check_ownership,
        parser::parse_file,
        source::SourceMap,
        type_checking::{check_types, standard_environments},
    };
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "stable.ko",
            "fun entry() { val source = \"kept\"; borrow val item = source; println(item) }",
        )
        .unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(owned.diagnostics().is_empty());
    assert_eq!(owned.borrow_results().bindings().len(), 1);
    let program = lower_scalar_file(&sources, &parsed, &names, &typed, &owned).unwrap();
    super::verify::verify_program(&program).unwrap();
    assert!(program.modules[0].functions.iter().any(|f| {
        f.instructions
            .iter()
            .any(|i| matches!(i.operation, super::model::Operation::RootPlace { .. }))
    }));
}

#[test]
fn stable_place_root_unit_lowering_preserves_real_storage() {
    use super::{
        unit_lower::lower_scalar_unit_with_entry,
        unit_lower_test_support::{analyze, declaration, parsed},
    };
    use lang_frontend::{
        name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
    };
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "stable.ko",
        "package app\nfun entry() { val source = \"kept\"; borrow val item = source; println(item) }",
    );
    let inputs = [SourceUnitInput::new("root", "app/stable.ko", source, &file)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &environment, &types);
    assert_eq!(owned.ownership().borrow_results().bindings().len(), 1);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        declaration(&names, "app", "entry"),
    )
    .unwrap();
    super::verify::verify_program(&program).unwrap();
    assert!(program.modules[0].functions.iter().any(|f| {
        f.instructions
            .iter()
            .any(|i| matches!(i.operation, super::model::Operation::RootPlace { .. }))
    }));
}
