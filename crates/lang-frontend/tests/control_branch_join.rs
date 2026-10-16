//! SPEC-0326 first source-only batch. No fact mutation or artifact execution.
use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail},
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::parse_file,
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CompilationUnitTypes, DeferredReason, FinalTypeAdmissionError, TypeId,
        TypeKind, TypedFile, UnitExpressionId, UnitTypeId, UnitTypeKind, UnitTypeRefId,
        check_compilation_unit_types, check_types, complete_compilation_unit_types,
        complete_file_types, standard_environments,
    },
};

#[derive(Debug, PartialEq, Eq)]
enum Shape {
    Builtin(BuiltinType),
    Nullable(Box<Shape>),
    Deferred(DeferredReason),
    Other(String),
}

#[derive(Debug)]
struct Observation {
    mode: &'static str,
    root: String,
    shape: Shape,
    nodes: Vec<(String, Shape)>,
    node_spans: Vec<(String, Span)>,
    matches_type_refs: Vec<String>,
    raw: Vec<Diagnostic>,
    primary_text: Vec<String>,
    complete: Result<(), FinalTypeAdmissionError>,
}

fn single_shape(typed: &TypedFile, id: TypeId) -> Shape {
    match typed.types().get(id).expect("single type") {
        TypeKind::Builtin(kind) => Shape::Builtin(*kind),
        TypeKind::Deferred(reason) => Shape::Deferred(*reason),
        TypeKind::Nullable(inner) => Shape::Nullable(Box::new(single_shape(typed, *inner))),
        other => Shape::Other(format!("{other:?}")),
    }
}

fn unit_shape(typed: &CompilationUnitTypes, id: UnitTypeId) -> Shape {
    match typed.types().get(id).expect("unit type") {
        UnitTypeKind::Builtin(kind) => Shape::Builtin(*kind),
        UnitTypeKind::Deferred(reason) => Shape::Deferred(*reason),
        UnitTypeKind::Nullable(inner) => Shape::Nullable(Box::new(unit_shape(typed, *inner))),
        other => Shape::Other(format!("{other:?}")),
    }
}

fn observe(root: &str) -> Vec<Observation> {
    let text = format!(
        "fun test(flag: Boolean, optional: Int?, wide: Long): Unit {{ val result = {root} }}"
    );
    observe_text(&text, root)
}

