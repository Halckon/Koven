//! SPEC-0210: constant type eligibility follows ordinary type diagnostics.
use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

fn check(texts: [&str; 2], expected_codes: &[&str], expected_span: Option<&str>) {
    check_with_spans(texts, expected_codes, |sources, diagnostics| {
        if let Some(span) = expected_span {
            // A helper top-level val can produce L0167 before the diagnostic under test.
            let diagnostic = diagnostics
                .iter()
                .find(|d| d.code().to_string() != "L0167")
                .expect("diagnostic under test");
            assert_eq!(sources.slice(diagnostic.primary_span()).unwrap(), span);
        }
    });
}

fn check_with_spans(
    texts: [&str; 2],
    expected_codes: &[&str],
    check_spans: impl Fn(&SourceMap, &[Diagnostic]),
) {
    let mut sources = SourceMap::new();
    let a = sources.add_source("a.ko", texts[0]).unwrap();
    let b = sources.add_source("b.ko", texts[1]).unwrap();
    let fa = parse_file(&sources, &lex(&sources, a).unwrap()).unwrap();
    let fb = parse_file(&sources, &lex(&sources, b).unwrap()).unwrap();
    assert!(fa.diagnostics().is_empty(), "{:?}", fa.diagnostics());
    assert!(fb.diagnostics().is_empty(), "{:?}", fb.diagnostics());
    let inputs = [
        SourceUnitInput::new("root", "a/source.ko", a, &fa),
        SourceUnitInput::new("root", "b/source.ko", b, &fb),
    ];
    let (ne, te) = standard_environments();
    let mut previous = None;
    for inputs in [inputs, [inputs[1], inputs[0]]] {
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &te).unwrap();
        assert_eq!(
            typed
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            expected_codes
        );
        check_spans(&sources, typed.diagnostics());
        let snapshot = (
            typed.types().clone(),
            typed.expression_types().clone(),
            typed.diagnostics().to_vec(),
        );
        if let Some(previous) = &previous {
            assert_eq!(&snapshot, previous);
        }
        previous = Some(snapshot);
        if !expected_codes.is_empty() {
            assert!(typed.constants().is_none());
            let rejected = typed
                .validate_constants()
                .expect_err("invalid constant unit");
            assert!(rejected.validate().is_err());
        }
    }
}

#[test]
fn explicit_constant_types_are_closed_in_each_namespace() {
    for (ty, value) in [
        ("Int?", "1"),
        ("Double", "1.5"),
        ("Float", "1.5f"),
        ("() -> Int", "{ 1 }"),
    ] {
        for body in [
            format!("const val VALUE: {ty} = {value}"),
            format!("object A {{ const val VALUE: {ty} = {value} }}"),
            format!("class A {{ companion {{ const val VALUE: {ty} = {value} }} }}"),
        ] {
            check(
                [
                    &format!("package a\n{body}"),
                    "package b\nfun unused(): Unit {}",
                ],
                &["L0155"],
                Some(ty),
            );
        }
    }
}

#[test]
fn inferred_constant_types_and_mismatches_keep_precise_diagnostics() {
    check(
        [
            "package a\nobject A { const val VALUE = 1.5 }",
            "package b\nfun unused(): Unit {}",
        ],
        &["L0155"],
        Some("1.5"),
    );
    for ty in ["Int", "Double"] {
        check(
            [
                &format!("package a\nobject A {{ const val VALUE: {ty} = true }}"),
                "package b\nfun unused(): Unit {}",
            ],
            &["L0084"],
            None,
        );
    }
}

#[test]
fn ordinary_top_level_variables_report_only_the_runtime_value_rule() {
    // Guide v0.46: top-level runtime values are L0167, never constant-type errors.
    check(
        [
            "package a\nval sample: Double = 1.5",
            "package b\nval sample: Int? = 1",
        ],
        &["L0167", "L0167"],
        None,
    );
}

