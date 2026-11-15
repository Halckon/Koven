//! SPEC-0012 / SPEC-0121 callable 参数契约与 typed call argument 的公共契约测试。

#[path = "parser_call_argument/recovery.rs"]
mod recovery;

use lang_frontend::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::{Diagnostic, Severity},
    parser::{
        AssignmentOperator, CallArgument, Expression, FunctionTypeParameter, Item, NameMarker,
        ParameterModeMarker, ParsedDeclaration, ParsedExpression, TypeRef,
    },
    source::{SourceId, SourceMap, Span},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{parse_declaration_twice, parse_expression_twice};

fn add_source(sources: &mut SourceMap, name: &str, text: &str) -> SourceId {
    sources.add_source(name, text).expect("unique source name")
}

fn parsed_expression(text: &str) -> (SourceMap, ParsedExpression) {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "expression.ko", text);
    let parsed = parse_expression_twice(&sources, source_id, text);
    (sources, parsed)
}

fn parsed_expression_ok(text: &str) -> (SourceMap, ParsedExpression) {
    let result = parsed_expression(text);
    assert!(
        result.1.diagnostics().is_empty(),
        "{text:?}: {:?}",
        result.1.diagnostics()
    );
    result
}

fn parsed_declaration(text: &str) -> (SourceMap, ParsedDeclaration) {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "declaration.ko", text);
    let parsed = parse_declaration_twice(&sources, source_id, text);
    (sources, parsed)
}

fn expression(parsed: &ParsedExpression, id: ExpressionId) -> &Expression {
    parsed
        .ast()
        .expressions()
        .get(id)
        .expect("expression ID")
        .payload()
}

fn declaration_type_ref(parsed: &ParsedDeclaration, id: TypeRefId) -> &TypeRef {
    parsed.ast().type_refs().get(id).expect("type ID").payload()
}

fn expression_type_ref(parsed: &ParsedExpression, id: TypeRefId) -> &TypeRef {
    parsed.ast().type_refs().get(id).expect("type ID").payload()
}

fn root_arguments(parsed: &ParsedExpression) -> &[CallArgument] {
    let Expression::Call { arguments, .. } = expression(parsed, parsed.root()) else {
        panic!("expected call root")
    };
    arguments
}

fn only_call(parsed: &ParsedExpression) -> (Span, &[CallArgument]) {
    let calls = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Expression::Call { arguments, .. } => Some((node.span(), arguments.as_slice())),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [call] = calls.as_slice() else {
        panic!("expected exactly one call, got {}", calls.len())
    };
    *call
}

fn mode_span(marker: ParameterModeMarker) -> Span {
    match marker {
        ParameterModeMarker::Own(span)
        | ParameterModeMarker::Borrow(span)
        | ParameterModeMarker::Inout(span) => span,
    }
}

fn mode_name(marker: ParameterModeMarker) -> &'static str {
    match marker {
        ParameterModeMarker::Own(_) => "own",
        ParameterModeMarker::Borrow(_) => "borrow",
        ParameterModeMarker::Inout(_) => "inout",
    }
}

fn name_span(marker: NameMarker) -> Span {
    match marker {
        NameMarker::Present(span) | NameMarker::Missing(span) | NameMarker::Error(span) => span,
    }
}

fn fingerprint(diagnostic: &Diagnostic) -> (String, Severity, String, usize, usize) {
    let span = diagnostic.primary_span();
    (
        diagnostic.code().to_string(),
        diagnostic.severity(),
        diagnostic.message().to_owned(),
        span.start(),
        span.end(),
    )
}

fn fingerprints(parsed: &ParsedExpression) -> Vec<(String, Severity, String, usize, usize)> {
    parsed.diagnostics().iter().map(fingerprint).collect()
}

