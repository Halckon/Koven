//! Call argument diagnostics and nested-owner recovery contracts.
use super::*;

#[test]
fn l0033_expected_value_preserves_committed_prefix_and_empty_error_value() {
    for (text, boundary, has_name, mode) in [
        ("f(name =)", 8, true, None),
        ("f(&,)", 3, false, Some("inout")),
        ("f(&)", 3, false, Some("inout")),
    ] {
        let (_, parsed) = parsed_expression(text);
        assert!(
            fingerprints(&parsed).contains(&(
                "L0033".to_owned(),
                Severity::Error,
                "expected argument value".to_owned(),
                boundary,
                boundary,
            )),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        let [argument] = root_arguments(&parsed) else {
            panic!("{text:?}: one recovered argument")
        };
        assert_eq!(argument.named_prefix.is_some(), has_name);
        assert_eq!(argument.mode_marker.map(mode_name), mode);
        assert_eq!(
            (argument.span.start(), argument.span.end()),
            (2, boundary),
            "an empty Error value must not enlarge its parent argument"
        );
        let error = parsed
            .ast()
            .expressions()
            .get(argument.value)
            .expect("error value");
        assert!(matches!(error.payload(), Expression::Error));
        assert_eq!(
            (error.span().start(), error.span().end()),
            (boundary, boundary)
        );
    }
}

#[test]
fn l0034_missing_separator_keeps_both_argument_values_in_source_order() {
    let text = "f(a b)";
    let (_, parsed) = parsed_expression(text);
    assert_eq!(
        fingerprints(&parsed),
        [(
            "L0034".to_owned(),
            Severity::Error,
            "expected argument separator".to_owned(),
            4,
            4,
        )]
    );
    let arguments = root_arguments(&parsed);
    assert_eq!(arguments.len(), 2);
    assert!(
        arguments
            .iter()
            .all(|argument| matches!(expression(&parsed, argument.value), Expression::Name))
    );
}

#[test]
fn l0034_ordinary_invalid_region_is_not_attached_to_the_next_argument() {
    let text = "f(a @ [x], next)";
    let (sources, parsed) = parsed_expression(text);
    assert_eq!(
        fingerprints(&parsed),
        [(
            "L0034".to_owned(),
            Severity::Error,
            "expected argument separator".to_owned(),
            4,
            5,
        )]
    );
    let arguments = root_arguments(&parsed);
    assert_eq!(arguments.len(), 2);
    assert_eq!(sources.slice(arguments[0].span).unwrap(), "a");
    assert_eq!(sources.slice(arguments[1].span).unwrap(), "next");
    assert!(
        arguments
            .iter()
            .all(|argument| matches!(expression(&parsed, argument.value), Expression::Name))
    );
}

#[test]
fn l0035_empty_element_consumes_only_the_comma_and_inserts_one_error_argument() {
    let text = "f(,a)";
    let (_, parsed) = parsed_expression(text);
    assert_eq!(
        fingerprints(&parsed),
        [(
            "L0035".to_owned(),
            Severity::Error,
            "unsupported argument empty element".to_owned(),
            2,
            3,
        )]
    );
    let arguments = root_arguments(&parsed);
    assert_eq!(arguments.len(), 2);
    let error = parsed
        .ast()
        .expressions()
        .get(arguments[0].value)
        .expect("empty element");
    assert!(matches!(error.payload(), Expression::Error));
    assert_eq!((error.span().start(), error.span().end()), (2, 2));
    assert!(matches!(
        expression(&parsed, arguments[1].value),
        Expression::Name
    ));

    let (_, before_closer) = parsed_expression("f(,)");
    assert_eq!(
        before_closer
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0035"]
    );
    assert_eq!(root_arguments(&before_closer).len(), 1);
}

#[test]
fn l0036_trailing_comma_inserts_one_error_at_the_preserved_closer() {
    let text = "f(a,)";
    let (_, parsed) = parsed_expression(text);
    assert_eq!(
        fingerprints(&parsed),
        [(
            "L0036".to_owned(),
            Severity::Error,
            "unsupported argument trailing comma".to_owned(),
            3,
            4,
        )]
    );
    let arguments = root_arguments(&parsed);
    assert_eq!(arguments.len(), 2);
    let error = parsed
        .ast()
        .expressions()
        .get(arguments[1].value)
        .expect("trailing error argument");
    assert!(matches!(error.payload(), Expression::Error));
    assert_eq!((error.span().start(), error.span().end()), (4, 4));
}

#[test]
fn l0037_mode_before_name_discards_the_reversed_prefix_but_keeps_mode_and_value() {
    let text = "f(&name = input)";
    let (_, parsed) = parsed_expression(text);
    assert_eq!(
        fingerprints(&parsed),
        [(
            "L0037".to_owned(),
            Severity::Error,
            "invalid argument mode ordering".to_owned(),
            8,
            9,
        )]
    );
    let [argument] = root_arguments(&parsed) else {
        panic!("one argument")
    };
    assert!(argument.named_prefix.is_none());
    assert!(matches!(
        argument.mode_marker,
        Some(ParameterModeMarker::Inout(_))
    ));
    assert_eq!((argument.span.start(), argument.span.end()), (2, 15));
    assert!(matches!(
        expression(&parsed, argument.value),
        Expression::Name
    ));
}

#[test]
fn l0038_reports_each_extra_mode_and_logical_and_is_not_a_mode() {
    let text = "f(& & &input)";
    let (_, parsed) = parsed_expression(text);
    assert_eq!(
        fingerprints(&parsed),
        [
            (
                "L0038".to_owned(),
                Severity::Error,
                "duplicate argument mode".to_owned(),
                4,
                5,
            ),
            (
                "L0038".to_owned(),
                Severity::Error,
                "duplicate argument mode".to_owned(),
                6,
                7,
            ),
        ]
    );
    let [argument] = root_arguments(&parsed) else {
        panic!("one argument")
    };
    assert!(matches!(
        argument.mode_marker,
        Some(ParameterModeMarker::Inout(_))
    ));
    assert!(matches!(
        expression(&parsed, argument.value),
        Expression::Name
    ));

    let (_, adjacent_ampersands) = parsed_expression("f(& &x)");
    assert_eq!(
        fingerprints(&adjacent_ampersands),
        [(
            "L0038".to_owned(),
            Severity::Error,
            "duplicate argument mode".to_owned(),
            4,
            5,
        )]
    );
    let [argument] = root_arguments(&adjacent_ampersands) else {
        panic!("one argument")
    };
    assert!(matches!(
        argument.mode_marker,
        Some(ParameterModeMarker::Inout(_))
    ));
    assert!(matches!(
        expression(&adjacent_ampersands, argument.value),
        Expression::Name
    ));

    let (_, logical_and) = parsed_expression("f(&&x)");
    assert!(logical_and.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().to_string() == "L0033"
            && diagnostic.message() == "expected argument value"
    }));
    assert!(
        !logical_and
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0038")
    );
}