fn observe_text(text: &str, root: &str) -> Vec<Observation> {
    let mut sources = SourceMap::new();
    let source = sources.add_source("main.ko", text).expect("source");
    let lexed = lex(&sources, source).expect("lex");
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let file = parse_file(&sources, &lexed).expect("parse");
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let ids: Vec<_> = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| (sources.slice(node.span()) == Ok(root)).then_some(id))
        .collect();
    assert_eq!(ids.len(), 1, "unique root {root}");
    let id = ids[0];
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).expect("single names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &file, &names, &types).expect("single types");
    let raw = typed.diagnostics().to_vec();
    eprintln!(
        "SPEC-0326 single type refs: {:?}",
        file.ast()
            .type_refs()
            .iter()
            .map(|(id, node)| (
                sources.slice(node.span()).unwrap(),
                typed.type_ref_type(id).map(|ty| single_shape(&typed, ty))
            ))
            .collect::<Vec<_>>()
    );
    let single = Observation {
        mode: "single",
        root: root.to_owned(),
        shape: single_shape(&typed, typed.expression_type(id).expect("single root type")),
        primary_text: raw
            .iter()
            .map(|d| sources.slice(d.primary_span()).unwrap().to_owned())
            .collect(),
        complete: complete_file_types(&sources, &file, &names, &types, &typed).map(|_| ()),
        nodes: file
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                typed.expression_type(id).map(|ty| {
                    (
                        sources.slice(node.span()).unwrap().to_owned(),
                        single_shape(&typed, ty),
                    )
                })
            })
            .collect(),
        matches_type_refs: file
            .ast()
            .type_refs()
            .iter()
            .filter_map(|(type_ref, node)| {
                (typed.type_ref_type(type_ref) == typed.expression_type(id))
                    .then(|| sources.slice(node.span()).unwrap().to_owned())
            })
            .collect(),
        node_spans: file
            .ast()
            .expressions()
            .iter()
            .map(|(_, node)| (sources.slice(node.span()).unwrap().to_owned(), node.span()))
            .collect(),
        raw: raw.clone(),
    };
    assert_eq!(typed.diagnostics(), raw);
    eprintln!("SPEC-0326 {single:?}");
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &file)];
    let index = index_compilation_unit(&sources, &inputs).expect("unit index");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .expect("unit names")
        .validate()
        .expect("valid unit names");
    let unit = names.names().index().source_units()[0].id();
    let typed =
        check_compilation_unit_types(&sources, &inputs, &names, &types).expect("unit types");
    let raw = typed.diagnostics().to_vec();
    eprintln!(
        "SPEC-0326 unit type refs: {:?}",
        file.ast()
            .type_refs()
            .iter()
            .map(|(id, node)| (
                sources.slice(node.span()).unwrap(),
                typed
                    .type_ref_type(UnitTypeRefId::new(unit, id))
                    .map(|ty| unit_shape(&typed, ty))
            ))
            .collect::<Vec<_>>()
    );
    let unit = Observation {
        mode: "unit",
        root: root.to_owned(),
        shape: unit_shape(
            &typed,
            typed
                .expression_type(UnitExpressionId::new(unit, id))
                .expect("unit root type"),
        ),
        primary_text: raw
            .iter()
            .map(|d| sources.slice(d.primary_span()).unwrap().to_owned())
            .collect(),
        complete: complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
            .map(|_| ()),
        nodes: file
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                typed
                    .expression_type(UnitExpressionId::new(unit, id))
                    .map(|ty| {
                        (
                            sources.slice(node.span()).unwrap().to_owned(),
                            unit_shape(&typed, ty),
                        )
                    })
            })
            .collect(),
        matches_type_refs: file
            .ast()
            .type_refs()
            .iter()
            .filter_map(|(type_ref, node)| {
                (typed.type_ref_type(UnitTypeRefId::new(unit, type_ref))
                    == typed.expression_type(UnitExpressionId::new(unit, id)))
                .then(|| sources.slice(node.span()).unwrap().to_owned())
            })
            .collect(),
        node_spans: file
            .ast()
            .expressions()
            .iter()
            .map(|(_, node)| (sources.slice(node.span()).unwrap().to_owned(), node.span()))
            .collect(),
        raw: raw.clone(),
    };
    assert_eq!(typed.diagnostics(), raw);
    eprintln!("SPEC-0326 {unit:?}");
    vec![single, unit]
}

fn roots(left: &str, right: &str) -> [String; 2] {
    [
        format!("if (flag) {left} else {right}"),
        format!("when (flag) {{ true -> {left}; false -> {right} }}"),
    ]
}

#[test]
fn known_mixed_value_branches_reject_implicit_any() {
    let mut observations: Vec<_> = ["\"text\"", "wide"]
        .into_iter()
        .flat_map(|right| roots("1", right))
        .flat_map(|root| observe(&root))
        .collect();
    for (prefix, parameters) in [
        (
            "enum class E { A }\nenum class F { B }\n",
            "left: E, right: F",
        ),
        ("", "left: List<Int>, right: List<Long>"),
    ] {
        for root in roots("left", "right") {
            observations.extend(observe_text(
                &format!(
                    "{prefix}fun test(flag: Boolean, {parameters}): Unit {{ val result = {root} }}"
                ),
                &root,
            ));
        }
    }
    for o in observations {
        let is_if = o.root.starts_with("if");
        let code = if is_if { "L0089" } else { "L0112" };
        assert_eq!(
            o.raw
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            [code],
            "{} {o:?}",
            o.mode
        );
        let expected_primary = if is_if {
            "else"
        } else {
            o.root
                .rsplit("-> ")
                .next()
                .expect("last branch")
                .trim_end_matches(" }")
        };
        assert_eq!(o.primary_text, [expected_primary]);
        match o.complete {
            Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => {
                assert_eq!(diagnostics, o.raw)
            }
            other => panic!("{} expected preserved diagnostics: {other:?}", o.mode),
        }
    }
}

