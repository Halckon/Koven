//! Explicit final qualification, separate from the still-valid raw/base controls.
use super::*;
use lang_frontend::{
    analysis::{SingleFileAnalysisError, analyze_single_file},
    diagnostic::Diagnostic,
    type_checking::{
        FinalTypeAdmissionError, complete_compilation_unit_types, complete_file_types,
    },
};

fn diagnostics(result: Result<(), FinalTypeAdmissionError>) -> Vec<Diagnostic> {
    match result {
        Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => {
            assert!(
                !diagnostics.is_empty(),
                "a failed source gate must explain its root"
            );
            diagnostics
        }
        other => panic!("expected source diagnostics, got {other:?}"),
    }
}

fn complete_both(
    text: &str,
    compare_fresh_analyses: bool,
) -> (
    SourceMap,
    Result<(), FinalTypeAdmissionError>,
    Result<(), FinalTypeAdmissionError>,
) {
    let mut sources = SourceMap::new();
    // Holding an unselected bad file must not poison this input's final qualification.
    sources.add_source("unselected.ko", FROZEN[3].0).unwrap();
    let (source, file) = parsed(&mut sources, "main.ko", text);
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &file, &names, &types).unwrap();
    let raw_diagnostics = typed.diagnostics().to_vec();
    let single = complete_file_types(&sources, &file, &names, &types, &typed).map(|complete| {
        assert!(std::ptr::eq(complete.types(), &typed));
        assert!(complete.is_same_analysis(&typed));
        if compare_fresh_analyses {
            let again = check_types(&sources, &file, &names, &types).unwrap();
            assert!(
                !complete.is_same_analysis(&again),
                "same source is not the same analysis"
            );
        }
    });
    assert_eq!(typed.diagnostics(), raw_diagnostics);
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &file)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    let raw_diagnostics = typed.diagnostics().to_vec();
    let unit = complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).map(
        |complete| {
            assert!(std::ptr::eq(complete.types(), &typed));
            assert!(complete.is_same_analysis(&typed));
            if compare_fresh_analyses {
                let again =
                    check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
                assert!(!complete.is_same_analysis(&again));
            }
        },
    );
    assert_eq!(typed.diagnostics(), raw_diagnostics);
    (sources, single, unit)
}

fn assert_exact(
    sources: &SourceMap,
    source_name: &str,
    result: Result<(), FinalTypeAdmissionError>,
    expected: &[(&str, usize, usize, &str)],
) {
    let diagnostics = diagnostics(result);
    assert_eq!(diagnostics.len(), expected.len(), "{diagnostics:?}");
    for (diagnostic, &(code, start, end, token)) in diagnostics.iter().zip(expected) {
        assert_eq!(diagnostic.code().to_string(), code);
        let span = diagnostic.primary_span();
        assert_eq!(sources.source_name(span.source_id()), Ok(source_name));
        assert_eq!((span.start(), span.end()), (start, end));
        assert_eq!(sources.slice(span), Ok(token));
    }
}

#[test]
fn frozen_roots_reject_with_exact_source_diagnostics() {
    let expected = [
        ("L0080", 40, 46, "fooBar"),
        ("L0080", 44, 50, "length"),
        ("L0165", 40, 45, "0..<5"),
        ("L0085", 32, 33, "?"),
    ];
    for ((text, _, _), &(code, start, end, token)) in FROZEN.iter().zip(&expected) {
        for prefix in ["", "const val PREFIX: Int = 1\n"] {
            let text = format!("{prefix}{text}");
            let (sources, single, unit) = complete_both(&text, false);
            let expected = [(code, start + prefix.len(), end + prefix.len(), token)];
            assert_exact(&sources, "main.ko", single, &expected);
            assert_exact(&sources, "main.ko", unit, &expected);
        }
        // A real completion failure in the normal observer seam must precede ownership.
        // Neither callback forces an artificial failure or rewrites a phase product.
        let mut sources = SourceMap::new();
        let source = sources.add_source("strict.ko", *text).unwrap();
        let (environment, types) = standard_environments();
        let result = analyze_single_file(
            &sources,
            source,
            &environment,
            &types,
            |_, diagnostics| {
                if diagnostics.is_empty() {
                    Ok(())
                } else {
                    Err(FinalTypeAdmissionError::Diagnostics(diagnostics.to_vec()))
                }
            },
            |view| {
                complete_file_types(&sources, view.parsed(), view.names(), &types, view.typed())
                    .map(|_| ())
            },
        );
        match result {
            Err(SingleFileAnalysisError::Host(error)) => {
                assert_exact(
                    &sources,
                    "strict.ko",
                    Err(error),
                    &[(code, start, end, token)],
                );
            }
            other => panic!("source root must stop before ownership: {other:?}"),
        }
    }
    // Nothing is bottom, so this slice does not invent a new illegal-operand rule for it.
    let text = "fun stop(): Nothing = error(\"stop\")\nfun inspect(): Int? = stop()?";
    let (sources, single, unit) = complete_both(text, false);
    let start = text.rfind("stop()?").unwrap();
    let expected = [("L0165", start, start + "stop()?".len(), "stop()?")];
    assert_exact(&sources, "main.ko", single, &expected);
    assert_exact(&sources, "main.ko", unit, &expected);
}

