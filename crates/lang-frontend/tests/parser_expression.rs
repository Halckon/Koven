//! SPEC-0007 的公开表达式 Parser 契约测试。

use lang_frontend::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, DiagnosticDetail, Severity},
    lexer::lex,
    parser::{
        AssignmentOperator, BinaryOperator, CallArgument, CastOperator, Expression, LiteralKind,
        ParsedExpression, ParserInternalError, PrefixOperator, StringPart, TypeRef,
        parse_expression,
    },
    source::{SourceId, SourceMap},
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
    let lexed = lex(&sources, source_id).expect("test source must lex");
    let parsed = parse_expression(&sources, &lexed).expect("test source identity must parse");
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
    let lexed = lex(&sources, source_id).expect("test source must lex");
    let parsed = parse_expression(&sources, &lexed).expect("test source identity must parse");
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
    let lexed = lex(&sources, source_id).expect("test source must lex");
    let parsed = parse_expression(&sources, &lexed).expect("test source identity must parse");
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
fn concrete_payloads_use_typed_ids_and_exact_composite_spans() {
    for (text, expected) in [
        ("name", Expression::Name),
        ("this", Expression::This),
        ("1", Expression::Literal(LiteralKind::Integer)),
        ("1.5", Expression::Literal(LiteralKind::Float)),
        ("'x'", Expression::Literal(LiteralKind::Char)),
        ("true", Expression::Literal(LiteralKind::Boolean(true))),
        ("null", Expression::Literal(LiteralKind::Null)),
    ] {
        let (_sources, parsed) = parsed_case(text);
        assert_eq!(expression(&parsed, parsed.root()), &expected);
    }

    let (_sources, parsed) = parsed_case("a!!.b(c)[i]::ref!!");
    let root = parsed.ast().expressions().get(parsed.root()).expect("root");
    assert_eq!((root.span().start(), root.span().end()), (0, 18));
    let Expression::NonNullAssert { operand, .. } = root.payload() else {
        panic!("expected !!")
    };
    let Expression::CallableReference {
        receiver: Some(receiver),
        name_span,
        ..
    } = expression(&parsed, *operand)
    else {
        panic!("expected bound reference")
    };
    assert_eq!((name_span.start(), name_span.end()), (13, 16));
    let Expression::Index { receiver, index } = expression(&parsed, *receiver) else {
        panic!("expected index")
    };
    assert!(matches!(expression(&parsed, *index), Expression::Name));
    let Expression::Call {
        callee, arguments, ..
    } = expression(&parsed, *receiver)
    else {
        panic!("expected call")
    };
    assert_eq!(arguments.len(), 1);
    let Expression::Member {
        receiver,
        safe,
        name_span,
        ..
    } = expression(&parsed, *callee)
    else {
        panic!("expected member")
    };
    assert!(!safe);
    assert_eq!((name_span.start(), name_span.end()), (4, 5));
    assert!(matches!(
        expression(&parsed, *receiver),
        Expression::NonNullAssert { .. }
    ));
}

#[test]
fn typed_and_basic_calls_expose_type_argument_ids_and_exact_spans() {
    for text in [
        "f<T>()",
        "obj.f<T>()",
        "(factory())<T>()",
        "factory()<T>()",
        "f<A<B<C>>>()",
        "f<T> /* comment */ ()",
    ] {
        let (sources, parsed) = parsed_case(text);
        let (_, type_arguments, type_arguments_span, arguments) =
            call_payload(&parsed, parsed.root());
        assert!(!type_arguments.is_empty(), "{text:?}");
        assert!(arguments.is_empty(), "{text:?}");
        let span = type_arguments_span.expect("typed call type-argument span");
        let expected_start = text.find('<').expect("opener");
        let expected_end = text.rfind('>').expect("closer") + 1;
        assert_eq!((span.start(), span.end()), (expected_start, expected_end));
        assert_eq!(
            sources.slice(span).expect("type arguments"),
            &text[expected_start..expected_end]
        );
        for argument in type_arguments {
            assert_eq!(
                parsed
                    .ast()
                    .type_refs()
                    .get(*argument)
                    .expect("typed ID")
                    .span()
                    .source_id(),
                parsed.source_id()
            );
        }
    }

    let (_, parsed) = parsed_case("f()");
    let (_, type_arguments, type_arguments_span, _) = call_payload(&parsed, parsed.root());
    assert!(type_arguments.is_empty());
    assert!(type_arguments_span.is_none());
}

