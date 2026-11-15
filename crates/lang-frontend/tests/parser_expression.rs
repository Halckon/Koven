//! SPEC-0007 / SPEC-0117 的公开表达式 Parser 契约测试。

#[path = "parser_expression/shapes.rs"]
mod shapes;

use lang_frontend::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, DiagnosticDetail, Severity},
    parser::{
        AssignmentOperator, BinaryOperator, CallArgument, CastOperator, Expression,
        FloatLiteralKind, IntegerLiteralKind, LiteralKind, ParsedExpression, ParserInternalError,
        PrefixOperator, StringPart, TypeRef, parse_expression,
    },
    source::{SourceError, SourceId, SourceMap},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{
    assert_parser_error_twice, lex_parser_source_twice, parse_expression_twice,
};

fn add_source(sources: &mut SourceMap, name: &str, text: &str) -> SourceId {
    sources
        .add_source(name, text)
        .expect("test source names must be unique")
}

fn diagnostic_fingerprint(diagnostic: &Diagnostic) -> (String, Severity, &str, usize, usize) {
    let span = diagnostic.primary_span();
    (
        diagnostic.code().to_string(),
        diagnostic.severity(),
        diagnostic.message(),
        span.start(),
        span.end(),
    )
}

fn parse_fingerprints(text: &str) -> Vec<(String, Severity, String, usize, usize)> {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "case.ko", text);
    let parsed = parse_expression_twice(&sources, source_id, text);
    parsed
        .diagnostics()
        .iter()
        .map(diagnostic_fingerprint)
        .map(|(code, severity, message, start, end)| {
            (code, severity, message.to_owned(), start, end)
        })
        .collect()
}

fn assert_parses(text: &str) {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "pass.ko", text);
    let parsed = parse_expression_twice(&sources, source_id, text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{text:?} produced diagnostics: {:?}",
        parsed.diagnostics()
    );
    let root = parsed
        .ast()
        .expressions()
        .get(parsed.root())
        .expect("the parser root must belong to its AST");
    assert_eq!(root.span().source_id(), source_id);
    assert_eq!((root.span().start(), root.span().end()), (0, text.len()));
}

fn parsed_case(text: &str) -> (SourceMap, ParsedExpression) {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "ast.ko", text);
    let parsed = parse_expression_twice(&sources, source_id, text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{text:?}: {:?}",
        parsed.diagnostics()
    );
    (sources, parsed)
}

fn expression(parsed: &ParsedExpression, id: ExpressionId) -> &Expression {
    parsed
        .ast()
        .expressions()
        .get(id)
        .expect("expression ID must resolve")
        .payload()
}

fn call_payload(
    parsed: &ParsedExpression,
    id: ExpressionId,
) -> (
    ExpressionId,
    &[lang_frontend::ast::TypeRefId],
    Option<lang_frontend::source::Span>,
    &[CallArgument],
) {
    let Expression::Call {
        callee,
        type_arguments,
        type_arguments_span,
        arguments,
    } = expression(parsed, id)
    else {
        panic!("expected call")
    };
    (*callee, type_arguments, *type_arguments_span, arguments)
}

type ExpressionPredicate = fn(&Expression) -> bool;

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

fn parsed_case_with_diagnostics(text: &str) -> (SourceMap, ParsedExpression) {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "red-team.ko", text);
    let parsed = parse_expression_twice(&sources, source_id, text);
    (sources, parsed)
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

#[test]
fn moderately_deep_prefix_nesting_does_not_overflow() {
    let prefix = format!("{}x", "!-".repeat(128));
    assert_parses(&prefix);
}

#[test]
fn moderately_deep_group_nesting_does_not_overflow() {
    let grouped = format!("{}x{}", "(".repeat(128), ")".repeat(128));
    assert_parses(&grouped);
}

#[test]
fn moderately_deep_generic_nesting_does_not_overflow() {
    let mut nested_type = "T".to_owned();
    for _ in 0..128 {
        nested_type = format!("A<{nested_type}>");
    }
    assert_parses(&format!("x as {nested_type}"));
}

