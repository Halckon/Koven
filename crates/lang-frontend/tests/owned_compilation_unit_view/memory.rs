//! MemoryDocument sealed handoff keeps source, analysis and capability identities distinct.
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        NameEnvironment, SourceUnitInput, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::{
        OwnedCompilationUnitViewError, check_compilation_unit_constant_ownership,
        check_compilation_unit_ownership, const_owned_compilation_unit_view,
        owned_compilation_unit_view,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{TypeEnvironment, check_compilation_unit_types, standard_environments},
};

fn with_memory(
    text: &str,
    test: impl FnOnce(
        &SourceMap,
        &[SourceUnitInput<'_>],
        &ValidatedCompilationUnitNames,
        &NameEnvironment,
        &TypeEnvironment,
    ),
) {
    let mut sources = SourceMap::new();
    let unrelated = sources.add_source("unselected", "#").unwrap();
    let source = sources.add_source("untitled:handoff", text).unwrap();
    assert_ne!(source, unrelated);
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let (name_environment, environment) = standard_environments();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .unwrap()
        .validate()
        .unwrap();
    test(&sources, &inputs, &names, &name_environment, &environment);
}

// Only real public producers: a fresh names product has the same index but a distinct owner.
fn assert_source_analysis_gate(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    name_environment: &NameEnvironment,
    environment: &TypeEnvironment,
    view: impl Fn(
        &SourceMap,
        &[SourceUnitInput<'_>],
        &ValidatedCompilationUnitNames,
        &TypeEnvironment,
    ) -> Result<(), OwnedCompilationUnitViewError>,
) {
    let index = index_compilation_unit(sources, inputs).unwrap();
    let fresh_names = resolve_compilation_unit_names(sources, inputs, &index, name_environment)
        .unwrap()
        .validate()
        .unwrap();
    assert_eq!(
        fresh_names, *names,
        "equal values do not replace the original name-analysis owner"
    );
    let (_, fresh_environment) = standard_environments();
    let duplicate = [inputs[0], inputs[0]];
    let rooted = [SourceUnitInput::new(
        "root",
        "source.ko",
        inputs[0].source_id(),
        inputs[0].parsed(),
    )];
    let foreign = SourceMap::new();
    assert_eq!(view(sources, inputs, names, environment), Ok(()));
    assert_eq!(
        view(sources, inputs, &names.clone(), &environment.clone()),
        Ok(()),
        "clones retain producer identity"
    );
    for (map, candidate, candidate_names, candidate_environment, expected) in [
        (
            sources,
            inputs,
            &fresh_names,
            environment,
            OwnedCompilationUnitViewError::MismatchedAnalysis,
        ),
        (
            sources,
            inputs,
            names,
            &fresh_environment,
            OwnedCompilationUnitViewError::MismatchedAnalysis,
        ),
        (
            sources,
            rooted.as_slice(),
            names,
            environment,
            OwnedCompilationUnitViewError::MismatchedAnalysis,
        ),
        (
            sources,
            duplicate.as_slice(),
            names,
            environment,
            OwnedCompilationUnitViewError::MismatchedSource,
        ),
        (
            &foreign,
            inputs,
            names,
            environment,
            OwnedCompilationUnitViewError::MismatchedSource,
        ),
        (
            sources,
            duplicate.as_slice(),
            &fresh_names,
            &fresh_environment,
            OwnedCompilationUnitViewError::MismatchedSource,
        ),
        (
            &foreign,
            inputs,
            &fresh_names,
            &fresh_environment,
            OwnedCompilationUnitViewError::MismatchedSource,
        ),
    ] {
        assert_eq!(
            view(map, candidate, candidate_names, candidate_environment),
            Err(expected)
        );
    }
}

#[test]
fn memory_basic_handoff_preserves_source_and_typed_owner_gates() {
    with_memory(
        "fun entry(): Int = 1",
        |sources, inputs, names, name_environment, environment| {
            let typed = check_compilation_unit_types(sources, inputs, names, environment)
                .unwrap()
                .validate()
                .unwrap();
            let owned =
                check_compilation_unit_ownership(sources, inputs, names, environment, &typed)
                    .unwrap()
                    .validate()
                    .unwrap();
            assert_source_analysis_gate(
                sources,
                inputs,
                names,
                name_environment,
                environment,
                |sources, inputs, names, environment| {
                    owned_compilation_unit_view(sources, inputs, names, environment, &typed, &owned)
                        .map(|_| ())
                },
            );
            let typed_clone = typed.clone();
            let owned_clone = owned.clone();
            let view = owned_compilation_unit_view(
                sources,
                inputs,
                names,
                environment,
                &typed_clone,
                &owned_clone,
            )
            .unwrap();
            assert!(std::ptr::eq(view.types(), typed_clone.types()));
            assert!(std::ptr::eq(view.ownership(), owned_clone.ownership()));
            let repeated =
                check_compilation_unit_ownership(sources, inputs, names, environment, &typed)
                    .unwrap()
                    .validate()
                    .unwrap();
            assert!(
                owned_compilation_unit_view(sources, inputs, names, environment, &typed, &repeated)
                    .is_ok()
            );
            let fresh_typed = check_compilation_unit_types(sources, inputs, names, environment)
                .unwrap()
                .validate()
                .unwrap();
            assert_eq!(fresh_typed, typed);
            assert!(!fresh_typed.types().is_same_analysis(typed.types()));
            assert_eq!(
                owned_compilation_unit_view(
                    sources,
                    inputs,
                    names,
                    environment,
                    &fresh_typed,
                    &owned
                )
                .err(),
                Some(OwnedCompilationUnitViewError::MismatchedAnalysis)
            );
        },
    );
}

#[test]
fn memory_constant_handoff_preserves_capability_with_and_without_declarations() {
    for text in [
        "fun entry(): Int = 1",
        "const val ANSWER = 1\nfun entry(): Int = ANSWER",
    ] {
        with_memory(
            text,
            |sources, inputs, names, name_environment, environment| {
                let typed = check_compilation_unit_types(sources, inputs, names, environment)
                    .unwrap()
                    .validate_constants()
                    .unwrap();
                let owned = check_compilation_unit_constant_ownership(
                    sources,
                    inputs,
                    names,
                    environment,
                    &typed,
                )
                .unwrap()
                .validate()
                .unwrap();
                assert_source_analysis_gate(
                    sources,
                    inputs,
                    names,
                    name_environment,
                    environment,
                    |sources, inputs, names, environment| {
                        const_owned_compilation_unit_view(
                            sources,
                            inputs,
                            names,
                            environment,
                            &typed,
                            &owned,
                        )
                        .map(|_| ())
                    },
                );
                assert!(
                    owned.ownership().clone().validate().is_err(),
                    "constant entry never grants basic capability"
                );
                let typed_clone = typed.clone();
                let owned_clone = owned.clone();
                let view = const_owned_compilation_unit_view(
                    sources,
                    inputs,
                    names,
                    environment,
                    &typed_clone,
                    &owned_clone,
                )
                .unwrap();
                assert!(std::ptr::eq(view.types(), typed_clone.types()));
                assert!(std::ptr::eq(view.constant_ownership(), &owned_clone));
                assert_eq!(
                    view.constant_ownership().materializations().len(),
                    usize::from(text.starts_with("const"))
                );
                let repeated = check_compilation_unit_constant_ownership(
                    sources,
                    inputs,
                    names,
                    environment,
                    &typed,
                )
                .unwrap()
                .validate()
                .unwrap();
                assert!(
                    const_owned_compilation_unit_view(
                        sources,
                        inputs,
                        names,
                        environment,
                        &typed,
                        &repeated
                    )
                    .is_ok()
                );
                let fresh_typed = check_compilation_unit_types(sources, inputs, names, environment)
                    .unwrap()
                    .validate_constants()
                    .unwrap();
                assert_eq!(fresh_typed, typed);
                assert!(!fresh_typed.types().is_same_analysis(typed.types()));
                assert_eq!(
                    const_owned_compilation_unit_view(
                        sources,
                        inputs,
                        names,
                        environment,
                        &fresh_typed,
                        &owned
                    )
                    .err(),
                    Some(OwnedCompilationUnitViewError::MismatchedAnalysis)
                );
            },
        );
    }
}
