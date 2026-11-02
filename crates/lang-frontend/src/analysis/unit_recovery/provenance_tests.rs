//! Existing typed diagnostics never waive the recovery producer's identity checks.
use crate::{
    diagnostic::{Diagnostic, DiagnosticCodeCatalog, Severity},
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{OwnershipCheckingError, check_compilation_unit_ownership_recovering},
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{check_compilation_unit_types_recovering, standard_environments},
};

const SOURCE: &str = "fun wrong(): Int = true\n";

fn parse(sources: &SourceMap, source: SourceId) -> ParsedFile {
    let lexed = lex(sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    parsed
}

fn input(memory: bool, source: SourceId, parsed: &ParsedFile) -> SourceUnitInput<'_> {
    if memory {
        SourceUnitInput::memory_document(source, parsed)
    } else {
        SourceUnitInput::new("root", "wrong.ko", source, parsed)
    }
}

#[test]
fn typed_recovery_diagnostics_do_not_bypass_foreign_context_rejection() {
    for memory in [false, true] {
        let mut sources = SourceMap::new();
        let source = sources.add_source("wrong.ko", SOURCE).unwrap();
        let parsed = parse(&sources, source);
        let inputs = [input(memory, source, &parsed)];
        let (ne, te) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne).unwrap();
        assert!(names.diagnostics().is_empty());
        let typed =
            check_compilation_unit_types_recovering(&sources, &inputs, &names, &te).unwrap();
        let catalog = DiagnosticCodeCatalog::try_new(&["L0084"]).unwrap();
        let primary = SOURCE.find("true").unwrap();
        let label = SOURCE.find("Int").unwrap();
        let mut expected = Diagnostic::new(
            &sources,
            Severity::Error,
            catalog.resolve("L0084").unwrap(),
            "expression type does not match the expected type",
            sources.span(source, primary, primary + 4).unwrap(),
        )
        .unwrap();
        expected
            .add_label(
                &sources,
                sources.span(source, label, label + 3).unwrap(),
                "expected Int, found Boolean",
            )
            .unwrap();
        assert_eq!(typed.diagnostics(), [expected.clone()]);
        assert!(typed.clone().validate().is_err());

        // Ordinary L0084 is a successful raw recovery, not an internal mismatch.
        let owned =
            check_compilation_unit_ownership_recovering(&sources, &inputs, &names, &te, &typed)
                .unwrap();
        assert!(owned.diagnostics().is_empty());
        assert!(owned.deferred().is_empty());
        assert!(owned.formed_owned_locals().is_empty());
        assert!(owned.validate().is_err());
        assert_eq!(typed.diagnostics(), [expected.clone()]);

        // Equal source/index/name values must not substitute a fresh name analysis.
        let fresh_names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne).unwrap();
        assert_eq!(fresh_names, names);
        let reparsed = parse(&sources, source);
        let reparsed_inputs = [input(memory, source, &reparsed)];
        let reparsed_index = index_compilation_unit(&sources, &reparsed_inputs).unwrap();
        let reparsed_names =
            resolve_compilation_unit_names(&sources, &reparsed_inputs, &reparsed_index, &ne)
                .unwrap();
        assert_eq!(reparsed_names, names);
        let (_, foreign_environment) = standard_environments();
        let mut foreign_sources = SourceMap::new();
        let foreign_source = foreign_sources.add_source("wrong.ko", SOURCE).unwrap();
        let foreign_parsed = parse(&foreign_sources, foreign_source);
        let foreign_inputs = [input(memory, foreign_source, &foreign_parsed)];
        let duplicate_inputs = [inputs[0], inputs[0]];
        let changed_key = [input(!memory, source, &parsed)];
        for (case, map, selected, context_names, environment) in [
            (
                "fresh names",
                &sources,
                inputs.as_slice(),
                &fresh_names,
                &te,
            ),
            (
                "reparsed names",
                &sources,
                reparsed_inputs.as_slice(),
                &reparsed_names,
                &te,
            ),
            (
                "foreign environment",
                &sources,
                inputs.as_slice(),
                &names,
                &foreign_environment,
            ),
            (
                "foreign map only",
                &foreign_sources,
                inputs.as_slice(),
                &names,
                &te,
            ),
            (
                "foreign map and inputs",
                &foreign_sources,
                foreign_inputs.as_slice(),
                &names,
                &te,
            ),
            (
                "duplicate inputs",
                &sources,
                duplicate_inputs.as_slice(),
                &names,
                &te,
            ),
            (
                "different input key",
                &sources,
                changed_key.as_slice(),
                &names,
                &te,
            ),
        ] {
            assert!(
                matches!(
                    check_compilation_unit_ownership_recovering(
                        map,
                        selected,
                        context_names,
                        environment,
                        &typed,
                    ),
                    Err(OwnershipCheckingError::MismatchedCompilationUnitTypes)
                ),
                "{case}, memory={memory}: an ordinary diagnostic cannot mask foreign provenance"
            );
            assert_eq!(typed.diagnostics(), [expected.clone()]);
        }
    }
}
