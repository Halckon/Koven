//! 合法普通借用声明在结果 ABI 接通前必须结构化拒绝；不生成故障 IR。
use super::LoweringErrorKind;

#[test]
fn non_source_inout_borrow_return_single_lowering_remains_unsupported() {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;

    let analysis = UnitSource::analyze(
        "borrow_result.ko",
        "fun view(source: Int, inout other: Int): borrow Int from source = source\nfun entry() {}",
    )
    .unwrap();
    analysis.assert_frontend_clean();
    assert!(analysis.ownership().diagnostics().is_empty());
    assert_eq!(analysis.ownership().borrow_return_origins().len(), 1);
    let marker = analysis.ownership().borrow_return_origins()[0].declaration_span();
    let view = analysis.declaration("view", SymbolKind::Function);
    assert_eq!(
        marker.source_id(),
        analysis.declaration_span(view).source_id()
    );
    let entry = analysis.declaration("entry", SymbolKind::Function);
    assert_eq!(
        analysis
            .sources()
            .slice(analysis.declaration_span(entry))
            .unwrap(),
        "fun entry() {}"
    );
    // The unchanged single source still contains an uncalled view. Unit's whole-source
    // ABI preflight must reject it before entry reachability can discard the declaration.
    let error = analysis
        .lower_entry(entry)
        .err()
        .expect("ordinary borrow ABI remains unsupported");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(marker));
    assert_eq!(
        analysis.sources().slice(error.span.unwrap()).unwrap(),
        "borrow"
    );
}

#[test]
fn non_source_inout_borrow_return_unit_lowering_remains_unsupported() {
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
        "package app\nfun view(source: Int, inout other: Int): borrow Int from source = source\nfun entry() {}",
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
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;

    let analysis = UnitSource::analyze(
        "stable.ko",
        "fun entry() { val source = \"kept\"; borrow val item = source; println(item) }",
    )
    .unwrap();
    analysis.assert_frontend_clean();
    assert!(analysis.ownership().diagnostics().is_empty());
    assert_eq!(analysis.ownership().borrow_results().bindings().len(), 1);
    let declaration = analysis.declaration("entry", SymbolKind::Function);
    let (program, entry) = analysis.lower_entry(declaration).unwrap();
    assert_eq!(
        program
            .module(entry.module())
            .unwrap()
            .function(entry)
            .unwrap()
            .origin,
        super::model::Origin::Source(analysis.declaration_span(declaration))
    );
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
