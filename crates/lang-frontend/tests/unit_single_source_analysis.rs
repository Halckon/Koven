//! 单个真实内存来源的 Unit strict/recovery、显式环境与身份合同。
//! foreign source/environment tests are CI-only controls; this target is not a local run list.

use std::error::Error;

use lang_frontend::{
    analysis::{
        UnitNameAnalysisError, UnitRecoveryAnalysisError, UnitSourceDescriptor,
        analyze_basic_unit_ownership, analyze_unit_names, analyze_unit_recovery,
    },
    diagnostic::{Diagnostic, DiagnosticCodeCatalog, Severity},
    lexer::{LexerInternalError, lex},
    name_resolution::{NameEnvironment, SourceUnitInputKey},
    ownership_checking::{
        check_compilation_unit_constant_ownership, check_compilation_unit_ownership,
        const_owned_compilation_unit_view, owned_compilation_unit_view,
    },
    source::SourceMap,
    type_checking::{
        CompilationUnitTypeError, TypeEnvironment, check_compilation_unit_types,
        complete_compilation_unit_types, standard_environments,
    },
};

#[test]
fn empty_and_unicode_crlf_sources_keep_full_strict_facts_and_owned_identity() {
    for text in ["", "fun main(): Unit { val greeting = \"你好🌍\" }\r\n"] {
        let (typed, owned) = {
            let mut sources = SourceMap::new();
            let excluded = sources
                .add_source("excluded.ko", "# ignored subset")
                .unwrap();
            let source = sources.add_source("memory/原始.ko", text).unwrap();
            let (environment, types) = standard_environments();
            let snapshot = analyze_unit_names(
                sources,
                vec![UnitSourceDescriptor::memory_document(source)],
                environment,
            )
            .unwrap();
            assert_ne!(source, excluded);
            assert_eq!(snapshot.sources().source_text(source), Ok(text));
            let names = snapshot
                .validated_names()
                .expect("healthy single source names");
            let inputs = snapshot.inputs();
            assert_eq!(inputs.len(), 1);
            assert_eq!(inputs[0].source_id(), source);
            assert_eq!(inputs[0].key(), SourceUnitInputKey::MemoryDocument);
            assert!(std::ptr::eq(
                inputs[0].parsed(),
                &snapshot.parsed_files()[0]
            ));
            let raw =
                check_compilation_unit_types(snapshot.sources(), &inputs, names, &types).unwrap();
            let complete =
                complete_compilation_unit_types(snapshot.sources(), &inputs, names, &types, &raw)
                    .expect("healthy final qualification");
            assert!(complete.is_same_analysis(&raw));
            let (typed, owned) =
                analyze_basic_unit_ownership(snapshot.sources(), &inputs, names, &types, raw)
                    .unwrap()
                    .into_result()
                    .expect("healthy basic path");
            let rechecked = check_compilation_unit_ownership(
                snapshot.sources(),
                &inputs,
                names,
                &types,
                &typed,
            )
            .unwrap();
            assert_eq!(owned, rechecked, "complete actual strict ownership facts");
            assert!(!owned.is_same_analysis(&rechecked));
            let (fresh, recovered) = analyze_unit_recovery(&snapshot, &types).unwrap();
            assert_eq!(&fresh, typed.types(), "complete typed recovery facts");
            assert_eq!(owned, recovered, "complete owned recovery facts");
            assert!(!fresh.is_same_analysis(typed.types()));
            assert!(owned.is_compatible_with(&typed.clone()));
            assert!(!owned.is_compatible_with(&fresh.validate().unwrap()));
            assert!(typed.types().is_compatible_with(
                snapshot.sources(),
                &inputs,
                &names.clone(),
                &types,
            ));
            let qualified = owned.clone().validate().expect("healthy ownership");
            let view = owned_compilation_unit_view(
                snapshot.sources(),
                &inputs,
                names,
                &types,
                &typed,
                &qualified,
            )
            .unwrap();
            assert_eq!(view.inputs()[0].source_id(), source);
            assert!(std::ptr::eq(view.types(), typed.types()));
            assert!(std::ptr::eq(view.ownership(), qualified.ownership()));
            (typed, owned)
        };
        // These are the actual moved products, not cloned stand-ins for expired inputs.
        assert!(owned.is_compatible_with(&typed));
        owned.validate().expect("owned result outlives input scope");
    }
}