#[test]
fn resolved_roles_acquire_only_their_original_analysis_qualification() {
    let (_, single, unit) = complete_both(VALID_ROLES, true);
    single.expect("resolved single value obligations");
    unit.expect("resolved unit value obligations");
}

#[test]
fn independent_roots_are_ordered_without_poison_cascades() {
    let mut sources = SourceMap::new();
    let texts = [
        "package p\nfun first(): Int { val n = 1; return n.fooBar() }",
        "package p\nfun second(x: Int?): Int? = x?",
        "package p\n// 中文\nfun poisoned(): Int = (true + false)!!\nfun good(): Long = 2L",
    ];
    let (a, af) = parsed(&mut sources, "a.ko", texts[0]);
    let (b, bf) = parsed(&mut sources, "b.ko", texts[1]);
    let (c, cf) = parsed(&mut sources, "c.ko", texts[2]);
    sources.add_source("unused.ko", FROZEN[0].0).unwrap();
    let inputs = [
        SourceUnitInput::new("root", "p/a.ko", a, &af),
        SourceUnitInput::new("root", "p/b.ko", b, &bf),
        SourceUnitInput::new("root", "p/c.ko", c, &cf),
    ];
    let (environment, types) = standard_environments();
    let mut ordered = Vec::new();
    for inputs in [inputs, [inputs[2], inputs[0], inputs[1]]] {
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert_eq!(
            typed.diagnostics().len(),
            1,
            "raw keeps only its existing operator error"
        );
        assert_eq!(typed.diagnostics()[0].code().to_string(), "L0085");
        let result =
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).map(|_| ());
        let diagnostics = diagnostics(result);
        assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
        let expected = [
            (a, "L0080", texts[0].find("fooBar").unwrap(), "fooBar"),
            (b, "L0085", texts[1].rfind('?').unwrap(), "?"),
            (c, "L0085", texts[2].find('+').unwrap(), "+"),
        ];
        for (diagnostic, (source, code, start, token)) in diagnostics.iter().zip(expected) {
            assert_eq!(diagnostic.code().to_string(), code);
            assert_eq!(
                diagnostic.primary_span(),
                sources.span(source, start, start + token.len()).unwrap()
            );
        }
        ordered.push(diagnostics);
        assert_eq!(
            typed.diagnostics().len(),
            1,
            "final analysis must not mutate raw recovery"
        );
    }
    assert_eq!(ordered[0], ordered[1]);
}

#[test]
fn selected_outer_calls_do_not_hide_illegal_argument_subtrees() {
    for (body, marker, code) in [
        ("val n = 1; val bad = n.fooBar(); 1", "fooBar", "L0080"),
        ("val bad = nullable?; 1", "?;", "L0085"),
    ] {
        let call = format!("take(if (true) {{ {body} }} else {{ 2 }})");
        let text = format!("fun take(n: Int): Int = n\nfun inspect(nullable: Int?): Int = {call}");
        // The outer descriptor is genuinely selected despite an unused invalid inner local.
        with_types(&text, |sources, file, single, unit, source| {
            assert!(
                single.diagnostics().is_empty(),
                "{:?}",
                single.diagnostics()
            );
            assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
            let id = expression(sources, file, &call);
            assert!(single.call(id).is_some(), "selected outer single call");
            assert!(
                unit.call(UnitExpressionId::new(source, id)).is_some(),
                "selected outer unit call"
            );
        });
        let start = text.find(marker).unwrap();
        let token = if code == "L0085" { "?" } else { marker };
        let (sources, single, unit) = complete_both(&text, false);
        let expected = [(code, start, start + token.len(), token)];
        assert_exact(&sources, "main.ko", single, &expected);
        assert_exact(&sources, "main.ko", unit, &expected);
    }
}
