//! Explicit final qualification, separate from the still-valid raw/base controls.
use super::*;
use lang_frontend::{
    analysis::analyze_basic_unit_ownership,
    diagnostic::Diagnostic,
    ownership_checking::check_compilation_unit_constant_ownership,
    type_checking::{
        FinalTypeAdmissionError, complete_compilation_unit_types, complete_file_types,
    },
};

// A real strict consumer: final qualification precedes either actual ownership producer.
// The boolean records entry to that operation, not a callback that supplies the result.
fn strict_unit_ownership(text: &str) -> (SourceMap, Result<(), FinalTypeAdmissionError>, bool) {
    let mut sources = SourceMap::new();
    sources.add_source("unselected.ko", FROZEN[3].0).unwrap();
    let (source, file) = parsed(&mut sources, "strict.ko", text);
    let inputs = [SourceUnitInput::memory_document(source, &file)];
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .expect("the frozen roots have valid names");
    let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    let before = raw.clone();
    let mut ownership_started = false;
    let result = (|| {
        let admission = complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw);
        assert_eq!(raw, before, "final admission must not alter raw facts");
        assert!(raw.is_same_analysis(&before));
        let complete = admission?;
        assert!(complete.is_same_analysis(&raw));
        assert!(std::ptr::eq(complete.types(), &raw));
        ownership_started = true;
        match analyze_basic_unit_ownership(&sources, &inputs, &names, &types, raw)
            .expect("ownership internal failure is not a successful source rejection")
            .into_result()
        {
            Ok((typed, owned)) => {
                assert!(owned.is_compatible_with(&typed));
                owned.validate().expect("healthy basic ownership");
            }
            Err(raw) => {
                let typed = raw
                    .validate_constants()
                    .expect("NotBasic must retain the real const capability");
                let owned = check_compilation_unit_constant_ownership(
                    &sources, &inputs, &names, &types, &typed,
                )
                .expect("constant ownership internal failure");
                assert!(owned.is_compatible_with(&typed));
                owned.validate().expect("healthy constant ownership");
            }
        }
        Ok(())
    })();
    (sources, result, ownership_started)
}

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
        Some(("L0080", 40, 46, "fooBar")),
        Some(("L0080", 44, 50, "length")),
        None,
        Some(("L0085", 32, 33, "?")),
    ];
    for ((text, _, _), expected) in FROZEN.iter().zip(&expected) {
        let Some((code, start, end, token)) = *expected else {
            for prefix in ["", "const val PREFIX: Int = 1\n"] {
                let (_, single, unit) = complete_both(&format!("{prefix}{text}"), false);
                single.expect("original p34 single numeric for qualifies");
                unit.expect("original p34 unit numeric for qualifies");
                let (_, result, ownership_started) =
                    strict_unit_ownership(&format!("{prefix}{text}"));
                result.expect("original p34 reaches actual strict Unit ownership");
                assert!(ownership_started, "the downstream control is reachable");
            }
            continue;
        };
        for prefix in ["", "const val PREFIX: Int = 1\n"] {
            let text = format!("{prefix}{text}");
            let (sources, single, unit) = complete_both(&text, false);
            let expected = [(code, start + prefix.len(), end + prefix.len(), token)];
            assert_exact(&sources, "main.ko", single, &expected);
            assert_exact(&sources, "main.ko", unit, &expected);
            let (sources, result, ownership_started) = strict_unit_ownership(&text);
            assert_exact(&sources, "strict.ko", result, &expected);
            assert!(!ownership_started, "final admission must precede ownership");
        }
    }
    let invalid = include_str!("../fixtures/final-type-admission-0319/invalid_integer_for.ko");
    for prefix in ["", "const val PREFIX: Int = 1\n"] {
        let (sources, single, unit) = complete_both(&format!("{prefix}{invalid}"), false);
        let expected = [("L0159", 29 + prefix.len(), 30 + prefix.len(), "1")];
        assert_exact(&sources, "main.ko", single, &expected);
        assert_exact(&sources, "main.ko", unit, &expected);
        let (sources, result, ownership_started) =
            strict_unit_ownership(&format!("{prefix}{invalid}"));
        assert_exact(&sources, "strict.ko", result, &expected);
        assert!(!ownership_started);
    }
    // Nothing is bottom, so this slice does not invent a new illegal-operand rule for it.
    let text = "fun stop(): Nothing = error(\"stop\")\nfun inspect(): Int? = stop()?";
    let (sources, single, unit) = complete_both(text, false);
    let start = text.rfind("stop()?").unwrap();
    let expected = [("L0165", start, start + "stop()?".len(), "stop()?")];
    assert_exact(&sources, "main.ko", single, &expected);
    assert_exact(&sources, "main.ko", unit, &expected);
    let (sources, result, ownership_started) = strict_unit_ownership(text);
    assert_exact(&sources, "strict.ko", result, &expected);
    assert!(!ownership_started);
}