#[test]
fn invalid_nested_signature_types_do_not_add_constant_diagnostics() {
    check(
        [
            "package a\nconst val VALUE: Array<Int<String>>? = null",
            "package b\nfun unused(): Unit {}",
        ],
        &["L0082"],
        None,
    );
}

#[test]
fn closed_scalar_types_remain_available() {
    for (ty, literal) in [
        ("Boolean", "true"),
        ("Byte", "1"),
        ("Short", "1"),
        ("Int", "1"),
        ("Long", "1L"),
        ("UByte", "1u"),
        ("UShort", "1u"),
        ("UInt", "1u"),
        ("ULong", "1uL"),
        ("Char", "'文'"),
        ("String", "\"中\""),
    ] {
        check(
            [
                &format!("package a\nobject A {{ const val VALUE: {ty} = {literal} }}"),
                "package b\nfun unused(): Unit {}",
            ],
            &[],
            None,
        );
    }
}

#[test]
fn invalid_cross_file_dependency_does_not_repeat_type_qualification() {
    for (ty, literal) in [("Double", "1.5"), ("Int?", "1")] {
        check(
            [
                "package a\nimport b.B\nobject A { const val VALUE = B.VALUE }",
                &format!("package b\nobject B {{ const val VALUE: {ty} = {literal} }}"),
            ],
            &["L0155"],
            Some(ty),
        );
    }
}

#[test]
fn invalid_signature_dependency_keeps_only_the_upstream_error() {
    check(
        [
            "package a\nimport b.B\nobject A { const val COPY = B.BAD }",
            "package b\nobject B { const val BAD: Array<Int<String>>? = null }",
        ],
        &["L0082"],
        None,
    );
}

#[test]
fn runtime_expressions_are_rejected_even_in_short_circuit_rhs() {
    for (expression, span) in [
        ("compute()", "compute()"),
        ("false && compute()", "compute()"),
        ("ordinary", "ordinary"),
        ("true == false", "true == false"),
        ("if (true) true else false", "if (true) true else false"),
    ] {
        check(
            [
                &format!(
                    "package a\nfun compute(): Boolean = true\nval ordinary: Boolean = true\nobject A {{ const val BAD = {expression} }}"
                ),
                "package b\nfun unused(): Unit {}",
            ],
            &["L0167", "L0156"],
            Some(span),
        );
    }
}

#[test]
fn expression_qualification_keeps_ordinary_operand_errors() {
    check(
        [
            "package a\nobject A { const val BAD = 1 && true }",
            "package b\nfun unused(): Unit {}",
        ],
        &["L0085"],
        None,
    );
}

#[test]
fn invalid_expression_dependencies_do_not_seed_cycle_or_duplicate_diagnostics() {
    check(
        [
            "package a\nimport b.B\nobject A { const val BAD: Boolean = false && B.BAD }",
            "package b\nimport a.A\nfun compute(): Boolean = true\nobject B { const val BAD: Boolean = A.BAD && compute() }",
        ],
        &["L0156"],
        Some("compute()"),
    );
}

#[test]
fn forward_operand_types_precede_expression_qualification() {
    check(
        [
            "package a\nimport b.B\nobject A { const val BAD = B.FLAG == B.FLAG }",
            "package b\nobject B { const val FLAG = true }",
        ],
        &["L0156"],
        Some("B.FLAG == B.FLAG"),
    );
    check(
        [
            "package a\nimport b.B\nobject A { const val BAD = B.FLAG == true }",
            "package b\nobject B { const val FLAG = 1 }",
        ],
        &["L0085"],
        None,
    );
}

#[test]
fn cross_file_constant_arithmetic_reports_only_evaluated_failures() {
    for (expression, expected) in [
        ("B.ZERO + 2147483647 + 1", vec!["L0158"]),
        ("1 / B.ZERO", vec!["L0158"]),
        ("false && (1 / B.ZERO == 0)", vec![]),
    ] {
        check(
            [
                &format!("package a\nimport b.B\nobject A {{ const val RESULT = {expression} }}"),
                "package b\nobject B { const val ZERO = 0 }",
            ],
            &expected,
            None,
        );
    }
}

