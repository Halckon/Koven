//! Expression diagnostics, source identity and nested-owner recovery contracts.
use super::*;

#[test]
fn parser_diagnostic_codes_messages_and_primary_spans_are_stable() {
    let cases = [
        ("", "L0009", "expected expression", 0, 0),
        ("(", "L0009", "expected expression", 1, 1),
        ("a.", "L0011", "expected member or reference name", 2, 2),
        (
            "a < b <= c",
            "L0012",
            "non-associative operator chain",
            6,
            8,
        ),
        ("a b", "L0013", "unexpected trailing token", 2, 3),
        ("a as 4", "L0014", "expected type reference", 5, 6),
        ("a++b", "L0015", "unsupported operator", 1, 3),
    ];

    for (text, code, message, start, end) in cases {
        let diagnostics = parse_fingerprints(text);
        assert!(
            diagnostics.iter().any(|actual| {
                actual
                    == &(
                        code.to_owned(),
                        Severity::Error,
                        message.to_owned(),
                        start,
                        end,
                    )
            }),
            "missing expected diagnostic for {text:?}: {diagnostics:?}"
        );
    }
}

#[test]
fn all_non_associative_groups_point_at_the_second_operator() {
    for (text, start, end) in [
        ("a..b..<c", 4, 7),
        ("a in b is T", 7, 9),
        ("a < b >= c", 6, 8),
        ("a == b != c", 7, 9),
    ] {
        let diagnostics = parse_fingerprints(text);
        assert!(
            diagnostics
                .iter()
                .any(|actual| { actual.0 == "L0012" && actual.3 == start && actual.4 == end })
        );
    }
    assert_parses("(a < b) < c");
}

#[test]
fn unsupported_operator_combinations_use_the_whole_adjacent_span() {
    for (text, start, end) in [
        ("a++b", 1, 3),
        ("a--b", 1, 3),
        ("a<<b", 1, 3),
        ("a>>b", 1, 3),
        ("a...b", 1, 4),
    ] {
        let diagnostics = parse_fingerprints(text);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|actual| actual.0 == "L0015")
                .map(|actual| (actual.3, actual.4))
                .collect::<Vec<_>>(),
            [(start, end)]
        );
    }
}

#[test]
fn call_argument_forms_are_contextual_and_grouped_assignment_remains_legal() {
    for text in [
        "f(name = input)",
        "f(input)",
        "f(&input)",
        "f(name = borrow(input))",
        "f(name = &input)",
        "f((a = b))",
    ] {
        assert_parses(text);
    }
}

#[test]
fn call_argument_recovery_respects_strings_nested_delimiters_and_eof() {
    let text = r#"f(name = "${inner(a, b)}", next)"#;
    let (_sources, parsed) = parsed_case_with_diagnostics(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let Expression::Call { arguments, .. } = expression(&parsed, parsed.root()) else {
        panic!("expected call")
    };
    assert_eq!(arguments.len(), 2);
    assert!(matches!(
        expression(&parsed, arguments[1].value),
        Expression::Name
    ));

    for nested in ["f(borrow (x), y)", "f(name = g(x), y)", "f(&(a = b), y)"] {
        let (_sources, parsed) = parsed_case_with_diagnostics(nested);
        assert!(
            parsed.diagnostics().is_empty(),
            "{nested:?}: {:?}",
            parsed.diagnostics()
        );
        let Expression::Call { arguments, .. } = expression(&parsed, parsed.root()) else {
            panic!("{nested:?} must retain its outer call")
        };
        assert_eq!(arguments.len(), 2, "{nested:?}");
    }

    for malformed in ["f(name =", "f(&", "!+1(name ="] {
        let mut sources = SourceMap::new();
        let source_id = add_source(&mut sources, "argument-eof.ko", malformed);
        let parsed = parse_expression_twice(&sources, source_id, malformed);
        assert!(
            parsed
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0033"),
            "{malformed:?}: {:?}",
            parsed.diagnostics()
        );
    }

    let interpolation = r#""${f(name =)}tail""#;
    let (_sources, parsed) = parsed_case_with_diagnostics(interpolation);
    assert!(
        parsed
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0033")
    );
    let root = parsed.ast().expressions().get(parsed.root()).expect("root");
    assert_eq!(root.span().end(), interpolation.len());

    let newline_string = "f(name = \"a\n, next)";
    let (_sources, parsed) = parsed_case_with_diagnostics(newline_string);
    assert!(
        parsed
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0004")
    );
    let Expression::Call { arguments, .. } = expression(&parsed, parsed.root()) else {
        panic!("newline recovery must retain the outer call")
    };
    assert_eq!(arguments.len(), 2);
    assert!(matches!(
        expression(&parsed, arguments[1].value),
        Expression::Name
    ));
}

#[test]
fn invalid_string_lexeme_is_preserved_as_an_error_part() {
    assert_terminal_string_escape_suppresses_only_its_own_missing_closer();

    let text = r#""a\qz""#;
    let (_sources, parsed) = parsed_case_with_diagnostics(text);
    let Expression::String { parts } = expression(&parsed, parsed.root()) else {
        panic!("expected string")
    };
    assert!(matches!(parts.as_slice(), [
        StringPart::Text(before),
        StringPart::Error(error),
        StringPart::Text(after),
    ] if (before.start(), before.end()) == (1, 2)
        && (error.start(), error.end()) == (2, 4)
        && (after.start(), after.end()) == (4, 5)));
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0006"]
    );
}