#[test]
fn outer_delimiters_are_empty_boundaries_for_inner_recovery() {
    for (text, code, boundary) in [
        ("f((a, b)", "L0010", 4),
        ("f(a., b)", "L0011", 4),
        ("f(::, x)", "L0011", 4),
    ] {
        let (_sources, parsed) = parsed_case_with_diagnostics(text);
        assert!(
            parsed.diagnostics().iter().any(|diagnostic| {
                diagnostic.code().to_string() == code
                    && diagnostic.primary_span().start() == boundary
                    && diagnostic.primary_span().end() == boundary
            }),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
    }
}

#[test]
fn malformed_nested_index_recovery_stops_before_the_outer_argument_boundary() {
    let text = "f(a[x, y), z)";
    let (_sources, parsed) = parsed_case_with_diagnostics(text);
    let outer_comma = text.rfind(',').expect("outer comma");
    let root = parsed.ast().expressions().get(parsed.root()).expect("root");
    assert_eq!(root.span().end(), text.len());
    assert!(
        parsed.diagnostics().iter().all(|diagnostic| {
            diagnostic.primary_span().start() <= outer_comma
                || diagnostic.code().to_string() != "L0010"
        }),
        "recovery must not invent delimiter errors after the outer synchronization point: {:?}",
        parsed.diagnostics()
    );
}

#[test]
fn invalid_argument_recovery_stops_at_the_owning_call_boundary() {
    for text in ["f(@[x, y)", "f(@(x, y)"] {
        let (_sources, parsed) = parsed_case_with_diagnostics(text);
        let call = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(_, node)| match node.payload() {
                Expression::Call { arguments, .. } => Some((node.span(), arguments)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{text:?} must retain the outer call"));
        let (call_span, arguments) = call;
        assert_eq!(call_span.end(), text.len(), "{text:?}");
        assert_eq!(arguments.len(), 1, "{text:?}");
        assert!(matches!(
            expression(&parsed, arguments[0].value),
            Expression::Error
        ));
        assert_eq!(
            parsed
                .diagnostics()
                .iter()
                .filter(|diagnostic| diagnostic.code().to_string() == "L0033")
                .count(),
            1,
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
    }
}

#[test]
fn malformed_move_function_type_keeps_prefix_span_and_error_return() {
    // A real move function-type prefix includes the parameter-list opener. Bare `move` is a name.
    for text in ["x as move ()", "x as A<move (), B>"] {
        let (sources, parsed) = parsed_case_with_diagnostics(text);
        let move_start = text.find("move").expect("move");
        let function = parsed
            .ast()
            .type_refs()
            .iter()
            .find_map(|(_, node)| match node.payload() {
                TypeRef::Function {
                    move_span: Some(marker),
                    return_type,
                    ..
                } if node.span().start() == move_start => {
                    Some((node.span(), *marker, *return_type))
                }
                _ => None,
            })
            .expect("committed move prefix belongs to the recovered function type");
        assert_eq!(sources.slice(function.1).unwrap(), "move");
        assert_eq!(sources.slice(function.0).unwrap(), "move ()");
        assert!(matches!(
            parsed.ast().type_refs().get(function.2).unwrap().payload(),
            TypeRef::Error
        ));
        assert_eq!(
            parsed.diagnostics().len(),
            1,
            "{text}: {:?}",
            parsed.diagnostics()
        );
        assert_eq!(parsed.diagnostics()[0].code().to_string(), "L0014");
    }
}

#[test]
fn deeply_right_associative_assignment_and_elvis_parse_without_overflow() {
    let assignment = format!("{}z", "a=".repeat(128));
    let (_sources, parsed) = parsed_case(&assignment);
    let mut cursor = parsed.root();
    let mut depth = 0;
    while let Expression::Assignment { value, .. } = expression(&parsed, cursor) {
        depth += 1;
        cursor = *value;
    }
    assert_eq!(depth, 128);

    let elvis = format!("{}z", "a?:".repeat(128));
    assert_parses(&elvis);
}

#[test]
fn deeply_nested_string_interpolation_returns_a_complete_outer_string() {
    let mut text = "x".to_owned();
    for _ in 0..64 {
        text = format!("\"${{{text}}}\"");
    }
    let (_sources, parsed) = parsed_case(&text);
    let root = parsed.ast().expressions().get(parsed.root()).expect("root");
    assert!(matches!(root.payload(), Expression::String { .. }));
    assert_eq!((root.span().start(), root.span().end()), (0, text.len()));
}

#[test]
fn deeply_recursive_function_type_and_mixed_generic_type_parse() {
    let function_type = format!("{}T", "() -> ".repeat(128));
    assert_parses(&format!("x as {function_type}"));

    let mut mixed = "T".to_owned();
    for _ in 0..64 {
        mixed = format!("A<() -> B<{mixed}>>");
    }
    assert_parses(&format!("x is {mixed}"));
}

#[test]
fn large_flat_expression_keeps_all_lexemes_and_ast_nodes_deterministic() {
    let text = std::iter::repeat_n("x", 10_000)
        .collect::<Vec<_>>()
        .join("+");
    let (_sources, first) = parsed_case(&text);
    let (_sources, second) = parsed_case(&text);
    assert_eq!(first.ast().expressions().len(), 19_999);
    assert_eq!(
        first.ast().expressions().len(),
        second.ast().expressions().len()
    );
    assert_eq!(
        first
            .ast()
            .expressions()
            .iter()
            .map(|(_, node)| (node.span().start(), node.span().end()))
            .collect::<Vec<_>>(),
        second
            .ast()
            .expressions()
            .iter()
            .map(|(_, node)| (node.span().start(), node.span().end()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn malformed_index_inside_call_emits_one_bounded_closer_error() {
    let text = "f(a[x,y),z)";
    let (_sources, parsed) = parsed_case_with_diagnostics(text);
    let closer_errors = parsed
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.code().to_string() == "L0010")
        .collect::<Vec<_>>();
    assert_eq!(closer_errors.len(), 1, "{:?}", parsed.diagnostics());
    let outer_stop = text.find(')').expect("inner malformed closer");
    assert!(closer_errors[0].primary_span().end() <= outer_stop);
    assert!(parsed.ast().expressions().iter().any(|(_, node)| {
        matches!(node.payload(), Expression::Index { .. })
            && node.span().start() == text.find('a').unwrap()
            && node.span().end() <= outer_stop
    }));
}

#[test]
fn trailing_call_comma_preserves_an_empty_error_argument() {
    let text = "f(a,)";
    let (_sources, parsed) = parsed_case_with_diagnostics(text);
    let Expression::Call { arguments, .. } = expression(&parsed, parsed.root()) else {
        panic!("call")
    };
    assert_eq!(arguments.len(), 2);
    let error = parsed
        .ast()
        .expressions()
        .get(arguments[1].value)
        .expect("empty error argument");
    assert!(matches!(error.payload(), Expression::Error));
    let right_paren = text.find(')').expect("right paren");
    assert_eq!(
        (error.span().start(), error.span().end()),
        (right_paren, right_paren)
    );
}

#[test]
fn grouped_type_in_call_emits_one_empty_expected_type_error() {
    let text = "f(x as (A), y)";
    let (_sources, parsed) = parsed_case_with_diagnostics(text);
    let argument_stop = text.find(',').expect("outer argument stop");
    let errors = parsed
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.code().to_string() == "L0014")
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1, "{:?}", parsed.diagnostics());
    assert_eq!(
        (
            errors[0].primary_span().start(),
            errors[0].primary_span().end()
        ),
        (argument_stop, argument_stop)
    );
}

#[test]
fn recursion_budget_prevents_stack_amplification() {
    let prefix = format!("{}x", "!-".repeat(128));
    assert_parses(&prefix);

    let assignment = format!("{}z", "a=".repeat(256));
    assert_parses(&assignment);

    let grouped = format!("{}x{}", "(".repeat(256), ")".repeat(256));
    assert_parses(&grouped);

    let function_type = format!("{}T", "() -> ".repeat(256));
    assert_parses(&format!("x as {function_type}"));

    let unsupported_chain = format!("{}z", "a++".repeat(256));
    let diagnostics = parse_fingerprints(&unsupported_chain);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.0 == "L0015")
            .count(),
        256
    );

    let flat_unsupported = format!("{}z", "a++".repeat(1_100));
    let diagnostics = parse_fingerprints(&flat_unsupported);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.0 == "L0015")
            .count(),
        1_100
    );

    for (label, text) in [
        ("prefix", format!("{}x", "!-".repeat(600))),
        ("assignment", format!("{}z", "a=".repeat(1_100))),
        ("elvis", format!("{}z", "a?:".repeat(1_100))),
        (
            "group",
            format!("{}x{}", "(".repeat(1_100), ")".repeat(1_100)),
        ),
        (
            "generic",
            format!("x as {}T{}", "A<".repeat(1_100), ">".repeat(1_100)),
        ),
        ("function", format!("x as {}T", "() -> ".repeat(1_100))),
    ] {
        let mut sources = SourceMap::new();
        let source_id = add_source(&mut sources, "budget.ko", &text);
        let lexed = lex_parser_source_twice(&sources, source_id, label);
        assert_parser_error_twice(
            &sources,
            &lexed,
            ParserInternalError::NestingLimitExceeded { limit: 1024 },
            label,
            parse_expression,
        );
    }
}

#[test]
fn contextual_keywords_parse_as_ordinary_expressions_and_members() {
    for text in [
        "node.value",
        "player.move()",
        "cell.borrow()",
        "state.inout()",
        "item.own()",
        "control.loop()",
        "value + 1",
        "loop + 2",
        "move + 3",
        "borrow + 4",
        "inout + 5",
        "own + 6",
    ] {
        assert_parses(text);
    }
}

#[test]
fn named_bitwise_soft_operators_parse_with_correct_precedence_and_associativity() {
    let cases = [
        ("a shl b", BinaryOperator::Shl),
        ("a shr b", BinaryOperator::Shr),
        ("a ushr b", BinaryOperator::Ushr),
        ("a and b", BinaryOperator::BitwiseAnd),
        ("a xor b", BinaryOperator::BitwiseXor),
        ("a or b", BinaryOperator::BitwiseOr),
    ];
    for (text, expected_op) in cases {
        let (_sources, parsed) = parsed_case(text);
        let Expression::Binary { operator, .. } = expression(&parsed, parsed.root()) else {
            panic!("expected binary expression for {text}");
        };
        assert_eq!(*operator, expected_op, "operator mismatch for {text}");
    }

    // Precedence: multiplicative (15) > additive (14) > shift (13) > bitwise_and (12) > bitwise_xor (11) > bitwise_or (10) > range (9) > logical_and (3) > logical_or (2)
    let precedence_cases = [
        ("a + b shl c", BinaryOperator::Shl, BinaryOperator::Add),
        (
            "a shl b and c",
            BinaryOperator::BitwiseAnd,
            BinaryOperator::Shl,
        ),
        (
            "a and b xor c",
            BinaryOperator::BitwiseXor,
            BinaryOperator::BitwiseAnd,
        ),
        (
            "a xor b or c",
            BinaryOperator::BitwiseOr,
            BinaryOperator::BitwiseXor,
        ),
        (
            "a or b .. c",
            BinaryOperator::InclusiveRange,
            BinaryOperator::BitwiseOr,
        ),
        (
            "a or b && c",
            BinaryOperator::LogicalAnd,
            BinaryOperator::BitwiseOr,
        ),
        (
            "a && b || c",
            BinaryOperator::LogicalOr,
            BinaryOperator::LogicalAnd,
        ),
    ];
    for (text, root_op, left_op) in precedence_cases {
        let (_sources, parsed) = parsed_case(text);
        let Expression::Binary { operator, left, .. } = expression(&parsed, parsed.root()) else {
            panic!("expected binary root for {text}");
        };
        assert_eq!(*operator, root_op, "root operator mismatch for {text}");
        let Expression::Binary {
            operator: child_op, ..
        } = expression(&parsed, *left)
        else {
            panic!("expected binary left child for {text}");
        };
        assert_eq!(
            *child_op, left_op,
            "left child operator mismatch for {text}"
        );
    }

    // Associativity (left-associative)
    for (text, expected_op) in [
        ("a shl b shl c", BinaryOperator::Shl),
        ("a and b and c", BinaryOperator::BitwiseAnd),
        ("a xor b xor c", BinaryOperator::BitwiseXor),
        ("a or b or c", BinaryOperator::BitwiseOr),
    ] {
        let (_sources, parsed) = parsed_case(text);
        let Expression::Binary { operator, left, .. } = expression(&parsed, parsed.root()) else {
            panic!("expected binary root for {text}");
        };
        assert_eq!(*operator, expected_op);
        let Expression::Binary {
            operator: child_op, ..
        } = expression(&parsed, *left)
        else {
            panic!("expected binary left child for {text}");
        };
        assert_eq!(*child_op, expected_op);
    }

    // Contextual soft operators as ordinary expressions
    for text in [
        "shl.foo()",
        "shr(1, 2)",
        "ushr = 3",
        "and.bar",
        "xor(x)",
        "or + 1",
    ] {
        assert_parses(text);
    }
}