#[test]
fn unit_evaluator_preserves_widths_and_cross_file_values() {
    for (provider, consumer, expected) in [
        (
            "const val X: Byte = 127\nconst val ONE: Byte = 1",
            "const val RESULT: Byte = B.X + B.ONE",
            vec!["L0158"],
        ),
        (
            "const val X: Long = -9223372036854775808L",
            "const val RESULT = B.X / -1L",
            vec!["L0158"],
        ),
        (
            "const val X = 20 + 22",
            "const val RESULT = 1 / (B.X - 42)",
            vec!["L0158"],
        ),
        (
            "const val X = \"中\" + \"文\"",
            "const val RESULT = (B.X == \"中文\") && (1 / 0 == 0)",
            vec!["L0158"],
        ),
        (
            "const val X: ULong = 18446744073709551615uL",
            "const val RESULT = B.X - 1uL",
            vec![],
        ),
    ] {
        check(
            [
                &format!("package a\nimport b.B\nobject A {{ {consumer} }}"),
                &format!("package b\nobject B {{ {provider} }}"),
            ],
            &expected,
            None,
        );
    }
}

#[test]
fn failed_constant_evaluation_does_not_cascade_to_dependents() {
    check(
        [
            "package a\nimport b.B\nobject A { const val RESULT = B.BAD + 1 }",
            "package b\nobject B { const val BAD = 1 / 0 }",
        ],
        &["L0158"],
        Some("/"),
    );
}

#[test]
fn unrelated_signature_errors_do_not_hide_constant_cycles_or_arithmetic_errors() {
    for (constants, expected, primary) in [
        (
            "const val FIRST: Int = SECOND\nconst val SECOND: Int = FIRST",
            "L0157",
            "FIRST",
        ),
        ("const val BROKEN: Int = 1 / 0", "L0158", "/"),
    ] {
        check(
            [
                &format!("package a\n{constants}"),
                "package b\nfun invalid(value: List<Nothing>): Unit {}",
            ],
            &[expected, "L0125"],
            Some(primary),
        );
    }
}

#[test]
fn invalid_constant_signatures_do_not_seed_dependency_cycles() {
    check(
        [
            "package a\nconst val FIRST: List<Nothing> = SECOND\nconst val SECOND: List<Nothing> = FIRST",
            "package b\nfun unused(): Unit {}",
        ],
        &["L0125", "L0125"],
        Some("Nothing"),
    );
}

#[test]
fn invalid_signature_dependency_preserves_independent_operand_error() {
    check(
        [
            "package a\nimport b.B\nobject A { const val COPY = (1 && true) && B.BAD }",
            "package b\nobject B { const val BAD: Array<Int<String>>? = null }",
        ],
        &["L0085", "L0082"],
        Some("&&"),
    );
}

#[test]
fn invalid_signature_dependency_preserves_private_constant_diagnostic() {
    check(
        [
            "package a\nimport b.B\nconst val COPY = B.BAD",
            "package b\nobject B { private const val BAD: List<Nothing>? = null }",
        ],
        &["L0154", "L0125"],
        Some("BAD"),
    );
}

#[test]
fn failed_constant_evaluation_preserves_independent_operand_error() {
    check(
        [
            "package a\nconst val BROKEN: Int = 1 / 0\nconst val COPY = (1 && true) && (BROKEN == 0)",
            "package b\nfun unused(): Unit {}",
        ],
        &["L0158", "L0085"],
        Some("/"),
    );
}

#[test]
fn failed_evaluation_does_not_hide_independent_constant_qualification() {
    for consumer in [
        "const val COPY = false && ((true == false) || (FORWARD == 0))\nconst val FORWARD = B.BAD + 1",
        "const val FIRST: Boolean = SECOND || ((true == false) && (B.BAD == 0))\nconst val SECOND: Boolean = FIRST",
    ] {
        check(
            [
                &format!("package a\nimport b.B\n{consumer}"),
                "package b\nobject B { const val BAD: Int = 1 / 0 }",
            ],
            &["L0156", "L0158"],
            Some("true == false"),
        );
    }
}