fn assert_terminal_string_escape_suppresses_only_its_own_missing_closer() {
    for text in ["\"abc\\", "\"${ \"abc\\"] {
        let (_sources, parsed) = parsed_case_with_diagnostics(text);
        assert_eq!(
            parsed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect::<Vec<_>>(),
            ["L0006"],
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
    }

    for text in ["(\"abc\\", "f(\"abc\\", "a[\"abc\\", "((\"abc\\"] {
        let (_sources, parsed) = parsed_case_with_diagnostics(text);
        assert!(
            parsed
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0010"),
            "an enclosing source delimiter remains an independent parser error: {text:?}: {:?}",
            parsed.diagnostics()
        );
    }

    let independent_tail = "\"abc\\\nnext";
    let (_sources, parsed) = parsed_case_with_diagnostics(independent_tail);
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0006", "L0013"]
    );

    for text in ["x as move /*", "x as (/*", "x as ('a"] {
        let (_sources, parsed) = parsed_case_with_diagnostics(text);
        assert_eq!(
            parsed
                .diagnostics()
                .iter()
                .filter(|diagnostic| diagnostic.code().to_string() == "L0014")
                .count(),
            0,
            "lexer poison must not be reclassified as a missing type: {text:?}: {:?}",
            parsed.diagnostics()
        );
    }

    for text in [
        "x as \"abc",
        "x as \"${a",
        "x as A<\"abc",
        "x as () -> \"abc",
        "x as \"abc\\",
        "x as A<\"abc\\",
        "x as () -> \"abc\\",
        "x as \"${\"closed\" + a",
        "x as \"${\"closed\"}tail${a",
        "x as \"${\"closed\"}tail\\",
        "x as \"${\"inner\n}tail\\",
        "x as \"${\"inner\n + a",
        "x as \"${\"inner\\\n}tail\\",
        "x as \"${\"inner\\",
        "x as \"${\"inner",
    ] {
        let (_sources, parsed) = parsed_case_with_diagnostics(text);
        assert!(
            parsed.diagnostics().iter().all(|diagnostic| !matches!(
                diagnostic.code().to_string().as_str(),
                "L0014" | "L0013"
            )),
            "segmented lexer poison must be one TypeRef error region: {text:?}: {:?}",
            parsed.diagnostics()
        );
    }
}

#[test]
fn lexer_poison_is_not_duplicated_and_independent_parser_errors_survive() {
    let poison_only = parse_fingerprints("async");
    assert_eq!(poison_only.len(), 1);
    assert_eq!(poison_only[0].0, "L0002");

    let mixed = parse_fingerprints("async trailing");
    assert_eq!(
        mixed
            .iter()
            .map(|entry| entry.0.as_str())
            .collect::<Vec<_>>(),
        ["L0002", "L0013"]
    );
}

#[test]
fn closing_delimiter_diagnostic_carries_the_opening_label() {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "closer.ko", "(a");
    let parsed = parse_expression_twice(&sources, source_id, "missing closer");
    let diagnostic = parsed
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code().to_string() == "L0010")
        .expect("a missing closer must be diagnosed");
    assert_eq!(diagnostic.message(), "expected closing delimiter");
    assert_eq!(
        (
            diagnostic.primary_span().start(),
            diagnostic.primary_span().end()
        ),
        (2, 2)
    );
    assert!(diagnostic.details().iter().any(|detail| {
        matches!(detail, DiagnosticDetail::Label(label) if (label.span().start(), label.span().end()) == (0, 1))
    }));
}

