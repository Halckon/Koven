//! 原单源 recovery fixtures 的完整 Unit 事实合同；仅真实 producer，无事实改写。

use super::analyze_unit_recovery;
use crate::{
    analysis::{UnitSourceDescriptor, analyze_unit_names},
    diagnostic::{Diagnostic, DiagnosticCodeCatalog, Severity},
    lexer::lex,
    name_resolution::{
        CompilationUnitNames, NameEnvironment, SourceUnitInput, SourceUnitInputKey, SourceUnitKey,
        index_compilation_unit,
        resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, OwnershipDeferredReason,
        check_compilation_unit_ownership_recovering,
    },
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        CompilationUnitTypes, TypeEnvironment, check_compilation_unit_types_recovering,
        standard_environments,
    },
};

const MOVED: &str = "class Resource()\nfun take(own r: Resource): Unit {}\nfun moves(own r: Resource): Unit { take(r)\ntake(r) }";

struct Manual {
    parsed: ParsedFile,
    names: CompilationUnitNames,
    typed: CompilationUnitTypes,
    owned: CompilationUnitOwnership,
}

// Independently spell out the existing producers, including the unvalidated name path.
// Calling analyze_unit_recovery here would make the full-facts comparison self-referential.
fn manual(
    sources: &SourceMap,
    source: SourceId,
    environment: &NameEnvironment,
    types: &TypeEnvironment,
) -> Manual {
    let lexed = lex(sources, source).unwrap();
    let parsed = parse_file(sources, &lexed).unwrap();
    let local = resolve_names(sources, &parsed, environment).unwrap();
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let index = index_compilation_unit(sources, &inputs).unwrap();
    let names =
        resolve_compilation_unit_names(sources, &inputs, &index, environment).unwrap();
    assert_eq!(
        names.source_units()[0].resolution(),
        &local,
        "the original import-free fixtures retain all original local name facts"
    );
    let typed =
        check_compilation_unit_types_recovering(sources, &inputs, &names, types).unwrap();
    let owned =
        check_compilation_unit_ownership_recovering(sources, &inputs, &names, types, &typed)
            .unwrap();
    Manual {
        parsed,
        names,
        typed,
        owned,
    }
}

fn assert_moved_diagnostic(
    sources: &SourceMap,
    source: SourceId,
    owned: &CompilationUnitOwnership,
) {
    let text = sources.source_text(source).unwrap();
    let calls = text.match_indices("take(r)").collect::<Vec<_>>();
    assert_eq!(calls.len(), 2);
    let first = calls[0].0 + "take(".len();
    let second = calls[1].0 + "take(".len();
    let parameter = "own r: Resource";
    let parameter_start = text.find(parameter).unwrap();
    let catalog = DiagnosticCodeCatalog::try_new(&["L0131"]).unwrap();
    let mut expected = Diagnostic::new(
        sources,
        Severity::Error,
        catalog.resolve("L0131").unwrap(),
        "use of moved value",
        sources.span(source, second, second + 1).unwrap(),
    )
    .unwrap();
    expected
        .add_label(
            sources,
            sources.span(source, first, first + 1).unwrap(),
            "value was moved here",
        )
        .unwrap();
    expected
        .add_label(
            sources,
            sources
                .span(source, parameter_start, parameter_start + parameter.len())
                .unwrap(),
            "selected parameter declared here",
        )
        .unwrap();
    assert_eq!(owned.diagnostics(), &[expected]);
}

