//! Ordinary Unit diagnostics extracted from the mixed legacy identity witness.
use super::{SourceKey, source_key};
use crate::ssa::unit_lower_test_support::parsed;
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

#[test]
fn original_recovery_diagnostics_stop_at_unit_capability_gates() {
    for (body, code) in [("missing", "L0080"), ("17", "L0084")] {
        let mut sources = SourceMap::new();
        let text = format!("package pair\nfun entry(): String = {body}");
        let (source, file) = parsed(&mut sources, "pair/entry.ko", &text);
        let (name_environment, environment) = standard_environments();
        let expected_span = SourceKey(
            "pair/entry.ko".to_owned(),
            text.len() - body.len(),
            text.len(),
        );
        // Retain the original unselected same-text source without mixing products.
        let (foreign_source, _foreign_file) = parsed(&mut sources, "foreign.ko", &text);
        assert_ne!(source, foreign_source);
        let inputs = [SourceUnitInput::new("root", "pair/entry.ko", source, &file)];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment).unwrap();
        if body == "missing" {
            assert_eq!(names.diagnostics()[0].code().to_string(), code);
            assert_eq!(
                source_key(&sources, names.diagnostics()[0].primary_span()),
                expected_span
            );
            assert!(
                names.validate().is_err(),
                "unit cannot create names capability"
            );
        } else {
            let names = names.validate().unwrap();
            let typed =
                check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
            assert_eq!(typed.diagnostics()[0].code().to_string(), code);
            assert_eq!(
                source_key(&sources, typed.diagnostics()[0].primary_span()),
                expected_span
            );
            assert!(
                typed.clone().validate().is_err(),
                "basic cannot create typed capability"
            );
            assert!(
                typed.validate_constants().is_err(),
                "const cannot create typed capability"
            );
        }
    }
}

#[test]
fn original_own_receiver_source_admits_the_empty_unit_entry() {
    use crate::ssa::{
        model::TerminatorKind, unit_lower::lower_scalar_unit_with_entry,
        unit_lower_test_support::declaration, verify::verify_program,
    };

    use lang_frontend::{
        ownership_checking::check_compilation_unit_ownership,
        type_checking::complete_compilation_unit_types,
    };

    let text = include_str!("../../../../lang-frontend/tests/fixtures/legacy-receiver-entry.ko");
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "pair/entry.ko", text);
    assert_eq!(sources.source_text(source), Ok(text));
    let (ne, te) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "pair/entry.ko", source, &file),
        SourceUnitInput::memory_document(source, &file),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &te).unwrap();
        complete_compilation_unit_types(&sources, &inputs, &names, &te, &typed).unwrap();
        let typed = typed.validate().unwrap();
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &te, &typed)
            .unwrap()
            .validate()
            .unwrap();
        let (program, id) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &te,
            &typed,
            &owned,
            declaration(&names, "pair", "entry"),
        )
        .expect("original entry is admitted by normal Unit lowering");
        verify_program(&program).unwrap();
        let function = program.module(id.module()).unwrap().function(id).unwrap();
        let [entry] = function.blocks.as_slice() else {
            panic!("one empty entry block")
        };
        assert!(entry.instructions.is_empty());
        assert!(matches!(&entry.terminator.as_ref().unwrap().kind,
            TerminatorKind::Return { values } if values.is_empty()));
    }
}