#[test]
fn same_type_and_bottom_branches_keep_exact_result() {
    let mut observations: Vec<_> = [
        ("1", "2"),
        ("error(\"stop\")", "2"),
        ("2", "error(\"stop\")"),
    ]
    .into_iter()
    .flat_map(|(left, right)| roots(left, right))
    .flat_map(|root| observe(&root))
    .map(|o| (BuiltinType::Int, o))
    .collect();
    for root in roots("error(\"left\")", "error(\"right\")") {
        observations.extend(
            observe(&root)
                .into_iter()
                .map(|o| (BuiltinType::Nothing, o)),
        );
    }
    for root in roots("return 1", "return 2") {
        observations.extend(
            observe_text(
                &format!("fun test(flag: Boolean): Int {{ return {root} }}"),
                &root,
            )
            .into_iter()
            .map(|o| (BuiltinType::Nothing, o)),
        );
    }
    let declaration_boundary: Vec<_> = roots("return 1", "return 2")
        .into_iter()
        .flat_map(|root| {
            observe_text(
                &format!("fun test(flag: Boolean): Int {{ val result = {root} }}"),
                &root,
            )
        })
        .collect();
    for (expected, o) in observations {
        assert_eq!(o.shape, Shape::Builtin(expected), "{} {o:?}", o.mode);
        assert!(o.raw.is_empty(), "{o:?}");
        assert!(o.complete.is_ok(), "{o:?}");
    }
    for o in declaration_boundary {
        assert_eq!(o.shape, Shape::Builtin(BuiltinType::Nothing), "{o:?}");
        if o.mode == "single" {
            assert_eq!(
                o.raw
                    .iter()
                    .map(|d| d.code().to_string())
                    .collect::<Vec<_>>(),
                ["L0088"],
                "{o:?}"
            );
            assert_eq!(o.primary_text, ["}"]);
            match o.complete {
                Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => {
                    assert_eq!(diagnostics, o.raw)
                }
                other => panic!("expected tracked declaration-flow gap: {other:?}"),
            }
        } else {
            assert!(o.raw.is_empty() && o.complete.is_ok(), "{o:?}");
        }
    }
}

#[test]
fn null_and_nullable_branches_use_canonical_result() {
    let mut observations: Vec<_> = [
        ("null", "2"),
        ("2", "null"),
        ("optional", "2"),
        ("2", "optional"),
        ("optional", "null"),
        ("null", "optional"),
    ]
    .into_iter()
    .flat_map(|(left, right)| roots(left, right))
    .flat_map(|root| observe(&root))
    .map(|o| (BuiltinType::Int, o))
    .collect();
    observations.extend(
        roots("null", "null")
            .into_iter()
            .flat_map(|root| observe(&root))
            .map(|o| (BuiltinType::Nothing, o)),
    );
    for root in roots("(null)", "2")
        .into_iter()
        .chain(roots("{ null }", "2"))
    {
        observations.extend(observe(&root).into_iter().map(|o| (BuiltinType::Int, o)));
    }
    let unrelated_null: Vec<_> = roots("{ val independent = null; null }", "2")
        .into_iter()
        .flat_map(|root| observe(&root))
        .collect();
    let standalone = observe("null");
    for (expected, o) in observations {
        assert_eq!(
            o.shape,
            Shape::Nullable(Box::new(Shape::Builtin(expected))),
            "{} {o:?}",
            o.mode
        );
        assert!(o.raw.is_empty(), "{o:?}");
        assert!(o.complete.is_ok(), "{o:?}");
    }
    for o in standalone {
        assert_eq!(
            o.raw
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            ["L0083"],
            "{o:?}"
        );
        assert_eq!(o.primary_text, ["null"]);
        match o.complete {
            Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => {
                assert_eq!(diagnostics, o.raw)
            }
            other => panic!("expected standalone null diagnostic: {other:?}"),
        }
    }
    for o in unrelated_null {
        assert_eq!(
            o.shape,
            Shape::Nullable(Box::new(Shape::Builtin(BuiltinType::Int))),
            "{o:?}"
        );
        assert_eq!(
            o.raw
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            ["L0083"],
            "{o:?}"
        );
        let independent = o
            .node_spans
            .iter()
            .filter(|(text, _)| text == "null")
            .map(|(_, span)| *span)
            .min_by_key(|span| span.start())
            .expect("independent initializer null");
        assert_eq!(o.raw[0].primary_span(), independent);
        match o.complete {
            Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => {
                assert_eq!(diagnostics, o.raw)
            }
            other => panic!("expected independent declaration null rejection: {other:?}"),
        }
    }
}

