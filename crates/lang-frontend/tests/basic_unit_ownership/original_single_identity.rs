//! Distinguish the old top-level value fixture from an admitted Unit identity chain.
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        OwnedCompilationUnitViewError, OwnershipCheckingError, check_compilation_unit_ownership,
        owned_compilation_unit_view,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        FinalTypeAdmissionError, check_compilation_unit_types, complete_compilation_unit_types,
        standard_environments,
    },
};

#[test]
fn original_scalar_sources_preserve_unit_analysis_and_source_boundaries() {
    for rooted in [false, true] {
        let (ne, te) = standard_environments();
        for (path, text) in [
            ("first.ko", "val first = 1"),
            ("second.ko", "val second = 2"),
        ] {
            let mut sources = SourceMap::new();
            let source = sources.add_source(path, text).unwrap();
            let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
            let inputs = [if rooted {
                SourceUnitInput::new("root", path, source, &file)
            } else {
                SourceUnitInput::memory_document(source, &file)
            }];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
                .unwrap()
                .validate()
                .unwrap();
            let raw = check_compilation_unit_types(&sources, &inputs, &names, &te).unwrap();
            let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) =
                complete_compilation_unit_types(&sources, &inputs, &names, &te, &raw)
            else {
                panic!("the unchanged Single fixture must fail Unit final admission");
            };
            let [diagnostic] = diagnostics.as_slice() else {
                panic!("expected one top-level runtime value diagnostic: {diagnostics:?}");
            };
            let span = diagnostic.primary_span();
            assert_eq!(diagnostic.code().to_string(), "L0167");
            assert_eq!((span.source_id(), span.start(), span.end()), (source, 0, 3));
            assert_eq!(sources.slice(span), Ok("val"));
        }

        // Functions are separately admitted; do not pretend these are the unchanged old inputs.
        let first_text = "fun first(): Int = 1";
        let mut sources = SourceMap::new();
        let first = sources.add_source("first.ko", first_text).unwrap();
        let second = sources
            .add_source("second.ko", "fun second(): Int = 2")
            .unwrap();
        let first_file = parse_file(&sources, &lex(&sources, first).unwrap()).unwrap();
        let second_file = parse_file(&sources, &lex(&sources, second).unwrap()).unwrap();
        let input = |path, source, file| {
            if rooted {
                SourceUnitInput::new("root", path, source, file)
            } else {
                SourceUnitInput::memory_document(source, file)
            }
        };
        let first_inputs = [input("first.ko", first, &first_file)];
        let second_inputs = [input("second.ko", second, &second_file)];
        let (foreign_ne, foreign_te) = standard_environments();
        let first_index = index_compilation_unit(&sources, &first_inputs).unwrap();
        let second_index = index_compilation_unit(&sources, &second_inputs).unwrap();
        let names = |inputs: &[SourceUnitInput<'_>], index, env| {
            resolve_compilation_unit_names(&sources, inputs, index, env)
                .unwrap()
                .validate()
                .unwrap()
        };
        let first_names = names(&first_inputs, &first_index, &ne);
        let second_names = names(&second_inputs, &second_index, &ne);
        let fresh_names = names(&first_inputs, &first_index, &ne);
        let foreign_names = names(&first_inputs, &first_index, &foreign_ne);
        let types = |inputs: &[SourceUnitInput<'_>], names, env| {
            let raw = check_compilation_unit_types(&sources, inputs, names, env).unwrap();
            complete_compilation_unit_types(&sources, inputs, names, env, &raw).unwrap();
            raw.validate().unwrap()
        };
        let first_types = types(&first_inputs, &first_names, &te);
        let second_types = types(&second_inputs, &second_names, &te);
        let fresh_types = types(&first_inputs, &first_names, &te);
        let fresh_names_types = types(&first_inputs, &fresh_names, &te);
        let foreign_types = types(&first_inputs, &foreign_names, &foreign_te);
        let owned = check_compilation_unit_ownership(
            &sources,
            &first_inputs,
            &first_names,
            &te,
            &first_types,
        )
        .unwrap()
        .validate()
        .unwrap();
        let view = owned_compilation_unit_view(
            &sources,
            &first_inputs,
            &first_names,
            &te,
            &first_types,
            &owned,
        )
        .unwrap();
        assert_eq!(view.inputs()[0].source_id(), first);
        assert!(std::ptr::eq(view.inputs()[0].parsed(), &first_file));
        assert_eq!(view.sources().source_text(first), Ok(first_text));
        assert!(view.types().is_same_analysis(first_types.types()));
        assert!(view.ownership().is_same_analysis(owned.ownership()));

        for (names, env, typed) in [
            (&second_names, &te, &first_types),
            (&first_names, &te, &second_types),
            (&first_names, &te, &foreign_types),
            (&first_names, &te, &fresh_names_types),
            (&first_names, &foreign_te, &first_types),
        ] {
            assert!(matches!(
                check_compilation_unit_ownership(&sources, &first_inputs, names, env, typed),
                Err(OwnershipCheckingError::MismatchedCompilationUnitTypes)
            ));
            assert!(matches!(
                owned_compilation_unit_view(&sources, &first_inputs, names, env, typed, &owned),
                Err(OwnedCompilationUnitViewError::MismatchedAnalysis)
            ));
        }

        // A fresh type analysis with matching names is legal; it does not own the old facts.
        assert!(!first_types.types().is_same_analysis(fresh_types.types()));
        let fresh_owned = check_compilation_unit_ownership(
            &sources,
            &first_inputs,
            &first_names,
            &te,
            &fresh_types,
        )
        .unwrap()
        .validate()
        .unwrap();
        assert!(
            owned_compilation_unit_view(
                &sources,
                &first_inputs,
                &first_names,
                &te,
                &fresh_types,
                &fresh_owned,
            )
            .is_ok()
        );
        assert!(matches!(
            owned_compilation_unit_view(
                &sources,
                &first_inputs,
                &first_names,
                &te,
                &fresh_types,
                &owned,
            ),
            Err(OwnedCompilationUnitViewError::MismatchedAnalysis)
        ));
        assert!(matches!(
            owned_compilation_unit_view(
                &SourceMap::new(),
                &first_inputs,
                &second_names,
                &te,
                &fresh_types,
                &owned,
            ),
            Err(OwnedCompilationUnitViewError::MismatchedSource)
        ));
    }
}