#[test]
fn typed_call_trial_commits_only_the_complete_strict_suffix() {
    let (_, parsed) = parsed_case("a < b > (c)");
    let (callee, type_arguments, type_arguments_span, arguments) =
        call_payload(&parsed, parsed.root());
    assert!(matches!(expression(&parsed, callee), Expression::Name));
    assert_eq!(type_arguments.len(), 1);
    assert!(type_arguments_span.is_some());
    assert_eq!(arguments.len(), 1);

    for text in ["f<T>", "a < b > c", "f<T", "f<>()", "f<,T>()", "f<T,>()"] {
        let (_sources, parsed) = parsed_case_with_diagnostics(text);
        assert!(
            !matches!(expression(&parsed, parsed.root()), Expression::Call { type_arguments, .. } if !type_arguments.is_empty()),
            "{text:?} must not commit an incomplete strict trial"
        );
        assert_eq!(
            parsed.ast().type_refs().len(),
            0,
            "{text:?} trial must not leave TypeRef nodes"
        );
        assert!(
            parsed.diagnostics().iter().all(|diagnostic| {
                !matches!(
                    diagnostic.code().to_string().as_str(),
                    "L0024" | "L0025" | "L0026"
                )
            }),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
    }
}

#[test]
fn strict_trial_adversarial_families_do_not_leave_ast_or_diagnostics_state() {
    fn dense_no_match(levels: usize) -> String {
        format!(
            "f{}T{} + z",
            "< /* trivia */ A".repeat(levels),
            "> /* trivia */".repeat(levels)
        )
    }

    for levels in [64, 128] {
        let text = dense_no_match(levels);
        let (_sources, first) = parsed_case_with_diagnostics(&text);
        let (_sources, second) = parsed_case_with_diagnostics(&text);
        assert_eq!(first.ast().type_refs().len(), 0);
        assert_eq!(
            first.ast().expressions().len(),
            second.ast().expressions().len()
        );
        assert_eq!(
            first
                .diagnostics()
                .iter()
                .map(diagnostic_fingerprint)
                .collect::<Vec<_>>(),
            second
                .diagnostics()
                .iter()
                .map(diagnostic_fingerprint)
                .collect::<Vec<_>>()
        );
    }

    let mut mixed = String::new();
    for index in 0..128 {
        if index > 0 {
            mixed.push_str(" + ");
        }
        if index % 2 == 0 {
            mixed.push_str("f<A<B>>()");
        } else {
            mixed.push_str("f<A<B>>");
        }
    }
    let (_sources, parsed) = parsed_case_with_diagnostics(&mixed);
    assert_eq!(
        parsed.ast().type_refs().len(),
        128,
        "only the 64 committed calls may allocate two TypeRefs each"
    );
}

#[test]
fn an_error_receiver_does_not_steal_a_less_than_comparison_as_postfix() {
    let text = "@ < b";
    let (_sources, parsed) = parsed_case_with_diagnostics(text);
    assert!(
        matches!(
            expression(&parsed, parsed.root()),
            Expression::Binary {
                operator: BinaryOperator::Less,
                ..
            }
        ),
        "the `<` remains an infix operator even when its receiver is Error: {:?}",
        parsed.ast().expressions()
    );
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0009"]
    );
}

#[test]
fn typed_call_trial_is_deterministic_across_source_loading_order() {
    type ParseShape = (
        usize,
        usize,
        Vec<(usize, usize)>,
        Vec<(String, usize, usize)>,
    );

    fn shape(text: &str, add_noise_first: bool) -> ParseShape {
        let mut sources = SourceMap::new();
        if add_noise_first {
            add_source(&mut sources, "noise.ko", "noise");
        }
        let source_id = add_source(&mut sources, "typed.ko", text);
        let lexed = lex(&sources, source_id).expect("lex");
        let parsed = parse_expression(&sources, &lexed).expect("parse");
        (
            parsed.ast().expressions().len(),
            parsed.ast().type_refs().len(),
            parsed
                .ast()
                .type_refs()
                .iter()
                .map(|(_, node)| (node.span().start(), node.span().end()))
                .collect(),
            parsed
                .diagnostics()
                .iter()
                .map(|diagnostic| {
                    (
                        diagnostic.code().to_string(),
                        diagnostic.primary_span().start(),
                        diagnostic.primary_span().end(),
                    )
                })
                .collect(),
        )
    }

    for text in ["f<A<B>, C>()", "f<A<B>>", "a < b > c"] {
        assert_eq!(shape(text, false), shape(text, true), "{text:?}");
        assert_eq!(shape(text, false), shape(text, false), "{text:?}");
    }
}