#[test]
fn l0039_keeps_the_first_parameter_mode_and_recovers_following_parameters() {
    let text = "fun f(own borrow inout x: T, y: U): R";
    let (sources, parsed) = parsed_declaration(text);
    let duplicates = parsed
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.code().to_string() == "L0039")
        .map(fingerprint)
        .collect::<Vec<_>>();
    assert_eq!(
        duplicates,
        [
            (
                "L0039".to_owned(),
                Severity::Error,
                "duplicate parameter mode".to_owned(),
                10,
                16,
            ),
            (
                "L0039".to_owned(),
                Severity::Error,
                "duplicate parameter mode".to_owned(),
                17,
                22,
            ),
        ]
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
    assert_eq!(parameters.len(), 2);
    assert!(matches!(
        parameters[0].mode_marker,
        Some(ParameterModeMarker::Own(_))
    ));
    assert_eq!(sources.slice(name_span(parameters[0].name)).unwrap(), "x");
    assert_eq!(sources.slice(name_span(parameters[1].name)).unwrap(), "y");

    let text = "input as (inout borrow T, U) -> R";
    let (sources, parsed) = parsed_expression(text);
    assert!(fingerprints(&parsed).contains(&(
        "L0039".to_owned(),
        Severity::Error,
        "duplicate parameter mode".to_owned(),
        16,
        22,
    )));
    let function = parsed
        .ast()
        .type_refs()
        .iter()
        .find_map(|(_, node)| match node.payload() {
            TypeRef::Function { parameters, .. } => Some(parameters),
            TypeRef::Error | TypeRef::Qualified { .. } => None,
        })
        .expect("function type");
    assert_eq!(function.len(), 2);
    assert!(matches!(
        function[0].mode_marker,
        Some(ParameterModeMarker::Inout(_))
    ));
    assert_eq!(sources.slice(function[0].span).unwrap(), "inout borrow T");
    assert_eq!(sources.slice(function[1].span).unwrap(), "U");

    let text = "input as (borrow inout borrow T) -> R";
    let (sources, parsed) = parsed_expression(text);
    let duplicates = fingerprints(&parsed)
        .into_iter()
        .filter(|diagnostic| diagnostic.0 == "L0039")
        .collect::<Vec<_>>();
    assert_eq!(
        duplicates,
        [
            (
                "L0039".to_owned(),
                Severity::Error,
                "duplicate parameter mode".to_owned(),
                17,
                22,
            ),
            (
                "L0039".to_owned(),
                Severity::Error,
                "duplicate parameter mode".to_owned(),
                23,
                29,
            ),
        ]
    );
    let function = parsed
        .ast()
        .type_refs()
        .iter()
        .find_map(|(_, node)| match node.payload() {
            TypeRef::Function { parameters, .. } => Some(parameters),
            TypeRef::Error | TypeRef::Qualified { .. } => None,
        })
        .expect("function type");
    let [parameter] = function.as_slice() else {
        panic!("one function parameter")
    };
    assert!(matches!(
        parameter.mode_marker,
        Some(ParameterModeMarker::Borrow(_))
    ));
    assert_eq!(
        sources.slice(parameter.span).unwrap(),
        "borrow inout borrow T"
    );
}