#[test]
fn same_enum_instance_branches_join_to_root() {
    let expressions = roots("E.A", "E.B").into_iter().chain(roots("E.A", "base"));
    let mut observations: Vec<_> = expressions.flat_map(|root| {
        let text = format!("enum class E {{ A, B }}\nfun test(flag: Boolean, base: E): Unit {{ val witness = base; val result = {root} }}");
        observe_text(&text, &root).into_iter().map(|o| ("E", o))
    }).collect();
    for (root, expected, input_type) in [
        (
            "when (e) { is E.A<Int> -> e; is E.B<Int> -> e }",
            "E<Int>",
            "E<Int>",
        ),
        (
            "when (e) { is E.A<Int> -> e; is E.B<Int> -> anchor }",
            "E<Int>",
            "E<Int>",
        ),
        (
            "when (e) { null -> optional; is E.A<Int> -> e; is E.B<Int> -> e }",
            "E<Int>?",
            "E<Int>?",
        ),
        (
            "when (e) { is E.A<Int> -> e; is E.B<Int> -> e; null -> optional }",
            "E<Int>?",
            "E<Int>?",
        ),
    ] {
        let text = format!(
            "enum class E<T> {{ A(value: T), B(value: T) }}\nfun test(e: {input_type}, anchor: E<Int>, optional: E<Int>?): Unit {{ val result = {root} }}"
        );
        observations.extend(observe_text(&text, root).into_iter().map(|o| (expected, o)));
    }
    for (root, expected) in [
        ("if (flag) e else anchor", "E<Int>"),
        ("if (flag) anchor else e", "E<Int>"),
        ("if (flag) e else optional", "E<Int>?"),
        ("if (flag) optional else e", "E<Int>?"),
    ] {
        let text = format!(
            "enum class E<T> {{ A(value: T), B(value: T) }}\nfun test(flag: Boolean, e: E<Int>, anchor: E<Int>, optional: E<Int>?): Unit {{ if (e is E.A<Int>) {{ val result = {root} }} }}"
        );
        observations.extend(observe_text(&text, root).into_iter().map(|o| (expected, o)));
    }
    for (expected, o) in observations {
        assert!(
            o.matches_type_refs.iter().any(|text| text == expected),
            "{} expected {expected}: {o:?}",
            o.mode
        );
        assert!(o.raw.is_empty(), "{o:?}");
        assert!(o.complete.is_ok(), "{o:?}");
    }
}

#[test]
fn expected_types_keep_original_mismatch_diagnostics() {
    let mut observations: Vec<_> = roots("1", "\"text\"")
        .into_iter()
        .flat_map(|root| {
            observe_text(
                &format!("fun test(flag: Boolean): Unit {{ val result: Int = {root} }}"),
                &root,
            )
        })
        .collect();
    for root in roots("1", "\"text\"") {
        observations.extend(observe_text(
            &format!("fun test(flag: Boolean): Int {{ return {root} }}"),
            &root,
        ));
        observations.extend(observe_text(
            &format!(
                "fun take(value: Int): Unit {{}}\nfun test(flag: Boolean): Unit {{ take({root}) }}"
            ),
            &root,
        ));
    }
    for o in observations {
        assert_eq!(
            o.raw
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            ["L0084"],
            "{o:?}"
        );
        assert_eq!(o.primary_text, ["\"text\""]);
        match o.complete {
            Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => {
                assert_eq!(diagnostics, o.raw)
            }
            other => panic!("expected original mismatch: {other:?}"),
        }
    }
    assert_runtime_any_annotations_remain_rejected();
}