#[test]
fn parsing_rejects_lexed_files_from_another_source_map() {
    let mut origin = SourceMap::new();
    let origin_id = add_source(&mut origin, "origin.ko", "x");
    let lexed = lex_parser_source_twice(&origin, origin_id, "foreign expression source");
    let foreign = SourceMap::new();
    assert_parser_error_twice(
        &foreign,
        &lexed,
        ParserInternalError::Source(SourceError::InvalidSourceId {
            source_id: origin_id,
        }),
        "foreign expression source",
        parse_expression,
    );
}

#[test]
fn diagnostics_are_deterministic_across_source_loading_order_and_repeated_runs() {
    let text = "a < b <= c trailing";
    assert_eq!(parse_fingerprints(text), parse_fingerprints(text));

    let mut sources = SourceMap::new();
    let _noise = add_source(&mut sources, "noise.ko", "noise");
    let source_id = add_source(&mut sources, "case.ko", text);
    let parsed = parse_expression_twice(&sources, source_id, "diagnostics after noise source");
    let after_noise = parsed
        .diagnostics()
        .iter()
        .map(diagnostic_fingerprint)
        .map(|(code, severity, message, start, end)| {
            (code, severity, message.to_owned(), start, end)
        })
        .collect::<Vec<_>>();
    assert_eq!(parse_fingerprints(text), after_noise);
}

#[test]
fn stop_tokens_produce_empty_expected_node_spans() {
    for (text, code, offset) in [("a[]", "L0009", 2), ("x as T<>", "L0014", 7)] {
        let diagnostics = parse_fingerprints(text);
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.0 == code && diagnostic.3 == offset && diagnostic.4 == offset
            }),
            "{text:?}: {diagnostics:?}"
        );
    }
}

#[test]
fn unterminated_interpolation_does_not_duplicate_the_lexer_root_cause() {
    let diagnostics = parse_fingerprints(r#""${a"#);
    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.0.as_str())
            .collect::<Vec<_>>(),
        ["L0005"]
    );
}

#[test]
fn interpolation_trailing_input_keeps_the_string_and_does_not_invent_a_closer_error() {
    let text = r#""${a b}""#;
    let (sources, parsed) = parsed_case_with_diagnostics(text);
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0013"]
    );
    let diagnostic = &parsed.diagnostics()[0];
    let b = text.find('b').expect("b exists");
    assert_eq!(
        (
            diagnostic.primary_span().start(),
            diagnostic.primary_span().end()
        ),
        (b, b + 1)
    );
    let root = parsed.ast().expressions().get(parsed.root()).expect("root");
    assert_eq!((root.span().start(), root.span().end()), (0, text.len()));
    assert_eq!(sources.slice(root.span()).expect("span"), text);
    assert!(matches!(root.payload(), Expression::String { .. }));
}

#[test]
fn contextual_move_type_name_reports_following_type_as_trailing_input() {
    let text = "x as move T";
    let (sources, parsed) = parsed_case_with_diagnostics(text);
    assert_eq!(parsed.diagnostics().len(), 1, "{:?}", parsed.diagnostics());
    assert_eq!(parsed.diagnostics()[0].code().to_string(), "L0013");
    assert_eq!(
        sources
            .slice(parsed.diagnostics()[0].primary_span())
            .unwrap(),
        "T"
    );
    assert!(matches!(
        expression(&parsed, parsed.root()),
        Expression::Error
    ));
    let type_ref = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(_, node)| match node.payload() {
            Expression::Cast { type_ref, .. } => Some(*type_ref),
            _ => None,
        })
        .expect("completed cast remains in the error region");
    let name = parsed.ast().type_refs().get(type_ref).expect("type name");
    assert!(matches!(name.payload(), TypeRef::Qualified { .. }));
    assert_eq!(sources.slice(name.span()).unwrap(), "move");
}

#[test]
fn reserved_words_in_name_and_type_positions_remain_single_lexer_poison_regions() {
    for text in ["a.async()", "a::async", "x as A<pkg.async>"] {
        let (_sources, parsed) = parsed_case_with_diagnostics(text);
        assert_eq!(
            parsed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect::<Vec<_>>(),
            ["L0002"],
            "{text:?}"
        );
        assert!(
            parsed
                .ast()
                .expressions()
                .iter()
                .any(|(_, node)| matches!(node.payload(), Expression::Error))
                || parsed
                    .ast()
                    .type_refs()
                    .iter()
                    .any(|(_, node)| matches!(node.payload(), TypeRef::Error)),
            "{text:?} must preserve poison as an explicit error node"
        );
        if text == "a.async()" {
            assert!(
                !matches!(expression(&parsed, parsed.root()), Expression::Call { .. }),
                "poison member must not acquire a call suffix"
            );
        }
    }
}

