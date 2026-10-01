//! SPEC-0231: contextual TypeRef words agree between the parser and strict call trial.

use lang_frontend::{
    ast::TypeRefId,
    parser::{Expression, Item, NameMarker, ParameterModeMarker, ParsedExpression, TypeRef},
    source::SourceMap,
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use parser_test_assertions::{parse_declaration_twice, parse_expression_twice};

fn parsed(text: &str) -> (SourceMap, ParsedExpression) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("contextual.ko", text).unwrap();
    let parsed = parse_expression_twice(&sources, source, text);
    (sources, parsed)
}

fn parsed_ok(text: &str) -> (SourceMap, ParsedExpression) {
    let result = parsed(text);
    assert!(
        result.1.diagnostics().is_empty(),
        "{text}: {:?}",
        result.1.diagnostics()
    );
    result
}

fn root_type(parsed: &ParsedExpression) -> TypeRefId {
    match parsed
        .ast()
        .expressions()
        .get(parsed.root())
        .unwrap()
        .payload()
    {
        Expression::Cast { type_ref, .. } => *type_ref,
        Expression::Call { type_arguments, .. } => {
            assert_eq!(type_arguments.len(), 1);
            type_arguments[0]
        }
        other => panic!("expected cast or typed call, got {other:?}"),
    }
}

#[test]
fn nested_function_parameter_modes_agree_in_casts_and_typed_calls() {
    for mode in ["own", "borrow", "inout"] {
        let ty = format!("({mode} () -> Unit) -> Unit");
        for text in [format!("source as {ty}"), format!("f<{ty}>()")] {
            let (sources, parsed) = parsed_ok(&text);
            let TypeRef::Function { parameters, .. } = parsed
                .ast()
                .type_refs()
                .get(root_type(&parsed))
                .unwrap()
                .payload()
            else {
                panic!("outer function type");
            };
            let [parameter] = parameters.as_slice() else {
                panic!("one parameter");
            };
            let marker = parameter
                .mode_marker
                .expect("explicit outer parameter mode");
            let marker_span = match marker {
                ParameterModeMarker::Own(span)
                | ParameterModeMarker::Borrow(span)
                | ParameterModeMarker::Inout(span) => span,
            };
            assert_eq!(sources.slice(marker_span).unwrap(), mode);
            assert_eq!(
                sources.slice(parameter.span).unwrap(),
                format!("{mode} () -> Unit")
            );
            assert!(matches!(
                parsed
                    .ast()
                    .type_refs()
                    .get(parameter.type_ref)
                    .unwrap()
                    .payload(),
                TypeRef::Function { .. }
            ));
        }
    }
}

#[test]
fn mode_words_at_type_boundaries_remain_ordinary_type_names() {
    for ty in [
        "(own, borrow, inout) -> Unit",
        "(borrow borrow) -> Unit",
        "(own inout) -> borrow",
    ] {
        for text in [format!("source as {ty}"), format!("f<{ty}>()")] {
            let (sources, parsed) = parsed_ok(&text);
            let TypeRef::Function { parameters, .. } = parsed
                .ast()
                .type_refs()
                .get(root_type(&parsed))
                .unwrap()
                .payload()
            else {
                panic!("outer function type");
            };
            for parameter in parameters {
                assert!(matches!(
                    parsed
                        .ast()
                        .type_refs()
                        .get(parameter.type_ref)
                        .unwrap()
                        .payload(),
                    TypeRef::Qualified { .. }
                ));
                if ty.starts_with("(own,") {
                    assert!(parameter.mode_marker.is_none());
                    assert!(matches!(
                        sources.slice(parameter.span).unwrap(),
                        "own" | "borrow" | "inout"
                    ));
                } else {
                    assert!(parameter.mode_marker.is_some());
                }
            }
        }
    }
}

#[test]
fn move_is_a_type_name_unless_followed_by_a_function_parameter_list() {
    for ty in ["move", "move?", "move<T>", "move.Member", "Outer<move>"] {
        for text in [format!("source as {ty}"), format!("f<{ty}>()")] {
            let (sources, parsed) = parsed_ok(&text);
            let node = parsed.ast().type_refs().get(root_type(&parsed)).unwrap();
            assert!(matches!(node.payload(), TypeRef::Qualified { .. }));
            assert_eq!(sources.slice(node.span()).unwrap(), ty);
        }
    }
    for ty in ["move () -> Unit", "(borrow move () -> Unit) -> Unit"] {
        for text in [format!("source as {ty}"), format!("f<{ty}>()")] {
            let (sources, parsed) = parsed_ok(&text);
            assert!(parsed.ast().type_refs().iter().any(|(_, node)| matches!(node.payload(), TypeRef::Function { move_span: Some(span), .. } if sources.slice(*span) == Ok("move"))));
        }
    }
}