#[test]
fn original_single_source_fixtures_keep_complete_unit_facts_and_source_identity() {
    // These are the five original fixture strings, byte for byte, including the
    // malformed mixed input. Do not rewrite recovery source to make a gate green.
    for text in [
        String::new(),
        "fun main(): Unit { val greeting = \"你好🌍\" }\r\n".into(),
        format!("#\nfun broken( {{\nfun names(): Unit {{ missing }}\nfun types(): Unit {{ val item: String = 1 }}\n{MOVED}"),
        format!("const val unused: Int = 7\n{MOVED}"),
        "class Resource {}\nclass Holder(var payload: Resource)\nfun deferred(holders: List<Holder>): Unit { val projected = holders[0].payload }".into(),
    ] {
        let mut sources = SourceMap::new();
        let excluded = sources.add_source("excluded.ko", "# ignored subset").unwrap();
        let source = sources.add_source("memory/原始.ko", text.clone()).unwrap();
        assert_ne!(source, excluded);
        let (environment, types) = standard_environments();
        let expected = manual(&sources, source, &environment, &types);
        let snapshot = analyze_unit_names(
            sources,
            vec![UnitSourceDescriptor::memory_document(source)],
            environment,
        )
        .unwrap();
        let (typed, owned) = analyze_unit_recovery(&snapshot, &types).unwrap();
        assert_eq!(snapshot.sources().source_name(source), Ok("memory/原始.ko"));
        assert_eq!(snapshot.sources().source_text(source), Ok(text.as_str()));
        assert_eq!(
            snapshot.sources().source_text(excluded),
            Ok("# ignored subset")
        );
        assert_eq!(snapshot.descriptors().len(), 1);
        assert_eq!(snapshot.descriptors()[0].source_id(), source);
        assert_eq!(snapshot.parsed_files().len(), 1);
        assert_eq!(snapshot.parsed_files()[0].source_id(), source);
        assert_eq!(
            format!("{:?}", snapshot.parsed_files()[0]),
            format!("{:?}", expected.parsed),
            "complete original AST tables, spans, roots and ordered diagnostics"
        );
        assert_eq!(snapshot.names(), &expected.names);
        assert_eq!(typed, expected.typed, "all typed facts and diagnostics");
        assert_eq!(
            owned, expected.owned,
            "all ownership facts, diagnostics, labels and deferred entries"
        );
        assert!(!typed.is_same_analysis(&expected.typed));
        assert!(!owned.is_same_analysis(&expected.owned));
        assert!(typed.is_same_analysis(&typed.clone()));
        assert!(owned.is_same_analysis(&owned.clone()));
        let inputs = snapshot.inputs();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].source_id(), source);
        assert_eq!(inputs[0].key(), SourceUnitInputKey::MemoryDocument);
        assert!(std::ptr::eq(inputs[0].parsed(), &snapshot.parsed_files()[0]));
        assert_eq!(snapshot.names().index().source_units().len(), 1);
        assert_eq!(
            snapshot.names().index().source_units()[0].key(),
            &SourceUnitKey::MemoryDocument
        );
        assert!(typed.is_compatible_with_raw(
            snapshot.sources(), &inputs, snapshot.names(), &types,
        ));
        assert!(!typed.is_compatible_with_raw(
            snapshot.sources(), &inputs, &expected.names, &types,
        ));
        if let Ok(validated) = typed.clone().validate() {
            assert!(owned.is_compatible_with(&validated));
            assert!(!expected.owned.is_compatible_with(&validated));
        }
        if text.contains("const val unused") {
            let constants = typed.constants().expect("original constant facts survive");
            assert_eq!(constants.declarations().len(), 1);
            assert!(constants.uses().is_empty());
            assert_moved_diagnostic(snapshot.sources(), source, &owned);
            assert!(typed.clone().validate().is_err(), "const is not basic");
            typed.clone().validate_constants().expect("real const capability");
            assert!(owned.clone().validate().is_err(), "moved is not owned");
        }
        if text.contains("fun deferred") {
            assert!(owned.diagnostics().is_empty());
            let [deferred] = owned.deferred() else {
                panic!("the original unresolved index place remains explicit")
            };
            assert_eq!(deferred.reason(), OwnershipDeferredReason::IndexPlace);
            let expression = snapshot.parsed_files()[0]
                .ast()
                .expressions()
                .get(deferred.expression().expression())
                .unwrap();
            assert_eq!(expression.span().source_id(), source);
            assert_eq!(
                snapshot.sources().slice(expression.span()),
                Ok("holders[0].payload")
            );
            assert!(owned.conditional_receiver_deliveries().is_empty());
            assert!(owned.clone().validate().is_err(), "deferred is not owned");
        }
    }
}

#[test]
fn original_gate_sources_keep_their_first_real_diagnostic_stage() {
    // Retain language inputs from the removed callback protocol without recreating it.
    // Recovery still produces all facts; strict eligibility remains a separate assertion.
    for (text, expected_stage) in [
        (format!("#\n{MOVED}"), "lexer"),
        ("fun bad( {".into(), "parser"),
        (format!("fun names(): Unit {{ missing }}\n{MOVED}"), "names"),
        (
            format!("fun types(): Unit {{ val x: String = 1 }}\n{MOVED}"),
            "types",
        ),
        (MOVED.into(), "ownership"),
    ] {
        let mut sources = SourceMap::new();
        let source = sources.add_source("gate.ko", text).unwrap();
        let (environment, types) = standard_environments();
        let lexed = lex(&sources, source).unwrap();
        let expected = manual(&sources, source, &environment, &types);
        let snapshot = analyze_unit_names(
            sources,
            vec![UnitSourceDescriptor::memory_document(source)],
            environment,
        )
        .unwrap();
        let (typed, owned) = analyze_unit_recovery(&snapshot, &types).unwrap();
        assert_eq!(
            format!("{:?}", snapshot.parsed_files()[0]),
            format!("{:?}", expected.parsed)
        );
        assert_eq!(snapshot.names(), &expected.names);
        assert_eq!(typed, expected.typed);
        assert_eq!(owned, expected.owned);
        let stages = [
            ("lexer", lexed.diagnostics()),
            ("parser", snapshot.parsed_files()[0].diagnostics()),
            ("names", snapshot.names().diagnostics()),
            ("types", typed.diagnostics()),
            ("ownership", owned.diagnostics()),
        ];
        assert_eq!(
            stages
                .iter()
                .find(|(_, diagnostics)| !diagnostics.is_empty())
                .map(|(stage, _)| *stage),
            Some(expected_stage)
        );
        if expected_stage == "ownership" {
            assert_moved_diagnostic(snapshot.sources(), source, &owned);
            typed.clone().validate().expect("the real error is ownership, not types");
        } else {
            assert!(typed.clone().validate().is_err());
            assert!(typed.clone().validate_constants().is_err());
        }
        assert!(owned.validate().is_err(), "no raw rejection becomes owned capability");
    }
}