#[test]
fn statement_and_value_contexts_remain_distinct() {
    let when = "when (flag) { true -> 1; false -> \"text\" }";
    let with_else = "if (flag) 1 else \"text\"";
    let without_else = "if (flag) 1";
    let cases = [(when, true), (with_else, false), (without_else, true)];
    let observations: Vec<_> = cases
        .into_iter()
        .flat_map(|(root, ok)| {
            observe_text(
                &format!("fun test(flag: Boolean): Unit {{ {root}; println(\"done\") }}"),
                root,
            )
            .into_iter()
            .map(move |o| (ok, o))
        })
        .collect();
    let lambda_values: Vec<_> = roots("1", "\"text\"")
        .into_iter()
        .flat_map(|root| {
            observe_text(
                &format!("fun test(flag: Boolean): Unit {{ val callback = {{ {root} }} }}"),
                &root,
            )
        })
        .collect();
    for (ok, o) in observations {
        if ok {
            assert_eq!(o.shape, Shape::Builtin(BuiltinType::Unit), "{o:?}");
            assert!(o.raw.is_empty() && o.complete.is_ok(), "{o:?}");
        } else {
            assert_eq!(
                o.raw
                    .iter()
                    .map(|d| d.code().to_string())
                    .collect::<Vec<_>>(),
                ["L0089"],
                "{o:?}"
            );
            assert!(matches!(
                o.complete,
                Err(FinalTypeAdmissionError::Diagnostics(_))
            ));
        }
    }
    for o in lambda_values {
        let code = if o.root.starts_with("if") {
            "L0089"
        } else {
            "L0112"
        };
        assert_eq!(
            o.raw
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            [code],
            "{o:?}"
        );
        match o.complete {
            Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => {
                assert_eq!(diagnostics, o.raw)
            }
            other => panic!("expected value-context conflict: {other:?}"),
        }
    }
}

#[test]
fn error_and_deferred_branches_preserve_original_roots() {
    let deferred: Vec<_> = [("r?", "2"), ("2", "r?"), ("r?", "error(\"stop\")"), ("error(\"stop\")", "r?"), ("r?", "r?")].into_iter().flat_map(|(left, right)| roots(left, right)).flat_map(|root| {
        observe_text(&format!("enum class Result<T, E> {{ Ok(success: T), Err(error: E) }}\nfun test(flag: Boolean, r: Result<Int, Int>): Result<Int, Int> {{ val result = {root}; return r }}"), &root)
    }).collect();
    let invalid: Vec<_> = roots("optional?", "2")
        .into_iter()
        .flat_map(|root| observe(&root))
        .collect();
    let errors: Vec<_> = roots("1 + true", "2")
        .into_iter()
        .flat_map(|root| observe(&root))
        .collect();
    for o in deferred {
        let leaves: Vec<_> = o
            .node_spans
            .iter()
            .filter(|(text, _)| text == "r?")
            .map(|(_, span)| *span)
            .collect();
        assert!(!leaves.is_empty());
        assert!(
            o.nodes
                .iter()
                .filter(|(text, _)| text == "r?")
                .all(|(_, shape)| *shape == Shape::Deferred(DeferredReason::ErrorPropagation)),
            "{o:?}"
        );
        assert_eq!(
            o.shape,
            Shape::Deferred(DeferredReason::ErrorPropagation),
            "{o:?}"
        );
        assert!(o.raw.is_empty(), "{o:?}");
        match o.complete {
            Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => {
                let mut actual_spans: Vec<_> =
                    diagnostics.iter().map(|d| d.primary_span()).collect();
                let mut expected_spans = leaves.clone();
                actual_spans.sort_by_key(|span| (span.start(), span.end()));
                expected_spans.sort_by_key(|span| (span.start(), span.end()));
                assert_eq!(actual_spans, expected_spans);
                for d in diagnostics {
                    assert_eq!(d.code().to_string(), "L0165");
                    assert!(leaves.contains(&d.primary_span()), "{d:?}");
                }
            }
            other => panic!("expected deferred source gate: {other:?}"),
        }
    }
    for o in invalid {
        assert!(o.raw.is_empty(), "{o:?}");
        match o.complete {
            Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => {
                assert_eq!(diagnostics.len(), 1);
                assert_eq!(diagnostics[0].code().to_string(), "L0085");
                let leaf = o
                    .node_spans
                    .iter()
                    .find(|(text, _)| text == "optional?")
                    .expect("invalid leaf")
                    .1;
                assert_eq!(diagnostics[0].primary_span().end(), leaf.end());
                assert_eq!(diagnostics[0].primary_span().len(), 1);
            }
            other => panic!("expected illegal operand source diagnostic: {other:?}"),
        }
    }
    for o in errors {
        assert_eq!(
            o.raw
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            ["L0085"],
            "{o:?}"
        );
        match o.complete {
            Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => {
                assert_eq!(diagnostics, o.raw)
            }
            other => panic!("expected preserved original error: {other:?}"),
        }
    }
}

