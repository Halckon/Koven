//! SPEC-0323: ordinary source through raw typing and final completion only.
//! No ownership, codegen, native execution, phase-product mutation, or injected facts.
use super::*;
use lang_frontend::{
    diagnostic::Diagnostic,
    source::Span,
    type_checking::{
        FinalTypeAdmissionError, complete_compilation_unit_types, complete_file_types,
    },
};

struct Observation {
    mode: &'static str,
    original: Vec<Diagnostic>,
    deferred: Vec<(Span, String)>,
    result: Result<(), FinalTypeAdmissionError>,
}

fn collect(text: &str) -> (SourceMap, Vec<Observation>) {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "main.ko", text);
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).expect("single names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &file, &names, &types).expect("single raw types");
    let original = typed.diagnostics().to_vec();
    let deferred = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(
                typed
                    .expression_type(id)
                    .and_then(|ty| typed.types().get(ty)),
                Some(TypeKind::Deferred(_))
            )
            .then(|| (node.span(), sources.slice(node.span()).unwrap().to_owned()))
        })
        .collect();
    let single = complete_file_types(&sources, &file, &names, &types, &typed).map(|_| ());
    assert_eq!(typed.diagnostics(), original);
    let mut observations = vec![Observation {
        mode: "single",
        original,
        deferred,
        result: single,
    }];
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &file)];
    let index = index_compilation_unit(&sources, &inputs).expect("unit index");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .expect("unit names")
        .validate()
        .expect("valid unit names");
    let unit = names.names().index().source_units()[0].id();
    let typed =
        check_compilation_unit_types(&sources, &inputs, &names, &types).expect("unit raw types");
    let original = typed.diagnostics().to_vec();
    let deferred = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(
                typed
                    .expression_type(UnitExpressionId::new(unit, id))
                    .and_then(|ty| typed.types().get(ty)),
                Some(UnitTypeKind::Deferred(_))
            )
            .then(|| (node.span(), sources.slice(node.span()).unwrap().to_owned()))
        })
        .collect();
    let result =
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).map(|_| ());
    assert_eq!(typed.diagnostics(), original);
    observations.push(Observation {
        mode: "unit-basic",
        original,
        deferred,
        result,
    });
    // Collect both real frontend outcomes before an expected-positive assertion can fail.
    for entry in &observations {
        eprintln!(
            "{} deferred={:?} original={:?} final={:?}",
            entry.mode, entry.deferred, entry.original, entry.result
        );
    }
    (sources, observations)
}

fn assert_deferred_source_diagnostic(sources: &SourceMap, entry: &Observation) {
    assert!(
        !entry.deferred.is_empty(),
        "{} must retain unresolved raw obligations",
        entry.mode
    );
    let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) = &entry.result else {
        panic!(
            "{} must reject with source diagnostics, not Internal or Ok: {:?}",
            entry.mode, entry.result
        );
    };
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code().to_string() == "L0165"
                && entry
                    .deferred
                    .iter()
                    .any(|(span, _)| *span == diagnostic.primary_span())
        }),
        "L0165 must identify an actual raw Deferred expression: {diagnostics:?}"
    );
    for old in &entry.original {
        assert!(
            diagnostics.iter().any(|new| new.code() == old.code()
                && new.primary_span() == old.primary_span()
                && new.message() == old.message()),
            "original diagnostic disappeared: {old:?}"
        );
    }
    for diagnostic in diagnostics {
        assert_eq!(
            sources.source_name(diagnostic.primary_span().source_id()),
            Ok("main.ko")
        );
        assert!(!sources.slice(diagnostic.primary_span()).unwrap().is_empty());
    }
}

fn check_companion(text: &str) {
    let (sources, entries) = collect(text);
    assert_deferred_source_diagnostic(&sources, &entries[0]);
    assert!(
        entries[1].deferred.is_empty(),
        "unit positive control must be fully typed"
    );
    assert!(entries[1].original.is_empty());
    entries[1]
        .result
        .as_ref()
        .expect("existing unit companion completion");
}

fn check_result(text: &str) {
    let (sources, entries) = collect(text);
    for entry in &entries {
        assert_deferred_source_diagnostic(&sources, entry);
    }
}

#[test]
fn original_companion_preserves_source_rejection_and_unit_control() {
    check_companion(include_str!(
        "../fixtures/final-admission-static-recovery-0323/p28_companion.ko"
    ));
}
#[test]
fn minimal_companion_preserves_source_rejection_and_unit_control() {
    check_companion(include_str!(
        "../fixtures/final-admission-static-recovery-0323/companion_minimal.ko"
    ));
}
#[test]
fn original_result_propagation_preserves_source_rejections() {
    check_result(include_str!(
        "../fixtures/final-admission-static-recovery-0323/p16_result_q.ko"
    ));
}
#[test]
fn minimal_result_propagation_preserves_source_rejections() {
    check_result(include_str!(
        "../fixtures/final-admission-static-recovery-0323/result_minimal.ko"
    ));
}

fn assert_original_errors(text: &str) {
    let (sources, entries) = collect(text);
    for entry in entries {
        assert!(
            !entry.original.is_empty(),
            "ordinary source must publish a real original error"
        );
        let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) = entry.result else {
            panic!(
                "{} must fail closed with original source diagnostics",
                entry.mode
            );
        };
        for old in entry.original {
            assert!(diagnostics.iter().any(|new| new.code() == old.code()
                && new.primary_span() == old.primary_span()
                && new.message() == old.message()));
            assert!(!sources.slice(old.primary_span()).unwrap().is_empty());
        }
    }
}
#[test]
fn dynamic_receiver_real_value_error_is_not_hidden() {
    assert_original_errors("fun main(): Unit { val bad = true + false; bad.fooBar() }");
}
#[test]
fn static_constructor_real_argument_error_is_not_hidden() {
    assert_original_errors("value class P(val n: Int)\nfun main(): Unit { val p = P(true) }");
}