#[test]
fn non_associative_recovery_keeps_an_explicit_error_region() {
    let text = "a<b<c==d";
    let (_sources, parsed) = parsed_case_with_diagnostics(text);
    let second_less = text.rfind('<').expect("second less");
    assert!(
        parsed
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0012"
                && diagnostic.primary_span().start() == second_less)
    );
    assert!(parsed.ast().expressions().iter().any(|(_, node)| matches!(
        node.payload(),
        Expression::Error
    ) && node.span().start()
        == second_less));
}

#[test]
fn incomplete_named_argument_keeps_call_span_at_eof() {
    let text = "f(name =";
    let (_sources, parsed) = parsed_case_with_diagnostics(text);
    let root = parsed.ast().expressions().get(parsed.root()).expect("root");
    assert!(matches!(root.payload(), Expression::Call { .. }));
    assert_eq!((root.span().start(), root.span().end()), (0, text.len()));
    assert!(
        parsed
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0033")
    );
    assert!(
        parsed
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0010")
    );
}

#[test]
fn missing_member_name_keeps_dot_in_root_recovery_span() {
    let text = "a.";
    let (_sources, parsed) = parsed_case_with_diagnostics(text);
    let root = parsed.ast().expressions().get(parsed.root()).expect("root");
    assert!(matches!(root.payload(), Expression::Error));
    assert_eq!((root.span().start(), root.span().end()), (0, text.len()));
}

#[test]
fn delimiter_stops_and_recovery_spans_do_not_cross_their_owner() {
    for text in ["f(", "a[", "(a", "x as A<", "x is () ->"] {
        let (_sources, parsed) = parsed_case_with_diagnostics(text);
        assert!(!parsed.diagnostics().is_empty(), "{text:?} must fail");
        for diagnostic in parsed.diagnostics() {
            assert!(
                diagnostic.primary_span().end() <= text.len(),
                "{text:?}: {diagnostic:?}"
            );
        }
        let root = parsed.ast().expressions().get(parsed.root()).expect("root");
        assert_eq!(root.span().end(), text.len(), "{text:?}");
    }
}

#[test]
fn poison_in_trailing_and_nested_type_positions_is_not_reclassified() {
    for text in ["a async", "x as A<B<async>>", "a.async tail"] {
        let (_sources, parsed) = parsed_case_with_diagnostics(text);
        let codes = parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            codes.iter().filter(|code| code.as_str() == "L0002").count(),
            1,
            "{text:?}: {codes:?}"
        );
        assert!(
            !codes.iter().any(|code| code == "L0009"
                || code == "L0010"
                || code == "L0011"
                || code == "L0014"),
            "{text:?}: {codes:?}"
        );
    }
}

#[test]
fn call_and_index_recovery_spans_include_every_consumed_suffix_byte() {
    for text in ["f(a,", "a[x, y]", "a[]", "f(a, b", "a[x"] {
        let (_sources, parsed) = parsed_case_with_diagnostics(text);
        let root = parsed.ast().expressions().get(parsed.root()).expect("root");
        assert_eq!(
            (root.span().start(), root.span().end()),
            (0, text.len()),
            "{text:?}"
        );
    }
}

#[test]
fn nested_interpolation_consumes_its_own_delimiters_before_outer_string_end() {
    let text = r#""head${f(a[1 + (b * c)], "${d}")}tail""#;
    let (_sources, parsed) = parsed_case(text);
    let root = parsed.ast().expressions().get(parsed.root()).expect("root");
    assert_eq!((root.span().start(), root.span().end()), (0, text.len()));
    let Expression::String { parts } = root.payload() else {
        panic!("string")
    };
    let interpolation = parts
        .iter()
        .find_map(|part| match part {
            StringPart::Interpolation { span, expression } => Some((*span, *expression)),
            StringPart::Text(_) | StringPart::Error(_) => None,
        })
        .expect("interpolation");
    assert_eq!(interpolation.0.end(), text.rfind("tail").expect("tail"));
    assert!(matches!(
        expression(&parsed, interpolation.1),
        Expression::Call { .. }
    ));
}

#[test]
fn lexical_poison_postfix_recovery_preserves_nested_interpolation_owners() {
    let text = r#""前${ # ('界', "内${x}")}后""#;
    let (_sources, parsed) = parsed_case_with_diagnostics(text);
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0001"]
    );
    let root = parsed.ast().expressions().get(parsed.root()).expect("root");
    assert_eq!((root.span().start(), root.span().end()), (0, text.len()));
    assert!(matches!(root.payload(), Expression::String { .. }));
}