#[test]
fn four_argument_forms_share_one_payload_and_preserve_exact_spans() {
    struct Case {
        text: &'static str,
        argument: (usize, usize),
        named: Option<((usize, usize), (usize, usize))>,
        mode: Option<(&'static str, (usize, usize), &'static str)>,
        value: (usize, usize),
    }

    let cases = [
        Case {
            text: "f(e)",
            argument: (2, 3),
            named: None,
            mode: None,
            value: (2, 3),
        },
        Case {
            text: "f(name = e)",
            argument: (2, 10),
            named: Some(((2, 6), (7, 8))),
            mode: None,
            value: (9, 10),
        },
        Case {
            text: "f(&e)",
            argument: (2, 4),
            named: None,
            mode: Some(("inout", (2, 3), "&")),
            value: (3, 4),
        },
        Case {
            text: "f(name = &e)",
            argument: (2, 11),
            named: Some(((2, 6), (7, 8))),
            mode: Some(("inout", (9, 10), "&")),
            value: (10, 11),
        },
    ];

    for case in cases {
        let (sources, parsed) = parsed_expression_ok(case.text);
        let [argument] = root_arguments(&parsed) else {
            panic!("{:?}: expected one argument", case.text)
        };
        assert_eq!(
            (argument.span.start(), argument.span.end()),
            case.argument,
            "{:?}",
            case.text
        );

        match (argument.named_prefix, case.named) {
            (None, None) => {}
            (Some(actual), Some((name, equals))) => {
                assert_eq!((actual.name_span.start(), actual.name_span.end()), name);
                assert_eq!(
                    (actual.equals_span.start(), actual.equals_span.end()),
                    equals
                );
                assert_eq!(sources.slice(actual.name_span).expect("name"), "name");
                assert_eq!(sources.slice(actual.equals_span).expect("equals"), "=");
            }
            pair => panic!("{:?}: named prefix mismatch: {pair:?}", case.text),
        }

        match (argument.mode_marker, case.mode) {
            (None, None) => {}
            (Some(actual), Some((kind, expected_span, spelling))) => {
                let span = mode_span(actual);
                assert_eq!(mode_name(actual), kind);
                assert_eq!((span.start(), span.end()), expected_span);
                assert_eq!(sources.slice(span).expect("mode"), spelling);
            }
            pair => panic!("{:?}: mode mismatch: {pair:?}", case.text),
        }

        let value = parsed
            .ast()
            .expressions()
            .get(argument.value)
            .expect("argument value");
        assert!(matches!(value.payload(), Expression::Name));
        assert_eq!((value.span().start(), value.span().end()), case.value);
    }
}

#[test]
fn basic_typed_member_and_chained_calls_reuse_call_argument_payload() {
    let (_, typed) = parsed_expression_ok("f<T>(name = &input)");
    let Expression::Call {
        type_arguments,
        arguments,
        ..
    } = expression(&typed, typed.root())
    else {
        panic!("typed call")
    };
    assert_eq!(type_arguments.len(), 1);
    assert_eq!(arguments.len(), 1);
    assert!(arguments[0].named_prefix.is_some());
    assert!(matches!(
        arguments[0].mode_marker,
        Some(ParameterModeMarker::Inout(_))
    ));

    let (_, member) = parsed_expression_ok("obj.f(input)");
    let Expression::Call {
        callee, arguments, ..
    } = expression(&member, member.root())
    else {
        panic!("member call")
    };
    assert!(matches!(
        expression(&member, *callee),
        Expression::Member { .. }
    ));
    assert!(matches!(
        arguments.as_slice(),
        [CallArgument {
            mode_marker: None,
            ..
        }]
    ));

    let (_, chained) = parsed_expression_ok("factory()(x).next(&y)");
    let Expression::Call {
        callee,
        arguments: outer_arguments,
        ..
    } = expression(&chained, chained.root())
    else {
        panic!("outer chained call")
    };
    assert_eq!(outer_arguments.len(), 1);
    assert!(matches!(
        outer_arguments[0].mode_marker,
        Some(ParameterModeMarker::Inout(_))
    ));
    let Expression::Member { receiver, .. } = expression(&chained, *callee) else {
        panic!("member receiver")
    };
    let Expression::Call {
        callee,
        arguments: middle_arguments,
        ..
    } = expression(&chained, *receiver)
    else {
        panic!("middle call")
    };
    assert_eq!(middle_arguments.len(), 1);
    let Expression::Call {
        arguments: inner_arguments,
        ..
    } = expression(&chained, *callee)
    else {
        panic!("inner call")
    };
    assert!(inner_arguments.is_empty());
}