#[test]
fn string_group_prefix_cast_assignment_and_error_payloads_are_observable() {
    let (_sources, parsed) = parsed_case(r#""a${x + 1}b""#);
    let Expression::String { parts } = expression(&parsed, parsed.root()) else {
        panic!("expected string")
    };
    assert_eq!(parts.len(), 3);
    assert!(matches!(parts[0], StringPart::Text(span) if (span.start(), span.end()) == (1, 2)));
    assert!(
        matches!(parts[1], StringPart::Interpolation { span, expression: _ } if (span.start(), span.end()) == (2, 10))
    );
    assert!(matches!(parts[2], StringPart::Text(span) if (span.start(), span.end()) == (10, 11)));

    let (_sources, parsed) = parsed_case("(-a as? T) = b += c");
    let Expression::Assignment {
        operator: AssignmentOperator::Assign,
        target,
        value,
        ..
    } = expression(&parsed, parsed.root())
    else {
        panic!("expected assignment")
    };
    assert!(matches!(
        expression(&parsed, *value),
        Expression::Assignment {
            operator: AssignmentOperator::AddAssign,
            ..
        }
    ));
    let Expression::Group {
        expression: grouped,
    } = expression(&parsed, *target)
    else {
        panic!("expected group")
    };
    let Expression::Cast {
        expression: cast_input,
        operator: CastOperator::SafeAs,
        ..
    } = expression(&parsed, *grouped)
    else {
        panic!("expected cast")
    };
    assert!(matches!(
        expression(&parsed, *cast_input),
        Expression::Prefix {
            operator: PrefixOperator::Minus,
            ..
        }
    ));

    let mut sources = SourceMap::new();
    let id = add_source(&mut sources, "error.ko", "");
    let lexed = lex(&sources, id).expect("lex");
    let parsed = parse_expression(&sources, &lexed).expect("parse");
    assert!(matches!(
        expression(&parsed, parsed.root()),
        Expression::Error
    ));
}

#[test]
fn every_adjacent_precedence_level_has_the_expected_outer_node() {
    let cases: &[(&str, ExpressionPredicate)] = &[
        ("a = b || c", |e| matches!(e, Expression::Assignment { .. })),
        ("a || b && c", |e| {
            matches!(
                e,
                Expression::Binary {
                    operator: BinaryOperator::LogicalOr,
                    ..
                }
            )
        }),
        ("a && b == c", |e| {
            matches!(
                e,
                Expression::Binary {
                    operator: BinaryOperator::LogicalAnd,
                    ..
                }
            )
        }),
        ("a == b < c", |e| {
            matches!(
                e,
                Expression::Binary {
                    operator: BinaryOperator::Equal,
                    ..
                }
            )
        }),
        ("a < b in c", |e| {
            matches!(
                e,
                Expression::Binary {
                    operator: BinaryOperator::Less,
                    ..
                }
            )
        }),
        ("a in b ?: c", |e| {
            matches!(
                e,
                Expression::Binary {
                    operator: BinaryOperator::In,
                    ..
                }
            )
        }),
        ("a ?: b to c", |e| {
            matches!(
                e,
                Expression::Binary {
                    operator: BinaryOperator::Elvis,
                    ..
                }
            )
        }),
        ("a to b..c", |e| {
            matches!(
                e,
                Expression::Binary {
                    operator: BinaryOperator::To,
                    ..
                }
            )
        }),
        ("a..b + c", |e| {
            matches!(
                e,
                Expression::Binary {
                    operator: BinaryOperator::InclusiveRange,
                    ..
                }
            )
        }),
        ("a + b * c", |e| {
            matches!(
                e,
                Expression::Binary {
                    operator: BinaryOperator::Add,
                    ..
                }
            )
        }),
        ("a * b as T", |e| {
            matches!(
                e,
                Expression::Binary {
                    operator: BinaryOperator::Multiply,
                    ..
                }
            )
        }),
        ("-a as T", |e| matches!(e, Expression::Cast { .. })),
        ("-a.b", |e| {
            matches!(
                e,
                Expression::Prefix {
                    operator: PrefixOperator::Minus,
                    ..
                }
            )
        }),
    ];
    for (text, predicate) in cases {
        let (_sources, parsed) = parsed_case(text);
        assert!(
            predicate(expression(&parsed, parsed.root())),
            "wrong root for {text:?}"
        );
    }
}

#[test]
fn type_ref_payloads_and_spans_distinguish_qualified_nullable_and_function_types() {
    let (_sources, parsed) = parsed_case("x as pkg.Outer<Inner<T>, Size>?");
    let Expression::Cast { type_ref, .. } = expression(&parsed, parsed.root()) else {
        panic!("cast")
    };
    let node = parsed.ast().type_refs().get(*type_ref).expect("type");
    assert_eq!((node.span().start(), node.span().end()), (5, 31));
    let TypeRef::Qualified {
        segments,
        nullable_span: Some(nullable),
    } = node.payload()
    else {
        panic!("qualified")
    };
    assert_eq!(segments.len(), 2);
    assert!(segments[0].arguments.is_empty());
    assert_eq!(segments[1].arguments.len(), 2);
    assert_eq!((nullable.start(), nullable.end()), (30, 31));

    let (_sources, parsed) = parsed_case("x is move (A, B<C>) -> D?");
    let Expression::TypeTest {
        type_ref,
        negated: false,
        ..
    } = expression(&parsed, parsed.root())
    else {
        panic!("type test")
    };
    let node = parsed.ast().type_refs().get(*type_ref).expect("type");
    assert_eq!((node.span().start(), node.span().end()), (5, 25));
    let TypeRef::Function {
        move_span: Some(move_span),
        parameters,
        return_type,
        ..
    } = node.payload()
    else {
        panic!("function")
    };
    assert_eq!(parameters.len(), 2);
    assert_eq!((move_span.start(), move_span.end()), (5, 9));
    let return_node = parsed
        .ast()
        .type_refs()
        .get(*return_type)
        .expect("return type");
    assert!(matches!(
        return_node.payload(),
        TypeRef::Qualified {
            nullable_span: Some(_),
            ..
        }
    ));
}

#[test]
fn every_closed_operator_variant_and_remaining_payload_shape_is_constructed() {
    for (text, expected) in [
        ("a*b", BinaryOperator::Multiply),
        ("a/b", BinaryOperator::Divide),
        ("a%b", BinaryOperator::Remainder),
        ("a+b", BinaryOperator::Add),
        ("a-b", BinaryOperator::Subtract),
        ("a..b", BinaryOperator::InclusiveRange),
        ("a..<b", BinaryOperator::ExclusiveRange),
        ("a to b", BinaryOperator::To),
        ("a?:b", BinaryOperator::Elvis),
        ("a in b", BinaryOperator::In),
        ("a !in b", BinaryOperator::NotIn),
        ("a<b", BinaryOperator::Less),
        ("a>b", BinaryOperator::Greater),
        ("a<=b", BinaryOperator::LessEqual),
        ("a>=b", BinaryOperator::GreaterEqual),
        ("a==b", BinaryOperator::Equal),
        ("a!=b", BinaryOperator::NotEqual),
        ("a&&b", BinaryOperator::LogicalAnd),
        ("a||b", BinaryOperator::LogicalOr),
    ] {
        let (_sources, parsed) = parsed_case(text);
        assert!(
            matches!(expression(&parsed, parsed.root()), Expression::Binary { operator, .. } if *operator == expected)
        );
    }
    for (text, expected) in [
        ("a=b", AssignmentOperator::Assign),
        ("a+=b", AssignmentOperator::AddAssign),
        ("a-=b", AssignmentOperator::SubtractAssign),
        ("a*=b", AssignmentOperator::MultiplyAssign),
        ("a/=b", AssignmentOperator::DivideAssign),
        ("a%=b", AssignmentOperator::RemainderAssign),
    ] {
        let (_sources, parsed) = parsed_case(text);
        assert!(
            matches!(expression(&parsed, parsed.root()), Expression::Assignment { operator, .. } if *operator == expected)
        );
    }
    for (text, expected) in [
        ("!a", PrefixOperator::Not),
        ("+a", PrefixOperator::Plus),
        ("-a", PrefixOperator::Minus),
    ] {
        let (_sources, parsed) = parsed_case(text);
        assert!(
            matches!(expression(&parsed, parsed.root()), Expression::Prefix { operator, .. } if *operator == expected)
        );
    }

    let (_sources, parsed) = parsed_case("a?.b");
    assert!(matches!(
        expression(&parsed, parsed.root()),
        Expression::Member { safe: true, .. }
    ));
    let (_sources, parsed) = parsed_case("::ref");
    assert!(matches!(
        expression(&parsed, parsed.root()),
        Expression::CallableReference { receiver: None, .. }
    ));
    let (_sources, parsed) = parsed_case("f(a,b)");
    assert!(
        matches!(expression(&parsed, parsed.root()), Expression::Call { arguments, .. } if arguments.len() == 2)
    );
    for (text, negated) in [("x is T", false), ("x !is T", true)] {
        let (_sources, parsed) = parsed_case(text);
        assert!(
            matches!(expression(&parsed, parsed.root()), Expression::TypeTest { negated: actual, .. } if *actual == negated)
        );
    }

    let mut sources = SourceMap::new();
    let id = add_source(&mut sources, "bad-type.ko", "x as 4");
    let lexed = lex(&sources, id).expect("lex");
    let parsed = parse_expression(&sources, &lexed).expect("parse");
    let Expression::Cast { type_ref, .. } = expression(&parsed, parsed.root()) else {
        panic!("cast")
    };
    assert!(matches!(
        parsed
            .ast()
            .type_refs()
            .get(*type_ref)
            .expect("type")
            .payload(),
        TypeRef::Error
    ));
}

#[test]
fn associativity_is_encoded_by_child_direction() {
    let (_sources, parsed) = parsed_case("a-b-c");
    let Expression::Binary {
        operator: BinaryOperator::Subtract,
        left,
        right,
        ..
    } = expression(&parsed, parsed.root())
    else {
        panic!("expected subtraction")
    };
    assert!(matches!(expression(&parsed, *right), Expression::Name));
    assert!(matches!(
        expression(&parsed, *left),
        Expression::Binary {
            operator: BinaryOperator::Subtract,
            ..
        }
    ));

    for text in ["a=b=c", "a?:b?:c"] {
        let (_sources, parsed) = parsed_case(text);
        let right = match expression(&parsed, parsed.root()) {
            Expression::Assignment { value, .. } => *value,
            Expression::Binary {
                operator: BinaryOperator::Elvis,
                right,
                ..
            } => *right,
            other => panic!("{text:?}: {other:?}"),
        };
        assert!(matches!(
            expression(&parsed, right),
            Expression::Assignment { .. }
                | Expression::Binary {
                    operator: BinaryOperator::Elvis,
                    ..
                }
        ));
    }
}

#[test]
fn soft_words_and_argument_markers_do_not_become_general_infix_or_prefix_syntax() {
    let (_sources, parsed) = parsed_case("infix");
    assert!(matches!(
        expression(&parsed, parsed.root()),
        Expression::Name
    ));
    for text in [
        "a name b",
        "a infix b",
        "own x",
        "inout x",
        "borrow x",
        "move x",
    ] {
        let diagnostics = parse_fingerprints(text);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.0 == "L0009" || diagnostic.0 == "L0013"),
            "{text:?}: {diagnostics:?}"
        );
    }
}