#[test]
fn failed_evaluation_still_blocks_dependent_evaluation_and_cycle_diagnostics() {
    check(
        [
            "package a\nimport b.B\nconst val DOWNSTREAM = 2 % 0 + FORWARD\nconst val FORWARD = B.BAD + 1\nconst val FIRST: Int = SECOND + B.BAD\nconst val SECOND: Int = FIRST",
            "package b\nobject B { const val BAD: Int = 1 / 0 }",
        ],
        &["L0158"],
        Some("/"),
    );
}

fn check_two_sources(texts: [&str; 2], code: &str, fragments: [&str; 2]) {
    check_with_spans(texts, &[code, code], |sources, diagnostics| {
        for ((diagnostic, file), (text, fragment)) in diagnostics
            .iter()
            .zip(["a.ko", "b.ko"])
            .zip(texts.into_iter().zip(fragments))
        {
            let span = diagnostic.primary_span();
            let start = text.find(fragment).expect("source fragment");
            assert_eq!(sources.source_name(span.source_id()), Ok(file));
            assert_eq!((span.start(), span.end()), (start, start + fragment.len()));
            assert_eq!(sources.slice(span), Ok(fragment));
        }
    });
}

#[test]
fn invalid_dependency_does_not_hide_an_independent_qualification_error() {
    for (prefix, expression, invalid) in [
        ("", "false && ((true == false) || B.BAD)", "true == false"),
        ("", "B.BAD == true", "B.BAD == true"),
        ("", "true == B.BAD", "true == B.BAD"),
        (
            "const val FORWARD = B.BAD\n",
            "FORWARD == true",
            "FORWARD == true",
        ),
    ] {
        check_two_sources(
            [
                &format!("package a\nimport b.B\n{prefix}const val COPY: Boolean = {expression}"),
                "package b\nfun runtime(value: Boolean): Boolean = value\nobject B { const val BAD: Boolean = runtime(true) }",
            ],
            "L0156",
            [invalid, "runtime(true)"],
        );
    }
}

#[test]
fn invalid_dependency_does_not_create_a_derived_qualification_error() {
    check(
        [
            "package a\nimport b.B\nconst val COPY: Boolean = B.BAD == B.BAD",
            "package b\nobject B { const val BAD: Double = 1.5 }",
        ],
        &["L0155"],
        Some("Double"),
    );
}

#[test]
fn invalid_dependency_preserves_independent_explicit_type_qualification() {
    check_two_sources(
        [
            "package a\nimport b.B\nconst val COPY: Double = B.BAD",
            "package b\nobject B { const val BAD: Double = 1.5 }",
        ],
        "L0155",
        ["Double", "Double"],
    );
}

#[test]
fn invalid_cyclic_provider_does_not_cascade_into_downstream_qualification() {
    check(
        [
            "package a\nimport b.B\nconst val COPY: Boolean = B.BAD == B.BAD",
            "package b\nobject B { const val BAD: Int? = BAD }",
        ],
        &["L0155"],
        Some("Int?"),
    );
    check(
        [
            "package a\nobject B { const val BAD: Int? = BAD }",
            "package b\nimport a.B\nconst val COPY: Boolean = B.BAD == B.BAD",
        ],
        &["L0155"],
        Some("Int?"),
    );
}

#[test]
fn cyclic_provider_with_an_allowed_type_still_reports_its_cycle() {
    check(
        [
            "package a\nimport b.B\nconst val COPY: Boolean = B.VALUE == B.VALUE",
            "package b\nobject B { const val VALUE: Int = VALUE }",
        ],
        &["L0157"],
        Some("VALUE"),
    );
}