#[test]
fn when_conflict_labels_track_actual_participating_branch() {
    let roots = [
        "when { flag -> error(\"stop\"); other -> 1; else -> \"text\" }",
        "when { flag -> 1 + true; other -> 1; else -> \"text\" }",
    ];
    let observations: Vec<_> = roots
        .into_iter()
        .flat_map(|root| {
            observe_text(
                &format!("fun test(flag: Boolean, other: Boolean): Unit {{ val result = {root} }}"),
                root,
            )
        })
        .collect();
    for o in observations {
        let joins: Vec<_> = o
            .raw
            .iter()
            .filter(|d| d.code().to_string() == "L0112")
            .collect();
        assert_eq!(joins.len(), 1, "{o:?}");
        let expected_primary = o
            .node_spans
            .iter()
            .find(|(text, _)| text == "\"text\"")
            .expect("conflicting branch")
            .1;
        let expected_label = o
            .node_spans
            .iter()
            .filter(|(text, _)| text == "1")
            .map(|(_, span)| *span)
            .max_by_key(|span| span.start())
            .expect("actual prior Int branch");
        assert_eq!(joins[0].primary_span(), expected_primary);
        let labels: Vec<_> = joins[0]
            .details()
            .iter()
            .filter_map(|detail| match detail {
                DiagnosticDetail::Label(label) => Some(label.span()),
                _ => None,
            })
            .collect();
        assert_eq!(labels, [expected_label]);
    }
}

#[test]
fn nullable_case_joins_deliver_the_same_enum_instance() {
    let mut observations = Vec::new();
    for root in roots("null", "e").into_iter().chain(roots("e", "null")) {
        let text = format!(
            "enum class E<T> {{ A(value: T), B(value: T) }}\nfun test(flag: Boolean, e: E<Int>): Unit {{ if (e is E.A<Int>) {{ val result = {root}; val delivered: E<Int>? = result }} }}"
        );
        observations.extend(observe_text(&text, &root));
    }
    let mut incompatible = Vec::new();
    for root in roots("other", "e").into_iter().chain(roots("e", "other")) {
        let text = format!(
            "enum class E<T> {{ A(value: T), B(value: T) }}\nfun test(flag: Boolean, e: E<Int>, other: E<Long>?): Unit {{ if (e is E.A<Int>) {{ val result = {root} }} }}"
        );
        incompatible.extend(observe_text(&text, &root));
    }
    for o in incompatible {
        let expected = if o.root.starts_with("if") {
            "L0089"
        } else {
            "L0112"
        };
        assert_eq!(
            o.raw
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            [expected],
            "{o:?}"
        );
        match o.complete {
            Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => {
                assert_eq!(diagnostics, o.raw)
            }
            other => panic!("expected different enum instance rejection: {other:?}"),
        }
    }
    for o in observations {
        assert!(
            o.nodes.iter().any(|(text, shape)| text == "e"
                && matches!(shape, Shape::Other(kind) if kind.starts_with("EnumCase"))),
            "leaf smart cast must remain: {o:?}"
        );
        assert!(
            o.matches_type_refs.iter().any(|text| text == "E<Int>?"),
            "{o:?}"
        );
        assert!(o.raw.is_empty(), "{o:?}");
        assert!(o.complete.is_ok(), "{o:?}");
    }
}

// Recorded entry-specific Any rejection; this does not enable an Any value representation.
fn assert_runtime_any_annotations_remain_rejected() {
    let observations: Vec<_> = roots("1", "\"text\"")
        .into_iter()
        .chain(roots("1", "2"))
        .flat_map(|root| observe_text(&format!("fun mixed(flag: Boolean): Any = {root}"), &root))
        .collect();
    for o in observations {
        let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) = &o.complete else {
            panic!("runtime Any must remain rejected: {o:?}");
        };
        if o.mode == "single" {
            assert_eq!(diagnostics[0].code().to_string(), "L0165", "{o:?}");
            assert_eq!(
                (
                    diagnostics[0].primary_span().start(),
                    diagnostics[0].primary_span().end()
                ),
                (26, 29)
            );
            assert_eq!(&diagnostics[1..], o.raw);
        } else {
            assert_eq!(
                o.raw
                    .iter()
                    .map(|d| d.code().to_string())
                    .collect::<Vec<_>>(),
                ["L0084", "L0084"],
                "{o:?}"
            );
            assert_eq!(diagnostics, &o.raw);
        }
    }
}