#[test]
fn deferred_type_and_call_forms_are_rejected_by_current_productions() {
    for text in [
        "x as T??",
        "x as (() -> T)?",
        "x as A<T>.B",
        "x as A<out T>",
        "f(x) { y }",
    ] {
        let diagnostics = parse_fingerprints(text);
        assert!(!diagnostics.is_empty(), "{text:?} must remain deferred");
    }
}

#[test]
fn primary_postfix_string_and_every_operator_level_parse() {
    for text in [
        "name",
        "this",
        "true",
        "false",
        "null",
        "0",
        "1.5",
        "'x'",
        "(name)",
        "::name",
        "\"\"",
        r#""text""#,
        r#""a${x + 1}b${y}""#,
        "a!!.b(c, d)[i]::ref!!",
        "!-x",
        "a + -b",
        "-!-x",
        "x as T as? U",
        "a * b / c % d",
        "a + b - c",
        "a..b",
        "a..<b",
        "a to b to c",
        "a ?: b ?: c",
        "a in b",
        "a !in b",
        "a is T",
        "a !is pkg.T?",
        "a < b",
        "a >= b",
        "a == b",
        "a != b",
        "a && b && c",
        "a || b || c",
        "a = b += c",
        "f()",
        "f((a = b))",
        "arr[1..3]",
    ] {
        assert_parses(text);
    }
}