#[test]
fn nested_argument_owners_do_not_split_on_inner_commas_or_closers() {
    let text = r#"outer(name = inner(a, b), (c = d), &items[index(a, b)], { x -> f(x, y) }, "${f(a, b)}")"#;
    let (_, parsed) = parsed_expression_ok(text);
    let arguments = root_arguments(&parsed);
    assert_eq!(arguments.len(), 5);
    assert!(matches!(
        expression(&parsed, arguments[0].value),
        Expression::Call { arguments, .. } if arguments.len() == 2
    ));
    assert!(matches!(
        expression(&parsed, arguments[1].value),
        Expression::Group { .. }
    ));
    assert!(matches!(
        expression(&parsed, arguments[2].value),
        Expression::Index { .. }
    ));
    assert!(matches!(
        expression(&parsed, arguments[3].value),
        Expression::Lambda { .. }
    ));
    assert!(matches!(
        expression(&parsed, arguments[4].value),
        Expression::String { .. }
    ));
}

#[test]
fn terminal_lexer_owners_inside_arguments_do_not_gain_parser_cascades() {
    for (text, expected_codes, expect_string_root) in [
        ("f(name = \"a\\\n, next)", vec!["L0006"], false),
        (r#""${f(name = "a\q${x}", next)}""#, vec!["L0006"], true),
        (
            "\"${f(\"a\\\n, name = \"${x}\", next)}tail\"",
            vec!["L0006"],
            true,
        ),
        ("f(name = \"a\\q\n, next)", vec!["L0004", "L0006"], false),
    ] {
        let (_, parsed) = parsed_expression(text);
        assert_eq!(
            parsed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect::<Vec<_>>(),
            expected_codes,
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        let root = parsed.ast().expressions().get(parsed.root()).expect("root");
        assert_eq!(root.span().end(), text.len(), "{text:?}");
        if expect_string_root {
            assert!(
                matches!(root.payload(), Expression::String { .. }),
                "{text:?}"
            );
        } else {
            assert!(
                matches!(root.payload(), Expression::Call { .. }),
                "{text:?}"
            );
        }
    }
}

#[test]
fn invalid_argument_recovery_keeps_l0033_primary_narrow_and_error_region_complete() {
    let text = "f(@ [x], next)";
    let (sources, parsed) = parsed_expression(text);
    assert_eq!(
        fingerprints(&parsed),
        [(
            "L0033".to_owned(),
            Severity::Error,
            "expected argument value".to_owned(),
            2,
            3,
        )]
    );
    let arguments = root_arguments(&parsed);
    assert_eq!(arguments.len(), 2);
    let error = parsed
        .ast()
        .expressions()
        .get(arguments[0].value)
        .expect("recovered value");
    assert!(matches!(error.payload(), Expression::Error));
    assert_eq!(sources.slice(error.span()).expect("error region"), "@ [x]");
    assert_eq!(sources.slice(arguments[0].span).unwrap(), "@ [x]");
    assert!(matches!(
        expression(&parsed, arguments[1].value),
        Expression::Name
    ));
    assert_eq!(sources.slice(arguments[1].span).unwrap(), "next");
}

#[test]
fn invalid_argument_recovery_observes_terminal_string_owners_and_resumes_after_comma() {
    for (text, expected_codes, expected_error) in [
        ("f(@ \"a\\\n, next)", vec!["L0033", "L0006"], "@ \"a\\"),
        (
            "f(@ \"a\\q\n, next)",
            vec!["L0033", "L0004", "L0006"],
            "@ \"a\\q",
        ),
    ] {
        let (sources, parsed) = parsed_expression(text);
        assert_eq!(
            parsed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect::<Vec<_>>(),
            expected_codes,
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        let l0033 = parsed
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.code().to_string() == "L0033")
            .expect("L0033");
        assert_eq!(sources.slice(l0033.primary_span()).unwrap(), "@");
        let arguments = root_arguments(&parsed);
        assert_eq!(arguments.len(), 2, "{text:?}");
        let error = parsed
            .ast()
            .expressions()
            .get(arguments[0].value)
            .expect("recovered value");
        assert!(matches!(error.payload(), Expression::Error));
        assert_eq!(sources.slice(error.span()).unwrap(), expected_error);
        assert!(matches!(
            expression(&parsed, arguments[1].value),
            Expression::Name
        ));
        assert_eq!(sources.slice(arguments[1].span).unwrap(), "next");
        let call = parsed
            .ast()
            .expressions()
            .get(parsed.root())
            .expect("call root");
        assert_eq!(call.span().end(), text.len());
    }
}

#[test]
fn invalid_argument_recovery_tracks_nested_interpolation_terminal_owner() {
    let text = r#""${f(@ "a\q${x}", next)}""#;
    let (sources, parsed) = parsed_expression(text);
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0033", "L0006"]
    );
    let l0033 = &parsed.diagnostics()[0];
    assert_eq!(sources.slice(l0033.primary_span()).unwrap(), "@");
    let (call_span, arguments) = only_call(&parsed);
    assert_eq!(arguments.len(), 2);
    let error = parsed
        .ast()
        .expressions()
        .get(arguments[0].value)
        .expect("recovered value");
    assert_eq!(sources.slice(error.span()).unwrap(), r#"@ "a\q${x}""#);
    assert_eq!(sources.slice(arguments[1].span).unwrap(), "next");
    assert_eq!(sources.slice(call_span).unwrap(), r#"f(@ "a\q${x}", next)"#);
    let root = parsed
        .ast()
        .expressions()
        .get(parsed.root())
        .expect("string root");
    assert!(matches!(root.payload(), Expression::String { .. }));
    assert_eq!(root.span().end(), text.len());
}

#[test]
fn invalid_argument_recovery_preserves_inherited_and_mismatched_outer_closers() {
    for (text, expected_codes, expected_root, expected_error, expected_call) in [
        (
            r#""${f(@ [x)}tail""#,
            vec!["L0033"],
            "string",
            "@ [x",
            "f(@ [x)",
        ),
        (
            "items[f(@ (x]",
            vec!["L0033", "L0010"],
            "index",
            "@ (x",
            "f(@ (x",
        ),
    ] {
        let (sources, parsed) = parsed_expression(text);
        assert_eq!(
            parsed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect::<Vec<_>>(),
            expected_codes,
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        assert_eq!(
            sources
                .slice(parsed.diagnostics()[0].primary_span())
                .unwrap(),
            "@"
        );
        let (call_span, arguments) = only_call(&parsed);
        let [argument] = arguments else {
            panic!("{text:?}: one recovered argument")
        };
        let error = parsed
            .ast()
            .expressions()
            .get(argument.value)
            .expect("error value");
        assert_eq!(sources.slice(error.span()).unwrap(), expected_error);
        assert_eq!(sources.slice(call_span).unwrap(), expected_call);
        let root = parsed
            .ast()
            .expressions()
            .get(parsed.root())
            .expect("outer root");
        assert_eq!(root.span().end(), text.len());
        assert!(
            matches!(root.payload(), Expression::String { .. }) && expected_root == "string"
                || matches!(root.payload(), Expression::Index { .. }) && expected_root == "index"
        );
    }
}

#[test]
fn prior_terminal_recovery_does_not_hide_later_invalid_argument_or_outer_owner() {
    let text = "\"${f(\"a\n, @ [x)}tail\"";
    let (sources, parsed) = parsed_expression(text);
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0004", "L0033"]
    );
    assert_eq!(
        sources
            .slice(parsed.diagnostics()[1].primary_span())
            .unwrap(),
        "@"
    );
    let (call_span, arguments) = only_call(&parsed);
    assert_eq!(arguments.len(), 2);
    assert!(matches!(
        expression(&parsed, arguments[0].value),
        Expression::String { .. }
    ));
    let error = parsed
        .ast()
        .expressions()
        .get(arguments[1].value)
        .expect("later error argument");
    assert_eq!(sources.slice(error.span()).unwrap(), "@ [x");
    assert_eq!(sources.slice(call_span).unwrap(), "f(\"a\n, @ [x)");
    let root = parsed
        .ast()
        .expressions()
        .get(parsed.root())
        .expect("string root");
    assert!(matches!(root.payload(), Expression::String { .. }));
    assert_eq!(root.span().end(), text.len());
}

#[test]
fn recovered_terminal_string_then_valid_string_stays_in_one_invalid_region() {
    let text = "f(@ \"a\n\"b,c\", next)";
    let (sources, parsed) = parsed_expression(text);
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0033", "L0004"]
    );
    assert_eq!(
        sources
            .slice(parsed.diagnostics()[0].primary_span())
            .unwrap(),
        "@"
    );
    let arguments = root_arguments(&parsed);
    assert_eq!(arguments.len(), 2);
    let error = parsed
        .ast()
        .expressions()
        .get(arguments[0].value)
        .expect("complete invalid region");
    assert!(matches!(error.payload(), Expression::Error));
    assert_eq!(sources.slice(error.span()).unwrap(), "@ \"a\n\"b,c\"");
    assert_eq!(sources.slice(arguments[1].span).unwrap(), "next");
}

#[test]
fn prior_terminal_escape_keeps_later_invalid_region_and_nested_string_owner() {
    let text = "\"${f(\"a\\\n, @ \"${x}\", next)}tail\"";
    let (sources, parsed) = parsed_expression(text);
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0006", "L0033"]
    );
    assert_eq!(
        sources
            .slice(parsed.diagnostics()[1].primary_span())
            .unwrap(),
        "@"
    );
    let (_, arguments) = only_call(&parsed);
    assert_eq!(arguments.len(), 3);
    let error = parsed
        .ast()
        .expressions()
        .get(arguments[1].value)
        .expect("later invalid region");
    assert!(matches!(error.payload(), Expression::Error));
    assert_eq!(sources.slice(error.span()).unwrap(), "@ \"${x}\"");
    assert_eq!(sources.slice(arguments[2].span).unwrap(), "next");
    let root = parsed
        .ast()
        .expressions()
        .get(parsed.root())
        .expect("outer string");
    assert!(matches!(root.payload(), Expression::String { .. }));
    assert_eq!(root.span().end(), text.len());
}