#[test]
fn grouped_assignments_remain_single_argument_values_with_optional_modes() {
    let text = "f((a = b), (c = d), &(e = g))";
    let (sources, parsed) = parsed_expression_ok(text);
    let arguments = root_arguments(&parsed);
    assert_eq!(arguments.len(), 3);
    assert!(arguments[0].mode_marker.is_none());
    assert!(arguments[1].mode_marker.is_none());
    assert!(matches!(
        arguments[2].mode_marker,
        Some(ParameterModeMarker::Inout(_))
    ));
    for argument in arguments {
        let Expression::Group {
            expression: grouped,
        } = expression(&parsed, argument.value)
        else {
            panic!("grouped argument")
        };
        assert!(matches!(
            expression(&parsed, *grouped),
            Expression::Assignment {
                operator: AssignmentOperator::Assign,
                ..
            }
        ));
        assert_eq!(argument.span.source_id(), parsed.source_id());
        assert!(
            !sources
                .slice(argument.span)
                .expect("argument span")
                .is_empty()
        );
    }
}

#[test]
fn declarations_and_function_types_share_parameter_mode_markers() {
    let text = "fun f(x: T, own consumed: S, borrow y: U, inout z: V): R";
    let (sources, parsed) = parsed_declaration(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let Item::Function { parameters, .. } = parsed
        .ast()
        .items()
        .get(parsed.root())
        .expect("function")
        .payload()
    else {
        panic!("function item")
    };
    assert_eq!(parameters.len(), 4);
    assert!(parameters[0].mode_marker.is_none());
    assert_eq!(
        sources.slice(parameters[0].span).expect("value parameter"),
        "x: T"
    );
    for (parameter, expected_kind, expected_marker, expected_name, expected_parameter) in [
        (&parameters[1], "own", "own", "consumed", "own consumed: S"),
        (&parameters[2], "borrow", "borrow", "y", "borrow y: U"),
        (&parameters[3], "inout", "inout", "z", "inout z: V"),
    ] {
        let marker = parameter.mode_marker.expect("explicit marker");
        assert_eq!(mode_name(marker), expected_kind);
        assert_eq!(
            sources.slice(mode_span(marker)).expect("marker"),
            expected_marker
        );
        assert_eq!(
            sources.slice(name_span(parameter.name)).expect("name"),
            expected_name
        );
        assert_eq!(
            sources.slice(parameter.span).expect("parameter"),
            expected_parameter
        );
    }

    let text = "input as move (T, own S, borrow U, inout Box<V>) -> R";
    let (sources, parsed) = parsed_expression_ok(text);
    let function = parsed
        .ast()
        .type_refs()
        .iter()
        .find_map(|(_, node)| match node.payload() {
            TypeRef::Function {
                move_span,
                parameters,
                ..
            } => Some((*move_span, parameters)),
            TypeRef::Error | TypeRef::Qualified { .. } => None,
        })
        .expect("function type");
    assert_eq!(sources.slice(function.0.expect("move")).unwrap(), "move");
    assert_function_type_parameters(&sources, function.1);
}

fn assert_function_type_parameters(sources: &SourceMap, parameters: &[FunctionTypeParameter]) {
    assert_eq!(parameters.len(), 4);
    assert!(parameters[0].mode_marker.is_none());
    assert_eq!(sources.slice(parameters[0].span).expect("value type"), "T");
    for (parameter, kind, spelling, full) in [
        (&parameters[1], "own", "own", "own S"),
        (&parameters[2], "borrow", "borrow", "borrow U"),
        (&parameters[3], "inout", "inout", "inout Box<V>"),
    ] {
        let marker = parameter.mode_marker.expect("mode");
        assert_eq!(mode_name(marker), kind);
        assert_eq!(sources.slice(mode_span(marker)).expect("mode"), spelling);
        assert_eq!(sources.slice(parameter.span).expect("parameter"), full);
    }
}

#[test]
fn strict_typed_call_trial_accepts_marker_function_type_arguments() {
    let text = "f<Box<move (own T, borrow U, inout Result<V>) -> R>>()";
    let (sources, parsed) = parsed_expression_ok(text);
    let Expression::Call { type_arguments, .. } = expression(&parsed, parsed.root()) else {
        panic!("typed call")
    };
    let [argument] = type_arguments.as_slice() else {
        panic!("one type argument")
    };
    let TypeRef::Qualified { segments, .. } = expression_type_ref(&parsed, *argument) else {
        panic!("generic wrapper type argument")
    };
    let [argument] = segments.last().expect("Box segment").arguments.as_slice() else {
        panic!("one nested type argument")
    };
    let TypeRef::Function {
        move_span,
        parameters,
        ..
    } = expression_type_ref(&parsed, *argument)
    else {
        panic!("function type argument")
    };
    assert_eq!(sources.slice(move_span.expect("move")).unwrap(), "move");
    assert_eq!(parameters.len(), 3);
    assert!(matches!(
        parameters[0].mode_marker,
        Some(ParameterModeMarker::Own(_))
    ));
    assert!(matches!(
        parameters[1].mode_marker,
        Some(ParameterModeMarker::Borrow(_))
    ));
    assert!(matches!(
        parameters[2].mode_marker,
        Some(ParameterModeMarker::Inout(_))
    ));
}

#[test]
fn failed_marker_function_type_trial_has_no_public_parser_side_effects() {
    let text = "f<Box<move (own T, borrow U, inout Result<V>) -> R>> + tail";
    let (_, first) = parsed_expression(text);
    let (_, second) = parsed_expression(text);

    assert_eq!(
        first.ast().type_refs().len(),
        0,
        "a failed strict candidate must not allocate TypeRef nodes"
    );
    assert_eq!(
        first.ast().expressions().len(),
        second.ast().expressions().len(),
        "a failed read-only trial must not perturb later AST allocation"
    );
    assert_eq!(fingerprints(&first), fingerprints(&second));
    assert!(first.diagnostics().iter().all(|diagnostic| {
        !matches!(
            diagnostic.code().to_string().as_str(),
            "L0024" | "L0025" | "L0026" | "L0039"
        )
    }));
    let root = first
        .ast()
        .expressions()
        .get(first.root())
        .expect("root expression");
    assert_eq!(
        root.span().end(),
        text.len(),
        "normal expression parsing must retain the cursor after the failed trial"
    );
}

#[test]
fn call_only_ampersand_is_not_a_prefix_operator_or_lexer_error() {
    let (_, parsed) = parsed_expression("&input");
    assert!(
        parsed
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0009")
    );
    assert!(
        !parsed
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0001")
    );

    for text in ["f(inout input)", "f(own input)"] {
        let (_, parsed) = parsed_expression(text);
        let boundary = text.find("input").expect("second identifier");
        assert_eq!(
            fingerprints(&parsed),
            [(
                "L0034".to_owned(),
                Severity::Error,
                "expected argument separator".to_owned(),
                boundary,
                boundary
            )],
            "{text}"
        );
        assert_eq!(root_arguments(&parsed).len(), 2);
        assert!(
            root_arguments(&parsed)
                .iter()
                .all(|argument| argument.mode_marker.is_none())
        );
    }
}

#[test]
fn empty_recovery_children_do_not_extend_parent_spans_across_trivia() {
    let text = "f(name = /* gap */ )";
    let (sources, parsed) = parsed_expression(text);
    let [argument] = root_arguments(&parsed) else {
        panic!("one recovered argument")
    };
    assert_eq!(sources.slice(argument.span).expect("argument"), "name =");
    let value = parsed
        .ast()
        .expressions()
        .get(argument.value)
        .expect("error value");
    let boundary = text.find(')').expect("call closer");
    assert_eq!(
        (value.span().start(), value.span().end()),
        (boundary, boundary)
    );

    let text = "fun f(borrow x: /* gap */ , y: T): R";
    let (sources, parsed) = parsed_declaration(text);
    let Item::Function { parameters, .. } = parsed
        .ast()
        .items()
        .get(parsed.root())
        .expect("function")
        .payload()
    else {
        panic!("function item")
    };
    assert_eq!(
        sources.slice(parameters[0].span).expect("value parameter"),
        "borrow x:"
    );
    let type_ref = parsed
        .ast()
        .type_refs()
        .get(parameters[0].type_ref)
        .expect("error type");
    let boundary = text.find(',').expect("parameter separator");
    assert_eq!(
        (type_ref.span().start(), type_ref.span().end()),
        (boundary, boundary)
    );

    let text = "input as (borrow /* gap */ , T) -> R";
    let (sources, parsed) = parsed_expression_ok(text);
    let function_parameters = parsed
        .ast()
        .type_refs()
        .iter()
        .find_map(|(_, node)| match node.payload() {
            TypeRef::Function { parameters, .. } => Some(parameters),
            TypeRef::Error | TypeRef::Qualified { .. } => None,
        })
        .expect("function type");
    assert_eq!(
        sources
            .slice(function_parameters[0].span)
            .expect("function parameter"),
        "borrow"
    );
    let type_ref = parsed
        .ast()
        .type_refs()
        .get(function_parameters[0].type_ref)
        .expect("ordinary borrow type");
    assert!(function_parameters[0].mode_marker.is_none());
    assert!(matches!(type_ref.payload(), TypeRef::Qualified { .. }));
    assert_eq!((type_ref.span().start(), type_ref.span().end()), (10, 16));
}

#[test]
fn missing_call_closer_span_stops_at_the_last_consumed_token_before_outer_owner() {
    for (text, expected_call) in [
        ("items[f(a, /* gap */ ]", "f(a,"),
        ("items[f( /* gap */ ]", "f("),
        ("items[f(name = /* gap */ ]", "f(name ="),
    ] {
        let (sources, parsed) = parsed_expression(text);
        let Expression::Index { index, .. } = expression(&parsed, parsed.root()) else {
            panic!("{text:?}: index root")
        };
        let call = parsed
            .ast()
            .expressions()
            .get(*index)
            .expect("call inside index");
        assert!(matches!(call.payload(), Expression::Call { .. }));
        assert_eq!(
            sources.slice(call.span()).expect("call span"),
            expected_call,
            "{text:?}"
        );
    }
}

#[test]
fn recovered_argument_spans_keep_their_source_identity() {
    let text = "f(name =, next)";
    let mut sources = SourceMap::new();
    add_source(&mut sources, "noise.ko", "noise");
    let source_id = add_source(&mut sources, "case.ko", text);
    let parsed = parse_expression_twice(&sources, source_id, text);
    assert_eq!(parsed.source_id(), source_id);
    assert_eq!(root_arguments(&parsed).len(), 2);
    for argument in root_arguments(&parsed) {
        assert_eq!(argument.span.source_id(), source_id);
        let value = parsed
            .ast()
            .expressions()
            .get(argument.value)
            .expect("value");
        assert_eq!(value.span().source_id(), source_id);
    }
}

#[test]
fn function_type_parameter_type_ids_remain_resolvable_after_marker_parsing() {
    let (_, parsed) = parsed_declaration("fun f(callback: (borrow T, inout U) -> R): R");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let Item::Function { parameters, .. } = parsed
        .ast()
        .items()
        .get(parsed.root())
        .expect("function")
        .payload()
    else {
        panic!("function")
    };
    let TypeRef::Function { parameters, .. } =
        declaration_type_ref(&parsed, parameters[0].type_ref)
    else {
        panic!("callback function type")
    };
    assert_eq!(parameters.len(), 2);
    for parameter in parameters {
        parsed
            .ast()
            .type_refs()
            .get(parameter.type_ref)
            .expect("function parameter type ID");
    }
}

#[test]
fn ordinary_borrow_calls_keep_whitespace_independent_expression_spans() {
    for gap in ["", " ", "\t", "\n", " /* gap */ "] {
        for prefix in ["", "name = "] {
            let inner = format!("borrow{gap}(input)");
            let text = format!("f({prefix}{inner}, tail)");
            let (sources, parsed) = parsed_expression_ok(&text);
            let arguments = root_arguments(&parsed);
            assert_eq!(arguments.len(), 2, "{text}");
            let argument = &arguments[0];
            assert!(argument.mode_marker.is_none(), "{text}");
            assert_eq!(argument.named_prefix.is_some(), !prefix.is_empty());
            assert_eq!(
                sources.slice(argument.span).unwrap(),
                format!("{prefix}{inner}")
            );
            let value = parsed.ast().expressions().get(argument.value).unwrap();
            assert_eq!(sources.slice(value.span()).unwrap(), inner);
            let Expression::Call {
                callee,
                arguments: inner_arguments,
                ..
            } = value.payload()
            else {
                panic!("{text}: ordinary borrow call")
            };
            let callee = parsed.ast().expressions().get(*callee).unwrap();
            assert!(matches!(callee.payload(), Expression::Name));
            assert_eq!(sources.slice(callee.span()).unwrap(), "borrow");
            assert_eq!(inner_arguments.len(), 1);
            assert!(inner_arguments[0].mode_marker.is_none());
            assert_eq!(sources.slice(inner_arguments[0].span).unwrap(), "input");
            assert_eq!(sources.slice(arguments[1].span).unwrap(), "tail");
        }
        let text = format!("borrow{gap}(input)");
        let (sources, parsed) = parsed_expression_ok(&text);
        assert_eq!(root_arguments(&parsed).len(), 1);
        assert_eq!(
            sources
                .slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(parsed.root())
                        .unwrap()
                        .span()
                )
                .unwrap(),
            text
        );
    }
}