#[test]
fn nested_type_references_and_function_types_parse_without_shift_confusion() {
    for text in [
        "x as pkg.Outer<Inner<T>>?",
        "x as Array<Int, Size>",
        "x is () -> T?",
        "x !is move (pkg.A, B<C>) -> D?",
    ] {
        assert_parses(text);
    }
}

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
        "f(borrow input)",
        "f(&input)",
        "f(name = borrow input)",
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

    for malformed in ["f(name =", "f(borrow", "!+1(name ="] {
        let mut sources = SourceMap::new();
        let source_id = add_source(&mut sources, "argument-eof.ko", malformed);
        let lexed = lex(&sources, source_id).expect("test source must lex");
        let parsed = parse_expression(&sources, &lexed)
            .unwrap_or_else(|error| panic!("{malformed:?} returned {error:?}"));
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
    let lexed = lex(&sources, source_id).expect("test source must lex");
    let parsed = parse_expression(&sources, &lexed).expect("test source identity must parse");
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
    let lexed = lex(&origin, origin_id).expect("test source must lex");
    let foreign = SourceMap::new();
    assert!(matches!(
        parse_expression(&foreign, &lexed),
        Err(ParserInternalError::Source(
            lang_frontend::source::SourceError::InvalidSourceId { source_id }
        )) if source_id == origin_id
    ));
}