#[test]
fn a_real_single_source_const_uses_only_the_distinct_const_ownership_chain() {
    let mut sources = SourceMap::new();
    sources
        .add_source("excluded.ko", "# ignored subset")
        .unwrap();
    let text = "const val value: Int = 7\nfun main(): Int = value";
    let source = sources.add_source("memory/constant.ko", text).unwrap();
    let (environment, types) = standard_environments();
    let snapshot = analyze_unit_names(
        sources,
        vec![UnitSourceDescriptor::memory_document(source)],
        environment,
    )
    .unwrap();
    let inputs = snapshot.inputs();
    let names = snapshot.validated_names().unwrap();
    let raw = check_compilation_unit_types(snapshot.sources(), &inputs, names, &types).unwrap();
    complete_compilation_unit_types(snapshot.sources(), &inputs, names, &types, &raw)
        .expect("constant final admission");
    let before = raw.clone();
    let raw = analyze_basic_unit_ownership(snapshot.sources(), &inputs, names, &types, raw)
        .unwrap()
        .into_result()
        .expect_err("a real const declaration must not use basic ownership");
    assert_eq!(*raw, before);
    assert!(raw.is_same_analysis(&before));
    let typed = raw.validate_constants().expect("original const capability");
    assert_eq!(typed.constants().declarations().len(), 1);
    assert_eq!(typed.constants().uses().len(), 1);
    let owned = check_compilation_unit_constant_ownership(
        snapshot.sources(),
        &inputs,
        names,
        &types,
        &typed,
    )
    .unwrap();
    assert!(owned.is_compatible_with(&typed));
    assert!(owned.ownership().diagnostics().is_empty());
    assert!(owned.ownership().deferred().is_empty());
    assert!(owned.ownership().clone().validate().is_err());
    let owned = owned.validate().expect("real const ownership");
    let view = const_owned_compilation_unit_view(
        snapshot.sources(),
        &inputs,
        names,
        &types,
        &typed,
        &owned,
    )
    .unwrap();
    assert_eq!(view.inputs()[0].source_id(), source);
    assert_eq!(view.sources().source_text(source), Ok(text));
    assert!(std::ptr::eq(view.types(), typed.types()));
    assert!(std::ptr::eq(view.constant_ownership(), &owned));
}

#[test]
fn explicit_empty_environment_keeps_exact_l0080_and_cannot_grant_capability() {
    let mut sources = SourceMap::new();
    let text = "fun main(): Unit {}";
    let source = sources.add_source("explicit.ko", text).unwrap();
    let environment = NameEnvironment::new();
    let before = environment.clone();
    let types = TypeEnvironment::new(&environment);
    let catalog = DiagnosticCodeCatalog::try_new(&["L0080"]).unwrap();
    let start = text.find("Unit").unwrap();
    let expected = Diagnostic::new(
        &sources,
        Severity::Error,
        catalog.resolve("L0080").unwrap(),
        "unresolved name",
        sources.span(source, start, start + "Unit".len()).unwrap(),
    )
    .unwrap();
    let snapshot = analyze_unit_names(
        sources,
        vec![UnitSourceDescriptor::memory_document(source)],
        environment,
    )
    .unwrap();
    assert_eq!(snapshot.environment(), &before);
    assert_eq!(snapshot.names().diagnostics(), &[expected]);
    assert!(snapshot.validated_names().is_none());
    let (typed, owned) = analyze_unit_recovery(&snapshot, &types).unwrap();
    assert!(typed.clone().validate().is_err());
    assert!(typed.validate_constants().is_err());
    assert!(owned.validate().is_err());
    assert_eq!(snapshot.environment(), &before);
    assert_eq!(snapshot.sources().source_text(source), Ok(text));
}

#[test]
fn ci_only_foreign_environment_keeps_the_exact_internal_error_chain() {
    let mut sources = SourceMap::new();
    let source = sources.add_source("env.ko", "fun main(): Unit {}").unwrap();
    let (environment, _) = standard_environments();
    let (_, foreign) = standard_environments();
    let snapshot = analyze_unit_names(
        sources,
        vec![UnitSourceDescriptor::memory_document(source)],
        environment,
    )
    .unwrap();
    assert!(snapshot.names().diagnostics().is_empty());
    assert!(snapshot.validated_names().is_some());
    let error = analyze_unit_recovery(&snapshot, &foreign).unwrap_err();
    assert!(matches!(
        error,
        UnitRecoveryAnalysisError::Type(CompilationUnitTypeError::MismatchedNameEnvironment)
    ));
    let expected = CompilationUnitTypeError::MismatchedNameEnvironment;
    assert_eq!(error.source().unwrap().to_string(), expected.to_string());
    assert_eq!(
        error.to_string(),
        format!("unit type analysis failed internally: {expected}")
    );
}

#[test]
fn ci_only_foreign_source_fails_before_building_a_unit_snapshot() {
    let sources = SourceMap::new();
    let mut foreign = SourceMap::new();
    let source = foreign.add_source("foreign.ko", "").unwrap();
    let expected = lex(&sources, source).unwrap_err();
    let (environment, _) = standard_environments();
    let error = match analyze_unit_names(
        sources,
        vec![UnitSourceDescriptor::memory_document(source)],
        environment,
    ) {
        Err(error) => error,
        Ok(_) => panic!("foreign source must not produce a snapshot"),
    };
    assert!(matches!(
        error,
        UnitNameAnalysisError::Lexer(LexerInternalError::Source(_))
    ));
    assert_eq!(error.source().unwrap().to_string(), expected.to_string());
    assert_eq!(error.to_string(), expected.to_string());
}
