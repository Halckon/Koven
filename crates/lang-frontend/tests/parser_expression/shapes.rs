//! Expression AST shape, strict suffix, type reference and precedence contracts.
use super::*;

#[test]
fn concrete_payloads_use_typed_ids_and_exact_composite_spans() {
    for (text, expected) in [
        ("name", Expression::Name),
        ("this", Expression::This),
        (
            "1",
            Expression::Literal(LiteralKind::Integer(IntegerLiteralKind::Unsuffixed)),
        ),
        (
            "1.5",
            Expression::Literal(LiteralKind::Float(FloatLiteralKind::Double)),
        ),
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
fn numeric_suffixes_have_normalized_parser_ast_identities() {
    for (text, expected) in [
        ("1", LiteralKind::Integer(IntegerLiteralKind::Unsuffixed)),
        ("1L", LiteralKind::Integer(IntegerLiteralKind::Long)),
        ("1u", LiteralKind::Integer(IntegerLiteralKind::Unsigned)),
        ("1U", LiteralKind::Integer(IntegerLiteralKind::Unsigned)),
        (
            "1uL",
            LiteralKind::Integer(IntegerLiteralKind::UnsignedLong),
        ),
        (
            "1UL",
            LiteralKind::Integer(IntegerLiteralKind::UnsignedLong),
        ),
        ("1.0", LiteralKind::Float(FloatLiteralKind::Double)),
        ("1.0f", LiteralKind::Float(FloatLiteralKind::Float)),
        ("1.0F", LiteralKind::Float(FloatLiteralKind::Float)),
        ("1f", LiteralKind::Float(FloatLiteralKind::Float)),
        ("1F", LiteralKind::Float(FloatLiteralKind::Float)),
    ] {
        let (_sources, parsed) = parsed_case(text);
        assert_eq!(
            expression(&parsed, parsed.root()),
            &Expression::Literal(expected),
            "{text}"
        );
    }
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
        let parsed = parse_expression_twice(&sources, source_id, text);
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
    let parsed = parse_expression_twice(&sources, id, "empty error expression");
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
    let parsed = parse_expression_twice(&sources, id, "bad cast type");
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
fn deferred_type_forms_are_rejected_by_current_productions() {
    for text in [
        "x as T??",
        "x as (() -> T)?",
        "x as A<T>.B",
        "x as A<out T>",
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
fn contextual_keyword_value_is_allowed_in_ordinary_and_postfix_positions() {
    assert_parses("owner.value");
    assert_parses("owner.value.x");
    assert_parses("inspect(owner.value)");
    assert_parses("value");
}