#[test]
fn diagnostics_are_deterministic_across_source_loading_order_and_repeated_runs() {
    let text = "a < b <= c trailing";
    assert_eq!(parse_fingerprints(text), parse_fingerprints(text));

    let mut sources = SourceMap::new();
    let _noise = add_source(&mut sources, "noise.ko", "noise");
    let source_id = add_source(&mut sources, "case.ko", text);
    let lexed = lex(&sources, source_id).expect("test source must lex");
    let parsed = parse_expression(&sources, &lexed).expect("test source identity must parse");
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
    let lexed = lex(&sources, source_id).expect("test source must lex");
    let parsed = parse_expression(&sources, &lexed).expect("test source identity must parse");
    (sources, parsed)
}

#[test]
fn malformed_move_type_consumes_one_error_region_without_trailing_duplication() {
    let text = "x as move T";
    let (_sources, parsed) = parsed_case_with_diagnostics(text);
    assert!(
        !parsed
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0013")
    );
    let Expression::Cast { type_ref, .. } = expression(&parsed, parsed.root()) else {
        panic!("cast")
    };
    let error = parsed.ast().type_refs().get(*type_ref).expect("error type");
    assert!(matches!(error.payload(), TypeRef::Error));
    assert!(!error.span().is_empty());
    assert_eq!(
        (error.span().start(), error.span().end()),
        (text.find("move").unwrap(), text.len())
    );
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
fn malformed_move_type_error_span_includes_the_consumed_marker() {
    for text in ["x as move", "x as A<move, B>"] {
        let (_sources, parsed) = parsed_case_with_diagnostics(text);
        let move_start = text.find("move").expect("move");
        let error = parsed.ast().type_refs().iter().find(|(_, node)| {
            matches!(node.payload(), TypeRef::Error) && node.span().start() == move_start
        });
        assert!(
            error.is_some(),
            "{text:?} must preserve move in its Error TypeRef: {:?}",
            parsed.ast().type_refs()
        );
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
        let lexed = lex(&sources, source_id).expect("test source must lex");
        let result = parse_expression(&sources, &lexed);
        assert!(
            matches!(
                result,
                Err(ParserInternalError::NestingLimitExceeded { limit: 1024 })
            ),
            "{label}: {result:?}"
        );
    }
}