#[test]
fn resolved_roles_acquire_only_their_original_analysis_qualification() {
    let (_, single, unit) = complete_both(VALID_ROLES, true);
    single.expect("resolved single value obligations");
    unit.expect("resolved unit value obligations");
    for text in [
        "fun iterate(xs: List<Int>): Int { var sum = 0; for (_ in xs) {}; for (n in xs) { sum = sum + n }; return sum }",
        "value class Pair(val n: Int, val flag: Boolean)\nfun inspect(xs: List<Pair>): Unit { for ((_, flag) in xs) {} }",
    ] {
        let (_, single, unit) = complete_both(text, false);
        single.expect("ordinary iteration/discard single qualification");
        unit.expect("ordinary iteration/discard unit qualification");
    }
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

#[test]
fn cross_file_references_reuse_the_defining_root_diagnostic() {
    for paths in [
        ["p/a-definition.ko", "p/b-uses.ko", "p/c-independent.ko"],
        ["p/z-definition.ko", "p/a-uses.ko", "p/m-independent.ko"],
    ] {
        assert_shared_root_diagnostics(paths);
    }
}

fn assert_shared_root_diagnostics(paths: [&str; 3]) {
    let mut sources = SourceMap::new();
    let definition = "package p\nval receiver: Int = 1\nval broken = receiver.fooBar()";
    let (a, af) = parsed(&mut sources, "definition.ko", definition);
    let (b, bf) = parsed(
        &mut sources,
        "uses.ko",
        "package p\nfun first(): Int = broken\nfun second(): Int = broken",
    );
    let independent = "package p\nval otherReceiver: Int = 2\nval unrelated = otherReceiver.baz()";
    let (c, cf) = parsed(&mut sources, "independent.ko", independent);
    let inputs = [
        SourceUnitInput::new("root", paths[0], a, &af),
        SourceUnitInput::new("root", paths[1], b, &bf),
        SourceUnitInput::new("root", paths[2], c, &cf),
    ];
    let (environment, types) = standard_environments();
    let mut results = Vec::new();
    for selected in [inputs, [inputs[2], inputs[1], inputs[0]]] {
        let index = index_compilation_unit(&sources, &selected).unwrap();
        let names = resolve_compilation_unit_names(&sources, &selected, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &selected, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert!(
            typed.clone().validate().is_ok(),
            "the old base contract is unchanged"
        );
        let source_unit = |source| {
            names
                .names()
                .index()
                .source_units()
                .iter()
                .find(|unit| unit.source_id() == source)
                .unwrap()
                .id()
        };
        let first_root = UnitExpressionId::new(
            source_unit(a),
            expression(&sources, &af, "receiver.fooBar()"),
        );
        let other_root = UnitExpressionId::new(
            source_unit(c),
            expression(&sources, &cf, "otherReceiver.baz()"),
        );
        let first_type = typed
            .expression_type(first_root)
            .expect("the defining root has a real raw type fact");
        let other_type = typed
            .expression_type(other_root)
            .expect("the independent root has a real raw type fact");
        assert_eq!(
            first_type, other_type,
            "independent roots share a canonical deferred type, not a diagnostic identity"
        );
        assert_eq!(
            typed.types().get(first_type),
            Some(&UnitTypeKind::Deferred(DeferredReason::MemberAccess)),
        );
        let result = complete_compilation_unit_types(&sources, &selected, &names, &types, &typed)
            .map(|_| ());
        eprintln!("cross-file shared root {paths:?}: {result:?}");
        results.push(result);
    }
    let start = definition.find("fooBar").unwrap();
    let other_start = independent.find("baz").unwrap();
    let mut expected = [
        (paths[0], sources.span(a, start, start + 6).unwrap()),
        (
            paths[2],
            sources.span(c, other_start, other_start + 3).unwrap(),
        ),
    ];
    expected.sort_by_key(|(path, _)| *path);
    let mut snapshots = Vec::new();
    for result in results {
        let diagnostics = diagnostics(result);
        assert_eq!(
            diagnostics.len(),
            2,
            "derived references must not add L0165: {diagnostics:?}"
        );
        for (diagnostic, (_, span)) in diagnostics.iter().zip(expected) {
            assert_eq!(diagnostic.code().to_string(), "L0080");
            assert_eq!(diagnostic.primary_span(), span);
        }
        snapshots.push(diagnostics);
    }
    assert_eq!(snapshots[0], snapshots[1]);
}

#[test]
fn early_call_diagnostics_preserve_unvisited_type_arguments() {
    let cases = [
        (
            "fun inspect(text: String): String = text.clone<Int>()\nfun separate(): Int { val n = 1; return n.fooBar() }",
            "clone",
        ),
        (
            "fun inspect(value: Int): Int = value.inv<Int>()\nfun separate(): Int { val n = 1; return n.fooBar() }",
            "inv",
        ),
    ];
    let mut results = Vec::new();
    for (text, member) in cases {
        with_types(text, |_, _, single, unit, _| {
            for diagnostics in [single.diagnostics(), unit.diagnostics()] {
                assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
                assert_eq!(diagnostics[0].code().to_string(), "L0091");
            }
            assert!(single.calls().is_empty());
            assert!(unit.calls().is_empty());
        });
        let (sources, single, unit) = complete_both(text, false);
        eprintln!("early {member} source error: single={single:?}; unit={unit:?}");
        results.push((text, member, sources, single, unit));
    }
    for (text, member, sources, single, unit) in results {
        let start = text.find(member).unwrap();
        let independent = text.find("fooBar").unwrap();
        let expected = [
            ("L0091", start, start + member.len(), member),
            ("L0080", independent, independent + 6, "fooBar"),
        ];
        assert_exact(&sources, "main.ko", single, &expected);
        assert_exact(&sources, "main.ko", unit, &expected);
    }
}

#[test]
fn real_underscore_bindings_keep_their_root_while_discards_remain_valid() {
    for text in [
        "fun echo(_: Int): Int = _\nfun inspect(): Int { val _ = 3; return echo(_) }",
        "value class Duo(val n: Int, val flag: Boolean)\nfun inspect(xs: List<Duo>): Unit { for ((_, flag) in xs) {}; for (_ in xs) {} }",
    ] {
        let (_, single, unit) = complete_both(text, false);
        single.expect("real underscore values or actual discard: single");
        unit.expect("real underscore values or actual discard: unit");
    }
    let text = "fun inspect(): Int { val n = 1; val _ = n.fooBar(); return _ }";
    let (sources, single, unit) = complete_both(text, false);
    eprintln!("underscore root: single={single:?}; unit={unit:?}");
    let start = text.find("fooBar").unwrap();
    let expected = [("L0080", start, start + 6, "fooBar")];
    assert_exact(&sources, "main.ko", single, &expected);
    assert_exact(&sources, "main.ko", unit, &expected);
}

#[test]
fn initializer_order_does_not_complete_forward_types_or_cycles() {
    let cases = [
        "val first = second\nval second = 1",
        "val first = second\nval second = first",
    ];
    let mut results = Vec::new();
    for prefix in cases {
        let text = format!("{prefix}\nfun separate(): Int {{ val n = 1; return n.fooBar() }}");
        with_types(&text, |sources, file, single, unit, source| {
            assert!(single.diagnostics().is_empty());
            assert!(unit.diagnostics().is_empty());
            assert!(unit.clone().validate().is_ok());
            let forward = expression(sources, file, "second");
            assert_eq!(
                single
                    .expression_type(forward)
                    .and_then(|ty| single.types().get(ty)),
                Some(&TypeKind::Deferred(DeferredReason::ForwardValueType)),
            );
            assert_eq!(
                unit.expression_type(UnitExpressionId::new(source, forward))
                    .and_then(|ty| unit.types().get(ty)),
                Some(&UnitTypeKind::Deferred(DeferredReason::ForwardValueType)),
            );
        });
        let (sources, single, unit) = complete_both(&text, false);
        eprintln!("initializer boundary {prefix:?}: single={single:?}; unit={unit:?}");
        results.push((text, sources, single, unit));
    }
    for (text, sources, single, unit) in results {
        let forward = text.find("second").unwrap();
        let independent = text.find("fooBar").unwrap();
        let expected = [
            ("L0165", forward, forward + 6, "second"),
            ("L0080", independent, independent + 6, "fooBar"),
        ];
        assert_exact(&sources, "main.ko", single, &expected);
        assert_exact(&sources, "main.ko", unit, &expected);
    }
}

#[test]
fn selected_constant_qualifiers_do_not_hide_runtime_member_roots() {
    use lang_frontend::name_resolution::{Namespace, UnitReferenceTarget};
    let cases = [
        (
            "package app\nimport lib.Labels\nfun value(): String = Labels.TEXT + lib.Labels.TEXT",
            Vec::new(),
        ),
        (
            "package app\nfun bad(): String = (lib.Labels.TEXT).fooBar()\nfun other(): Int { val n = 1; return n.fooBar() }",
            vec!["fooBar", "fooBar"],
        ),
        (
            "package app\nfun bad(): String { val lib = \"x\"; return lib.TEXT }\nfun value(): String = lib.Labels.TEXT",
            vec!["lib.TEXT"],
        ),
    ];
    let mut results = Vec::new();
    for (text, roots) in cases {
        let mut sources = SourceMap::new();
        let (app, app_file) = parsed(&mut sources, "app.ko", text);
        let (lib, lib_file) = parsed(
            &mut sources,
            "lib.ko",
            "package lib\nobject Labels { const val TEXT = \"value\" }",
        );
        let inputs = [
            SourceUnitInput::new("root", "app/main.ko", app, &app_file),
            SourceUnitInput::new("root", "lib/labels.ko", lib, &lib_file),
        ];
        let (environment, types) = standard_environments();
        for selected in [inputs, [inputs[1], inputs[0]]] {
            let index = index_compilation_unit(&sources, &selected).unwrap();
            let names = resolve_compilation_unit_names(&sources, &selected, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &selected, &names, &types).unwrap();
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            let unit = names
                .names()
                .index()
                .source_units()
                .iter()
                .find(|unit| unit.source_id() == app)
                .unwrap()
                .id();
            let constant =
                UnitExpressionId::new(unit, expression(&sources, &app_file, "lib.Labels.TEXT"));
            let usage = typed
                .constants()
                .expect("real constant facts")
                .uses()
                .iter()
                .find(|usage| usage.expression() == constant)
                .expect("selected qualified constant use");
            assert_eq!(
                typed.types().get(usage.ty()),
                Some(&UnitTypeKind::Builtin(BuiltinType::String))
            );
            let start = text.find("lib.Labels.TEXT").unwrap();
            let package_span = sources.span(app, start, start + 3).unwrap();
            let type_span = sources.span(app, start + 4, start + 10).unwrap();
            assert!(
                names
                    .names()
                    .references()
                    .iter()
                    .any(|r| r.span() == package_span
                        && matches!(r.target(), UnitReferenceTarget::Package(_)))
            );
            assert!(
                names
                    .names()
                    .references()
                    .iter()
                    .any(|r| r.span() == type_span
                        && r.namespace() == Some(Namespace::Type)
                        && matches!(
                            r.target(),
                            UnitReferenceTarget::Declaration(_) | UnitReferenceTarget::Symbol(_)
                        ))
            );
            if let Some(start) = text.find("lib.TEXT") {
                let runtime = sources.span(app, start, start + 3).unwrap();
                assert!(
                    names
                        .names()
                        .references()
                        .iter()
                        .any(|r| r.span() == runtime
                            && r.namespace() == Some(Namespace::Value)
                            && matches!(r.target(), UnitReferenceTarget::Symbol(_)))
                );
            }
            let result =
                complete_compilation_unit_types(&sources, &selected, &names, &types, &typed)
                    .map(|_| ());
            eprintln!("constant qualifier {text:?}: {result:?}");
            results.push((text, roots.clone(), app, result));
        }
    }
    for (text, roots, app, result) in results {
        if roots.is_empty() {
            result.expect("selected package/type qualifiers carry no runtime value obligation");
            continue;
        }
        let diagnostics = diagnostics(result);
        assert_eq!(diagnostics.len(), roots.len(), "{diagnostics:?}");
        let mut search = 0;
        for (diagnostic, context) in diagnostics.iter().zip(roots) {
            let start = search + text[search..].find(context).unwrap();
            let (start, width) = if context == "lib.TEXT" {
                (start + 4, 4)
            } else {
                (start, 6)
            };
            assert_eq!(diagnostic.code().to_string(), "L0080");
            let span = diagnostic.primary_span();
            assert_eq!(
                (span.source_id(), span.start(), span.end()),
                (app, start, start + width)
            );
            search = start + width;
        }
    }
}

#[test]
fn deferred_user_message_preserves_source_identity() {
    let (sources, single, unit) = complete_both(
        include_str!("../fixtures/final-type-admission-0319/call_to_deferred.ko"),
        false,
    );
    for result in [single, unit] {
        let diagnostics = diagnostics(result);
        assert_eq!(diagnostics.len(), 1);
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.code().to_string(), "L0165");
        assert_eq!(
            diagnostic.severity(),
            lang_frontend::diagnostic::Severity::Error
        );
        let span = diagnostic.primary_span();
        assert_eq!(sources.source_name(span.source_id()), Ok("main.ko"));
        assert_eq!((span.start(), span.end()), (30, 36));
        assert_eq!(sources.slice(span), Ok("1 to 2"));
        assert_eq!(
            diagnostic.message(),
            "type checking could not complete: the call's type is unresolved"
        );
        assert!(diagnostic.details().is_empty());
    }
}