#[test]
fn rejected_annotation_does_not_preempt_an_ordinary_initializer_error() {
    check(
        [
            "package a\nimport b.B\nconst val COPY: Boolean = B.BAD == B.BAD",
            "package b\nobject B { const val BAD: Int? = BAD + true }",
        ],
        &["L0085"],
        Some("+"),
    );
}

#[test]
fn downstream_qualification_waits_for_the_cyclic_provider() {
    for (provider, consumer) in [("a", "b"), ("b", "a")] {
        let consumer_text = format!(
            "package {consumer}\nimport {provider}.B\nconst val FORWARD = B.BAD\nconst val COPY: Boolean = FORWARD == FORWARD"
        );
        for (body, code, span) in [
            ("const val BAD: Boolean = BAD + true", "L0085", "+"),
            ("const val BAD: Boolean = BAD == BAD", "L0156", "BAD == BAD"),
            (
                "const val BAD: Boolean = NEXT + true\nconst val NEXT: Boolean = BAD",
                "L0085",
                "+",
            ),
        ] {
            let provider_text = format!("package {provider}\nobject B {{ {body} }}");
            let texts = if provider == "a" {
                [provider_text.as_str(), consumer_text.as_str()]
            } else {
                [consumer_text.as_str(), provider_text.as_str()]
            };
            check(texts, &[code], Some(span));
        }
    }
}

#[test]
fn downstream_qualification_sees_types_stabilized_inside_a_healthy_cycle() {
    check(
        [
            "package a\nimport b.B\nconst val COPY = B.OTHER == B.OTHER",
            "package b\nobject B { const val VALUE: Boolean = OTHER\nconst val OTHER = VALUE }",
        ],
        &["L0156", "L0157"],
        Some("B.OTHER == B.OTHER"),
    );
}

#[test]
fn cyclic_component_checks_ordinary_errors_before_qualification() {
    for (provider, consumer) in [("a", "b"), ("b", "a")] {
        let provider_text = format!(
            "package {provider}\nimport {consumer}.A\nobject B {{ const val BAD: Boolean = BAD + A.COPY }}"
        );
        for (expression, independent) in [("B.BAD == B.BAD", false), ("B.BAD == true", true)] {
            let consumer_text = format!(
                "package {consumer}\nimport {provider}.B\nobject A {{ const val COPY: Boolean = {expression} }}"
            );
            let provider_first = provider == "a";
            let texts = if provider_first {
                [provider_text.as_str(), consumer_text.as_str()]
            } else {
                [consumer_text.as_str(), provider_text.as_str()]
            };
            let (codes, first_span): (&[_], _) = match (independent, provider_first) {
                (false, _) => (&["L0085"], "+"),
                (true, true) => (&["L0085", "L0156"], "+"),
                (true, false) => (&["L0156", "L0085"], expression),
            };
            check(texts, codes, Some(first_span));
        }
    }
}

#[test]
fn cyclic_component_preserves_only_independent_qualification_when_available() {
    for (provider, consumer) in [("a", "b"), ("b", "a")] {
        let provider_text = format!(
            "package {provider}\nimport {consumer}.A\nobject B {{ const val BAD: Boolean = A.COPY == true }}"
        );
        let consumer_text = format!(
            "package {consumer}\nimport {provider}.B\nobject A {{ const val COPY: Boolean = B.BAD == B.BAD }}"
        );
        let texts = if provider == "a" {
            [provider_text.as_str(), consumer_text.as_str()]
        } else {
            [consumer_text.as_str(), provider_text.as_str()]
        };
        check(texts, &["L0156"], Some("A.COPY == true"));
    }
}

#[test]
fn mutually_dependent_qualification_failures_keep_a_primary_diagnostic() {
    check(
        [
            "package a\nimport b.B\nobject A { const val COPY: Boolean = B.BAD == B.BAD }",
            "package b\nimport a.A\nobject B { const val BAD: Boolean = A.COPY == A.COPY }",
        ],
        &["L0156"],
        Some("B.BAD == B.BAD"),
    );
}