#[test]
fn original_scalar_source_rejects_mixed_unit_analysis_before_lowering() {
    use crate::ssa::{
        LoweringErrorKind, unit_lower::lower_scalar_unit_with_entry, verify::verify_program,
    };
    use lang_frontend::{
        name_resolution::{NameEnvironment, ValidatedCompilationUnitNames},
        ownership_checking::{ValidatedCompilationUnitOwnership, check_compilation_unit_ownership},
        type_checking::{
            TypeEnvironment, ValidatedCompilationUnitTypes, complete_compilation_unit_types,
        },
    };

    fn types(
        sources: &SourceMap,
        inputs: &[SourceUnitInput<'_>],
        names: &ValidatedCompilationUnitNames,
        env: &TypeEnvironment,
    ) -> ValidatedCompilationUnitTypes {
        let raw = check_compilation_unit_types(sources, inputs, names, env).unwrap();
        complete_compilation_unit_types(sources, inputs, names, env, &raw).unwrap();
        raw.validate().unwrap()
    }

    fn chain(
        sources: &SourceMap,
        inputs: &[SourceUnitInput<'_>],
        ne: &NameEnvironment,
        te: &TypeEnvironment,
    ) -> (
        ValidatedCompilationUnitNames,
        ValidatedCompilationUnitTypes,
        ValidatedCompilationUnitOwnership,
    ) {
        let index = index_compilation_unit(sources, inputs).unwrap();
        let names = resolve_compilation_unit_names(sources, inputs, &index, ne)
            .unwrap()
            .validate()
            .unwrap();
        let typed = types(sources, inputs, &names, te);
        let owned = check_compilation_unit_ownership(sources, inputs, &names, te, &typed)
            .unwrap()
            .validate()
            .unwrap();
        (names, typed, owned)
    }

    // Both maps contain the exact Legacy fixture; distinct owners cannot be
    // substituted merely because their source text and logical keys coincide.
    let text = "fun value(input: Int): Int = input";
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "lowering.ko", text);
    let mut foreign_sources = SourceMap::new();
    let (foreign_source, foreign_file) = parsed(&mut foreign_sources, "lowering.ko", text);
    assert_eq!(sources.source_text(source), Ok(text));
    assert_eq!(foreign_sources.source_text(foreign_source), Ok(text));
    let (ne, te) = standard_environments();
    for (input, foreign_input) in [
        (
            SourceUnitInput::memory_document(source, &file),
            SourceUnitInput::memory_document(foreign_source, &foreign_file),
        ),
        (
            SourceUnitInput::new("root", "lowering.ko", source, &file),
            SourceUnitInput::new("root", "lowering.ko", foreign_source, &foreign_file),
        ),
    ] {
        let inputs = [input];
        let foreign_inputs = [foreign_input];
        let (names, typed, owned) = chain(&sources, &inputs, &ne, &te);
        let (_, foreign_typed, foreign_owned) = chain(&foreign_sources, &foreign_inputs, &ne, &te);
        let fresh_typed = types(&sources, &inputs, &names, &te);
        assert_eq!(typed, fresh_typed);
        assert!(!typed.types().is_same_analysis(fresh_typed.types()));
        assert!(!typed.types().is_same_analysis(foreign_typed.types()));
        let declarations = names.names().index().declarations();
        let [declaration] = declarations else {
            panic!("one original function");
        };
        assert_eq!(declaration.name(), "value");
        let declaration = declaration.id();

        let lower = |selected: &[SourceUnitInput<'_>], typed, owned| {
            lower_scalar_unit_with_entry(&sources, selected, &names, &te, typed, owned, declaration)
        };
        let (program, root) = lower(&inputs, &typed, &owned).expect("matching products lower");
        verify_program(&program).unwrap();
        let function = program
            .module(root.module())
            .unwrap()
            .function(root)
            .unwrap();
        assert_eq!(sources.slice(function.origin.span()), Ok(text));

        let wrong_inputs = [input, input];
        for (label, selected, candidate, product, expected) in [
            (
                "valid source with foreign ownership",
                inputs.as_slice(),
                &typed,
                &foreign_owned,
                LoweringErrorKind::MismatchedAnalysis,
            ),
            (
                "same source and names with a fresh typed owner",
                inputs.as_slice(),
                &fresh_typed,
                &owned,
                LoweringErrorKind::MismatchedAnalysis,
            ),
            (
                "invalid inputs take precedence over foreign ownership",
                wrong_inputs.as_slice(),
                &typed,
                &foreign_owned,
                LoweringErrorKind::MismatchedSource,
            ),
        ] {
            let Err(error) = lower(selected, candidate, product) else {
                panic!("{label}: incompatible products cannot create SSA");
            };
            assert_eq!((error.kind, error.span), (expected, None), "{label}");
        }
    }
}