#[test]
fn named_parameter_context_keeps_contextual_names_and_mode_contracts_separate() {
    let text = "fun f(borrow: borrow, own: own, inout: inout, move: move): move";
    let mut sources = SourceMap::new();
    let source = sources.add_source("declaration.ko", text).unwrap();
    let parsed = parse_declaration_twice(&sources, source, text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let Item::Function { parameters, .. } =
        parsed.ast().items().get(parsed.root()).unwrap().payload()
    else {
        panic!("function declaration");
    };
    for (parameter, expected) in parameters.iter().zip(["borrow", "own", "inout", "move"]) {
        assert!(parameter.mode_marker.is_none());
        let NameMarker::Present(name) = parameter.name else {
            panic!("real name");
        };
        assert_eq!(sources.slice(name).unwrap(), expected);
        assert!(matches!(
            parsed
                .ast()
                .type_refs()
                .get(parameter.type_ref)
                .unwrap()
                .payload(),
            TypeRef::Qualified { .. }
        ));
    }
}

#[test]
fn named_parameter_recovery_does_not_consume_a_name_before_a_function_type() {
    for prefix in ["borrow", "own borrow"] {
        let text = format!("fun f({prefix} () -> Unit, tail: Unit)");
        let mut sources = SourceMap::new();
        let source = sources.add_source("named-recovery.ko", &text).unwrap();
        let parsed = parse_declaration_twice(&sources, source, &text);
        assert_eq!(
            parsed.diagnostics().len(),
            1,
            "{text}: {:?}",
            parsed.diagnostics()
        );
        assert_eq!(
            parsed.diagnostics()[0].message(),
            "expected ':' after parameter name"
        );
        assert_eq!(parsed.diagnostics()[0].code().to_string(), "L0023");
        assert_eq!(
            sources
                .slice(parsed.diagnostics()[0].primary_span())
                .unwrap(),
            "("
        );
        let Item::Function { parameters, .. } =
            parsed.ast().items().get(parsed.root()).unwrap().payload()
        else {
            panic!("function declaration");
        };
        assert_eq!(parameters.len(), 2);
        assert_eq!(
            parameters[0].mode_marker.is_some(),
            prefix.starts_with("own")
        );
        if prefix.starts_with("own") {
            assert!(matches!(
                parameters[0].mode_marker,
                Some(ParameterModeMarker::Own(_))
            ));
        }
        for (parameter, name) in parameters.iter().zip(["borrow", "tail"]) {
            let NameMarker::Present(span) = parameter.name else {
                panic!("real parameter name");
            };
            assert_eq!(sources.slice(span).unwrap(), name);
        }
        assert!(matches!(
            parsed
                .ast()
                .type_refs()
                .get(parameters[0].type_ref)
                .unwrap()
                .payload(),
            TypeRef::Function { .. }
        ));
        assert!(parameters[0].colon_span.is_empty());
        assert!(!parameters[1].colon_span.is_empty());
    }
}

#[test]
fn duplicate_mode_recovery_keeps_first_mode_nested_type_and_following_type_name() {
    let (sources, parsed) = parsed("source as (borrow inout () -> Unit, own) -> Unit");
    assert_eq!(parsed.diagnostics().len(), 1, "{:?}", parsed.diagnostics());
    assert_eq!(parsed.diagnostics()[0].code().to_string(), "L0039");
    assert_eq!(
        sources
            .slice(parsed.diagnostics()[0].primary_span())
            .unwrap(),
        "inout"
    );
    let TypeRef::Function { parameters, .. } = parsed
        .ast()
        .type_refs()
        .get(root_type(&parsed))
        .unwrap()
        .payload()
    else {
        panic!("function type");
    };
    assert_eq!(parameters.len(), 2);
    assert!(matches!(
        parameters[0].mode_marker,
        Some(ParameterModeMarker::Borrow(_))
    ));
    assert!(matches!(
        parsed
            .ast()
            .type_refs()
            .get(parameters[0].type_ref)
            .unwrap()
            .payload(),
        TypeRef::Function { .. }
    ));
    assert!(parameters[1].mode_marker.is_none());
    assert_eq!(sources.slice(parameters[1].span).unwrap(), "own");
}

#[test]
fn malformed_contextual_type_trials_never_commit_partial_type_nodes() {
    for text in [
        "f<(borrow inout () -> Unit) -> Unit>()",
        "f<(borrow borrow T) -> Unit>()",
        "f<(borrow () ->) -> Unit>()",
        "f<(borrow () -> Unit,) -> Unit>()",
        "f<move (borrow () -> Unit)>()",
        "f<(borrow () -> Unit) ->>()",
    ] {
        let (_, parsed) = parsed(text);
        assert!(!parsed.diagnostics().is_empty(), "{text}");
        assert_eq!(
            parsed.ast().type_refs().len(),
            0,
            "failed trial leaked TypeRefs: {text}"
        );
    }
}

#[test]
fn nested_mode_type_trials_keep_cursor_and_structure_across_trivia() {
    for depth in [1, 8, 32] {
        let ty = format!(
            "{}Unit{}",
            "(borrow ".repeat(depth),
            ") -> Unit".repeat(depth)
        );
        for text in [format!("f<{ty}>()"), format!("f</*left*/{ty}/*right*/>()")] {
            let (_, parsed) = parsed_ok(&text);
            let root = parsed.ast().expressions().get(parsed.root()).unwrap();
            assert_eq!(root.span().end(), text.len());
            assert_eq!(
                parsed
                    .ast()
                    .type_refs()
                    .iter()
                    .filter(|(_, node)| matches!(node.payload(), TypeRef::Function { .. }))
                    .count(),
                depth
            );
        }
    }
}

#[test]
fn contextual_type_heads_ignore_trivia_at_the_decision_gap() {
    for gap in [" /* gap */ ", "\n", "\r\n/* gap */"] {
        for ty in [
            format!("move{gap}() -> Unit"),
            format!("(borrow{gap}() -> Unit) -> Unit"),
            format!("(own{gap}move{gap}() -> Unit) -> Unit"),
        ] {
            for text in [format!("source as {ty}"), format!("f<{ty}>()")] {
                let (_, parsed) = parsed_ok(&text);
                assert!(matches!(
                    parsed
                        .ast()
                        .type_refs()
                        .get(root_type(&parsed))
                        .unwrap()
                        .payload(),
                    TypeRef::Function { .. }
                ));
            }
        }
    }
}