#[test]
fn removed_borrow_prefix_uses_canonical_separator_recovery() {
    for prefix in ["", "name = "] {
        let text = format!("f({prefix}borrow input, tail)");
        let (sources, parsed) = parsed_expression(&text);
        let boundary = text.find("input").unwrap();
        assert_eq!(
            fingerprints(&parsed),
            [(
                "L0034".to_owned(),
                Severity::Error,
                "expected argument separator".to_owned(),
                boundary,
                boundary
            )],
            "{text}"
        );
        let arguments = root_arguments(&parsed);
        assert_eq!(arguments.len(), 3, "{text}");
        assert_eq!(arguments[0].named_prefix.is_some(), !prefix.is_empty());
        assert_eq!(
            sources.slice(arguments[0].span).unwrap(),
            format!("{prefix}borrow")
        );
        for (argument, expected) in arguments.iter().zip(["borrow", "input", "tail"]) {
            assert!(argument.mode_marker.is_none(), "{text}");
            let value = parsed.ast().expressions().get(argument.value).unwrap();
            assert!(matches!(value.payload(), Expression::Name));
            assert_eq!(sources.slice(value.span()).unwrap(), expected);
        }
    }
}

#[test]
fn ordinary_borrow_names_members_and_lambdas_never_become_markers() {
    for text in [
        "f(borrow)",
        "f(borrow.member)",
        "f(borrow { input })",
        "f(borrow<T>(input))",
        "f(borrow = input)",
        "f(own(input), inout(input))",
    ] {
        let (sources, parsed) = parsed_expression_ok(text);
        assert!(
            root_arguments(&parsed)
                .iter()
                .all(|argument| argument.mode_marker.is_none()),
            "{text}"
        );
        for (_, node) in parsed.ast().expressions().iter() {
            if let Expression::Call { arguments, .. } = node.payload() {
                assert!(
                    arguments
                        .iter()
                        .all(|argument| argument.mode_marker.is_none()),
                    "{text}"
                );
            }
        }
        assert_eq!(
            sources
                .slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(parsed.root())
                        .unwrap()
                        .span()
                )
                .unwrap(),
            text
        );
    }
}
