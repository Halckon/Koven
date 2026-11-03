//! SPEC-0341: independent complete L0082 oracles for signature and body producers.

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticCodeCatalog, Severity},
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::parse_file,
    source::{SourceId, SourceMap},
    type_checking::{
        FinalTypeAdmissionError, check_compilation_unit_types, check_types,
        complete_compilation_unit_types, complete_file_types, standard_environments,
    },
};

fn expected(sources: &SourceMap, source: SourceId, text: &str) -> Vec<Diagnostic> {
    let catalog = DiagnosticCodeCatalog::try_new(&["L0082"]).unwrap();
    // Source tokens define the oracle; neither checker nor formatter produces it.
    text.match_indices("<String>")
        .map(|(start, _)| {
            let name_start = text[..start]
                .rfind(|ch: char| !ch.is_ascii_alphanumeric())
                .map_or(0, |offset| offset + 1);
            let mut diagnostic = Diagnostic::new(
                sources,
                Severity::Error,
                catalog.resolve("L0082").unwrap(),
                "builtin type does not accept type arguments",
                sources.span(source, start + 1, start + 7).unwrap(),
            )
            .unwrap();
            diagnostic
                .add_label(
                    sources,
                    sources.span(source, name_start, start).unwrap(),
                    "builtin type declared here",
                )
                .unwrap();
            diagnostic
        })
        .collect()
}

fn rejected(result: Result<(), FinalTypeAdmissionError>) -> Vec<Diagnostic> {
    match result {
        Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => diagnostics,
        other => panic!("expected complete source rejection, not success or Internal: {other:?}"),
    }
}

fn assert_single_and_units(text: &str, signature_count: usize) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("builtin-arguments.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let expected = expected(&sources, source, text);
    assert!(!expected.is_empty());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty());
    let single = check_types(&sources, &parsed, &names, &types).unwrap();
    assert_eq!(single.diagnostics(), expected);
    assert_eq!(
        rejected(complete_file_types(&sources, &parsed, &names, &types, &single).map(|_| ())),
        expected
    );
    for input in [
        SourceUnitInput::memory_document(source, &parsed),
        SourceUnitInput::new("root", "builtin-arguments.ko", source, &parsed),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let names = names.validate().unwrap();
        let unit = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert_eq!(
            unit.signatures().diagnostics(),
            &expected[..signature_count]
        );
        assert_eq!(unit.body_diagnostics(), &expected[signature_count..]);
        assert_eq!(unit.diagnostics(), expected);
        assert_eq!(unit.diagnostics(), single.diagnostics());
        assert_eq!(
            rejected(
                complete_compilation_unit_types(&sources, &inputs, &names, &types, &unit)
                    .map(|_| ())
            ),
            expected
        );
    }
}

#[test]
fn signature_and_body_builtin_arguments_keep_the_same_complete_label() {
    for builtin in ["Int", "Boolean", "IntRange", "LongRange"] {
        assert_single_and_units(
            &format!("fun probe(value: {builtin}<String>): Unit {{}}"),
            1,
        );
        assert_single_and_units(
            &format!("fun probe(): Unit {{ val bad: {builtin}<String> = return }}"),
            0,
        );
    }
}

#[test]
fn return_signature_and_body_labels_keep_raw_final_order_with_crlf_and_unicode() {
    assert_single_and_units(
        "// 界é😀\r\nfun probe(): IntRange<String> {\r\nval bad: Boolean<String> = return\r\n}",
        1,
    );
}

#[test]
fn reordered_sources_keep_each_complete_diagnostic_on_its_original_source() {
    let mut sources = SourceMap::new();
    let text = "fun probe(value: IntRange<String>): Unit {}";
    // Different names avoid duplicate declarations; the error tokens have identical offsets.
    let other_text = text.replace("probe", "other");
    let a = sources.add_source("a.ko", text).unwrap();
    let z = sources.add_source("z.ko", &other_text).unwrap();
    let a_parsed = parse_file(&sources, &lex(&sources, a).unwrap()).unwrap();
    let z_parsed = parse_file(&sources, &lex(&sources, z).unwrap()).unwrap();
    assert!(a_parsed.diagnostics().is_empty());
    assert!(z_parsed.diagnostics().is_empty());
    let mut expected = expected(&sources, a, text);
    expected.extend(self::expected(&sources, z, &other_text));
    let inputs = [
        SourceUnitInput::new("root", "a.ko", a, &a_parsed),
        SourceUnitInput::new("root", "z.ko", z, &z_parsed),
    ];
    let (environment, types) = standard_environments();
    for inputs in [inputs, [inputs[1], inputs[0]]] {
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let names = names.validate().unwrap();
        let unit = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert_eq!(unit.signatures().diagnostics(), expected);
        assert!(unit.body_diagnostics().is_empty());
        assert_eq!(unit.diagnostics(), expected);
        assert_eq!(
            rejected(
                complete_compilation_unit_types(&sources, &inputs, &names, &types, &unit)
                    .map(|_| ())
            ),
            expected
        );
    }
}
