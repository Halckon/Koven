//! SPEC-0008 / SPEC-0118 的独立声明 Parser 公共契约与恢复测试。

use lang_frontend::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::{Diagnostic, Severity},
    parser::{
        Expression, FunctionBody, FunctionForm, Item, NameMarker, ParsedDeclaration,
        ParserInternalError, Statement, TypeRef, VariableKind, parse_declaration,
    },
    source::{SourceError, SourceId, SourceMap, Span},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{
    assert_parser_error_twice, lex_parser_source_twice, parse_declaration_twice,
};

fn add_source(sources: &mut SourceMap, text: &str) -> SourceId {
    sources.add_source("case.ko", text).expect("unique source")
}

fn parsed(text: &str) -> (SourceMap, ParsedDeclaration) {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, text);
    let parsed = parse_declaration_twice(&sources, source_id, text);
    (sources, parsed)
}

fn parsed_ok(text: &str) -> (SourceMap, ParsedDeclaration) {
    let result = parsed(text);
    assert!(
        result.1.diagnostics().is_empty(),
        "{text:?}: {:?}",
        result.1.diagnostics()
    );
    result
}

fn item(parsed: &ParsedDeclaration) -> &Item {
    parsed
        .ast()
        .items()
        .get(parsed.root())
        .expect("root")
        .payload()
}

fn expression(parsed: &ParsedDeclaration, id: ExpressionId) -> &Expression {
    parsed
        .ast()
        .expressions()
        .get(id)
        .expect("expression")
        .payload()
}

fn type_ref(parsed: &ParsedDeclaration, id: TypeRefId) -> &TypeRef {
    parsed.ast().type_refs().get(id).expect("type").payload()
}

fn marker_span(marker: NameMarker) -> Span {
    match marker {
        NameMarker::Present(span) | NameMarker::Missing(span) | NameMarker::Error(span) => span,
    }
}

fn fingerprints(text: &str) -> Vec<(String, Severity, String, usize, usize)> {
    parsed(text)
        .1
        .diagnostics()
        .iter()
        .map(|diagnostic: &Diagnostic| {
            let span = diagnostic.primary_span();
            (
                diagnostic.code().to_string(),
                diagnostic.severity(),
                diagnostic.message().to_owned(),
                span.start(),
                span.end(),
            )
        })
        .collect()
}

#[test]
fn variable_and_constant_payloads_preserve_typed_children_and_spans() {
    for (text, expected_kind) in [
        ("val answer = 42", VariableKind::Val),
        ("var answer: Int = 42", VariableKind::Var),
    ] {
        let (sources, parsed) = parsed_ok(text);
        let root = parsed.ast().items().get(parsed.root()).expect("root");
        assert_eq!((root.span().start(), root.span().end()), (0, text.len()));
        let Item::Variable {
            kind,
            name,
            colon_span,
            type_ref: annotation,
            equals_span,
            initializer,
        } = root.payload()
        else {
            panic!("variable")
        };
        assert_eq!(*kind, expected_kind);
        assert_eq!(sources.slice(marker_span(*name)).expect("name"), "answer");
        assert_eq!(sources.slice(*equals_span).expect("equals"), "=");
        assert!(matches!(
            expression(&parsed, *initializer),
            Expression::Literal(_)
        ));
        assert_eq!(colon_span.is_some(), annotation.is_some());
    }

    let text = "const val answer: Int = 42";
    let (sources, parsed) = parsed_ok(text);
    let Item::Constant {
        const_span,
        val_marker,
        name,
        colon_span,
        type_ref: annotation,
        equals_span,
        initializer,
    } = item(&parsed)
    else {
        panic!("constant")
    };
    assert_eq!(sources.slice(*const_span).expect("const"), "const");
    assert_eq!(sources.slice(marker_span(*val_marker)).expect("val"), "val");
    assert_eq!(sources.slice(marker_span(*name)).expect("name"), "answer");
    assert_eq!(
        sources.slice(colon_span.expect("colon")).expect("colon"),
        ":"
    );
    assert!(matches!(
        type_ref(&parsed, annotation.expect("type")),
        TypeRef::Qualified { .. }
    ));
    assert_eq!(sources.slice(*equals_span).expect("equals"), "=");
    assert!(matches!(
        expression(&parsed, *initializer),
        Expression::Literal(_)
    ));
}

#[test]
fn function_payload_preserves_generic_signature_and_optional_body() {
    let text = "fun <T: pkg.Outer<Inner>> map(x: T, f: move (T) -> T): T = f(x)";
    let (sources, parsed) = parsed_ok(text);
    let Item::Function {
        name,
        type_parameters,
        type_parameter_list_span,
        parameters,
        form,
    } = item(&parsed)
    else {
        panic!("function")
    };
    assert_eq!(sources.slice(marker_span(*name)).expect("name"), "map");
    assert_eq!(type_parameters.len(), 1);
    assert_eq!(
        sources
            .slice(type_parameter_list_span.expect("list"))
            .expect("list"),
        "<T: pkg.Outer<Inner>>"
    );
    assert!(type_parameters[0].bound.is_some());
    assert_eq!(parameters.len(), 2);
    let FunctionForm::Explicit {
        colon_span,
        type_ref: return_type,
        body,
    } = form
    else {
        panic!("explicit function")
    };
    assert_eq!(sources.slice(*colon_span).expect("colon"), ":");
    assert!(matches!(
        type_ref(&parsed, *return_type),
        TypeRef::Qualified { .. }
    ));
    let FunctionBody::Expression {
        equals_span,
        expression: body,
    } = body
    else {
        panic!("expression body")
    };
    assert_eq!(sources.slice(*equals_span).expect("equals"), "=");
    assert!(matches!(
        expression(&parsed, *body),
        Expression::Call { .. }
    ));

    let (_, signature) = parsed_ok("fun idle(): Unit");
    let Item::Function {
        type_parameters,
        type_parameter_list_span,
        parameters,
        form,
        ..
    } = item(&signature)
    else {
        panic!("signature")
    };
    assert!(type_parameters.is_empty() && parameters.is_empty());
    assert!(type_parameter_list_span.is_none());
    assert!(matches!(
        form,
        FunctionForm::Explicit {
            body: FunctionBody::Absent,
            ..
        }
    ));
}

fn markers(item: &Item) -> Vec<NameMarker> {
    match item {
        Item::Error => Vec::new(),
        Item::Variable { name, .. } | Item::Constant { name, .. } => vec![*name],
        Item::Function {
            name,
            type_parameters,
            parameters,
            ..
        } => std::iter::once(*name)
            .chain(type_parameters.iter().map(|parameter| parameter.name))
            .chain(parameters.iter().map(|parameter| parameter.name))
            .collect(),
        Item::Classifier(classifier) => vec![classifier.name],
        Item::Modified { .. } | Item::Companion(_) => Vec::new(),
    }
}

#[test]
fn declaration_and_parameter_names_lock_all_marker_states() {
    assert!(
        matches!(item(&parsed_ok("val name = 1").1), Item::Variable { name: NameMarker::Present(span), .. } if !span.is_empty())
    );
    for text in ["val = 1", "fun f(: T): R"] {
        let (_, parsed) = parsed(text);
        assert!(
            markers(item(&parsed))
                .iter()
                .any(|marker| matches!(marker, NameMarker::Missing(span) if span.is_empty())),
            "{text:?}"
        );
    }
    for text in ["val async = 1", "fun f(async: T): R", "fun <async> f(): R"] {
        let (_, parsed) = parsed(text);
        assert!(
            markers(item(&parsed))
                .iter()
                .any(|marker| matches!(marker, NameMarker::Error(span) if !span.is_empty())),
            "{text:?}"
        );
        assert_eq!(
            parsed
                .diagnostics()
                .iter()
                .filter(|diagnostic| diagnostic.code().to_string() == "L0002")
                .count(),
            1
        );
    }
}

#[test]
fn constant_val_marker_distinguishes_present_missing_and_consumed_error() {
    let cases = [
        ("const val x = 1", "present", (6, 9), "x"),
        ("const x = 1", "missing", (6, 6), "x"),
        ("const var x = 1", "error", (6, 9), "x"),
        ("const + x = 1", "error", (6, 7), "x"),
    ];
    for (text, state, expected_span, expected_name) in cases {
        let (sources, parsed) = parsed(text);
        let Item::Constant {
            val_marker, name, ..
        } = item(&parsed)
        else {
            panic!("{text:?}: constant")
        };
        assert!(
            matches!(
                (state, val_marker),
                ("present", NameMarker::Present(_))
                    | ("missing", NameMarker::Missing(_))
                    | ("error", NameMarker::Error(_))
            ),
            "{text:?}: {val_marker:?}"
        );
        assert_eq!(
            (
                marker_span(*val_marker).start(),
                marker_span(*val_marker).end()
            ),
            expected_span,
            "{text:?}"
        );
        assert_eq!(
            sources.slice(marker_span(*name)).expect("name"),
            expected_name
        );
    }

    let (_, parsed) = parsed("const = 1");
    let Item::Constant {
        val_marker, name, ..
    } = item(&parsed)
    else {
        panic!("constant")
    };
    assert!(matches!(val_marker, NameMarker::Missing(span) if span.is_empty()));
    assert!(matches!(name, NameMarker::Missing(span) if span.is_empty()));
}

#[test]
fn declaration_lists_preserve_elements_spans_and_each_recovery_state() {
    let (sources, valid) = parsed_ok("fun <T, U: A<B>> f(x: T, y: U): R");
    let Item::Function {
        type_parameters,
        type_parameter_list_span,
        parameters,
        ..
    } = item(&valid)
    else {
        panic!("function")
    };
    assert_eq!(type_parameters.len(), 2);
    assert_eq!(parameters.len(), 2);
    assert_eq!(
        sources
            .slice(type_parameter_list_span.expect("generic list span"))
            .expect("generic list"),
        "<T, U: A<B>>"
    );

    for (text, code, parameter_count) in [
        ("fun <> f(): R", "L0024", 0),
        ("fun <,T> f(): R", "L0024", 1),
        ("fun <T,,U> f(): R", "L0024", 2),
        ("fun <T U> f(): R", "L0025", 2),
        ("fun <T,> f(): R", "L0026", 1),
        ("fun f(,x: T): R", "L0024", 1),
        ("fun f(x: T,,y: U): R", "L0024", 2),
        ("fun f(x: T y: U): R", "L0025", 2),
        ("fun f(x: T,): R", "L0026", 1),
    ] {
        let (_, parsed) = parsed(text);
        let Item::Function {
            type_parameters,
            parameters,
            ..
        } = item(&parsed)
        else {
            panic!("{text:?}: function")
        };
        let count = if text.contains('<') {
            type_parameters.len()
        } else {
            parameters.len()
        };
        assert_eq!(count, parameter_count, "{text:?}");
        assert_eq!(
            parsed
                .diagnostics()
                .iter()
                .filter(|diagnostic| diagnostic.code().to_string() == code)
                .count(),
            1,
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
    }

    let text = "fun <T f(): R";
    let (sources, parsed) = parsed(text);
    let Item::Function {
        name,
        type_parameter_list_span,
        parameters,
        ..
    } = item(&parsed)
    else {
        panic!("function")
    };
    assert_eq!(sources.slice(marker_span(*name)).expect("name"), "f");
    assert!(parameters.is_empty());
    assert_eq!(
        sources
            .slice(type_parameter_list_span.expect("recovered generic list"))
            .expect("list"),
        "<T"
    );
    assert_eq!(fingerprints(text)[0].0, "L0010");
}

#[test]
fn l0017_through_l0027_have_stable_messages_and_representative_spans() {
    let cases = [
        ("", "L0017", "expected declaration", 0, 0),
        ("val = 1", "L0018", "expected declaration name", 4, 4),
        ("fun f(: T): R", "L0019", "expected parameter name", 6, 6),
        ("val x Int", "L0020", "expected initializer", 6, 9),
        (
            "fun f() = 1",
            "L0021",
            "expected explicit return type",
            8,
            8,
        ),
        ("const x = 1", "L0022", "expected 'val' after 'const'", 6, 7),
        (
            "fun f(x T): R",
            "L0023",
            "expected ':' after parameter name",
            8,
            9,
        ),
        ("fun <> f(): R", "L0024", "expected list element", 5, 6),
        (
            "fun f(x: T y: U): R",
            "L0025",
            "expected list separator",
            11,
            12,
        ),
        (
            "fun f(x: T,): R",
            "L0026",
            "unsupported trailing comma",
            10,
            11,
        ),
        (
            "fun f(x: T = 1): R",
            "L0027",
            "unsupported parameter default",
            11,
            14,
        ),
    ];
    for (text, code, message, start, end) in cases {
        let actual = fingerprints(text);
        assert!(
            actual.contains(&(
                code.to_owned(),
                Severity::Error,
                message.to_owned(),
                start,
                end
            )),
            "{text:?}: {actual:?}"
        );
    }
}

#[test]
fn separator_fallbacks_preserve_the_next_valid_child_and_suppress_same_root_cascades() {
    for text in ["val x 1", "const val x 1"] {
        let (_, parsed) = parsed(text);
        let initializer = match item(&parsed) {
            Item::Variable { initializer, .. } | Item::Constant { initializer, .. } => *initializer,
            other => panic!("{text:?}: {other:?}"),
        };
        assert!(matches!(
            expression(&parsed, initializer),
            Expression::Literal(_)
        ));
        assert_eq!(
            parsed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect::<Vec<_>>(),
            ["L0020"],
            "{text:?}"
        );
    }

    for text in ["fun f(x T): R", "fun f(x = value): R", "fun f(x): R"] {
        let (_, parsed) = parsed(text);
        let Item::Function { parameters, .. } = item(&parsed) else {
            panic!("{text:?}: function")
        };
        assert_eq!(parameters.len(), 1, "{text:?}");
        let codes = parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>();
        assert_eq!(codes, ["L0023"], "{text:?}: {codes:?}");
        assert!(matches!(
            type_ref(&parsed, parameters[0].type_ref),
            TypeRef::Qualified { .. } | TypeRef::Error
        ));
    }

    for (text, expected_span) in [
        ("fun f(x T): R", (8, 9)),
        ("fun f(x = value): R", (8, 9)),
        ("fun f(x): R", (7, 7)),
    ] {
        let diagnostic = parsed(text)
            .1
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.code().to_string() == "L0023")
            .expect("missing parameter colon diagnostic")
            .primary_span();
        assert_eq!(
            (diagnostic.start(), diagnostic.end()),
            expected_span,
            "{text:?}"
        );
    }

    for text in ["fun f() R = 1", "fun f() = 1"] {
        let (_, parsed) = parsed(text);
        let Item::Function {
            form:
                FunctionForm::Explicit {
                    type_ref: return_type,
                    ..
                },
            ..
        } = item(&parsed)
        else {
            panic!("{text:?}: function")
        };
        assert!(matches!(
            type_ref(&parsed, *return_type),
            TypeRef::Qualified { .. } | TypeRef::Error
        ));
        let codes = parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>();
        assert_eq!(codes.iter().filter(|code| *code == "L0021").count(), 1);
        assert!(
            !codes.iter().any(|code| code == "L0014"),
            "{text:?}: {codes:?}"
        );
    }
}

#[test]
fn missing_initializer_fallback_consumes_one_error_region_without_trailing_cascade() {
    let text = "val x ) junk";
    let (_, variable) = parsed(text);
    let Item::Variable { initializer, .. } = item(&variable) else {
        panic!("variable")
    };
    let error = variable
        .ast()
        .expressions()
        .get(*initializer)
        .expect("initializer error");
    assert!(matches!(error.payload(), Expression::Error));
    assert_eq!(
        (error.span().start(), error.span().end()),
        (text.find(')').expect("error start"), text.len())
    );
    assert_eq!(
        variable
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0020"]
    );
}

#[test]
fn missing_return_colon_fallback_preserves_the_expression_body() {
    let text = "fun f() = 1";
    let (_, parsed) = parsed(text);
    let Item::Function {
        form:
            FunctionForm::Explicit {
                colon_span,
                type_ref: return_type,
                body,
            },
        ..
    } = item(&parsed)
    else {
        panic!("function")
    };
    assert_eq!((colon_span.start(), colon_span.end()), (8, 8));
    assert!(matches!(type_ref(&parsed, *return_type), TypeRef::Error));
    let FunctionBody::Expression {
        expression: body, ..
    } = body
    else {
        panic!("preserved expression body")
    };
    assert!(matches!(expression(&parsed, *body), Expression::Literal(_)));
    let codes = parsed
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect::<Vec<_>>();
    assert_eq!(codes, ["L0021"]);
}

#[test]
fn lexer_poison_transfers_to_markers_without_same_span_parser_duplicates() {
    for text in ["const async x = 1", "fun f(async: T): R"] {
        let (_, parsed) = parsed(text);
        let lexical = parsed
            .diagnostics()
            .iter()
            .filter(|diagnostic| {
                diagnostic.code().to_string() == "L0002" || diagnostic.code().to_string() == "L0001"
            })
            .collect::<Vec<_>>();
        assert_eq!(lexical.len(), 1, "{text:?}: {:?}", parsed.diagnostics());
        assert!(
            parsed.diagnostics().iter().all(|parser_diagnostic| {
                lexical.iter().all(|lexical_diagnostic| {
                    parser_diagnostic.code() == lexical_diagnostic.code()
                        || parser_diagnostic.primary_span() != lexical_diagnostic.primary_span()
                })
            }),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
    }
}

#[test]
fn lexer_poison_in_a_missing_separator_region_is_not_reclassified() {
    let text = "fun f(x async T): R";
    let (_, parsed) = parsed(text);
    let lexical = parsed
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code().to_string() == "L0002")
        .expect("reserved-word root cause");
    assert!(parsed.diagnostics().iter().all(|diagnostic| {
        diagnostic.code() == lexical.code() || diagnostic.primary_span() != lexical.primary_span()
    }));
}

#[test]
fn value_parameter_separator_recovery_keeps_the_completed_parameter_and_owner_closer() {
    for text in ["fun f(x: T async y: U): R", "fun f(x: T @ junk): R"] {
        let (_, parsed) = parsed(text);
        let Item::Function { parameters, .. } = item(&parsed) else {
            panic!("{text:?}: function")
        };
        assert!(!parameters.is_empty(), "{text:?}");
        let codes = parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>();
        assert!(
            codes.iter().any(|code| code == "L0025"),
            "{text:?}: {codes:?}"
        );
        assert!(
            !codes
                .iter()
                .any(|code| matches!(code.as_str(), "L0010" | "L0021")),
            "{text:?}: {codes:?}"
        );
        if let Some(lexical) = parsed
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.code().to_string() == "L0002")
        {
            assert!(parsed.diagnostics().iter().all(|diagnostic| {
                diagnostic.code() == lexical.code()
                    || diagnostic.primary_span() != lexical.primary_span()
            }));
        }
    }
}

#[test]
fn invalid_name_region_recovers_to_the_owned_stop_without_losing_the_initializer() {
    let text = "val @ @ = 1";
    let (_, parsed) = parsed(text);
    let Item::Variable {
        name, initializer, ..
    } = item(&parsed)
    else {
        panic!("variable")
    };
    assert!(matches!(name, NameMarker::Error(span) if (span.start(), span.end()) == (4, 7)));
    assert!(matches!(
        expression(&parsed, *initializer),
        Expression::Literal(_)
    ));
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0018"]
    );
}

#[test]
fn type_parameter_eof_does_not_invent_a_list_separator_error() {
    let text = "fun <T";
    let (_, parsed) = parsed(text);
    let codes = parsed
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect::<Vec<_>>();
    assert!(!codes.iter().any(|code| code == "L0025"), "{codes:?}");
}

#[test]
fn unsupported_and_multi_declaration_forms_are_not_silently_accepted() {
    for text in [
        "fun f(x: T = 1): R",
        "fun f((x, y): T): R",
        "fun <T: A & B> f(): R",
        "fun <T> f(): R where T: A",
        "val x = 1\nval y = 2",
    ] {
        let (_, parsed) = parsed(text);
        assert!(!parsed.diagnostics().is_empty(), "{text:?}");
    }
    assert_eq!(
        fingerprints("val x = 1\nval y = 2")
            .last()
            .expect("diagnostic")
            .0,
        "L0013"
    );

    let (_, public) = parsed("public val x = 1");
    assert!(
        public.diagnostics().is_empty(),
        "{:?}",
        public.diagnostics()
    );
    assert!(matches!(item(&public), Item::Modified { .. }));
}

#[test]
fn function_block_body_is_a_typed_statement_and_extends_the_item_span() {
    let text = "fun f(): Unit { x val y = 1 }";
    let (_, parsed) = parsed_ok(text);
    let root = parsed.ast().items().get(parsed.root()).expect("function");
    assert_eq!((root.span().start(), root.span().end()), (0, text.len()));
    let Item::Function {
        form: FunctionForm::Explicit { body, .. },
        ..
    } = root.payload()
    else {
        panic!("function")
    };
    let FunctionBody::Block(block) = body else {
        panic!("block body")
    };
    let block = parsed.ast().statements().get(*block).expect("block");
    let Statement::Block { elements } = block.payload() else {
        panic!("block statement")
    };
    assert_eq!(elements.len(), 2);
    assert!(matches!(
        parsed
            .ast()
            .statements()
            .get(elements[0])
            .expect("local")
            .payload(),
        Statement::Expression { .. }
    ));
    assert!(matches!(
        parsed
            .ast()
            .statements()
            .get(elements[1])
            .expect("expression")
            .payload(),
        Statement::LocalVariable { .. }
    ));
}

#[test]
fn function_block_body_distinguishes_implicit_and_committed_explicit_recovery() {
    let (_, implicit) = parsed_ok("fun f() {}");
    assert!(matches!(
        item(&implicit),
        Item::Function {
            form: FunctionForm::ImplicitUnitBlock(_),
            ..
        }
    ));

    let text = "fun f(): {}";
    let (_, parsed) = parsed(text);
    let Item::Function {
        form:
            FunctionForm::Explicit {
                type_ref: return_type,
                body,
                ..
            },
        ..
    } = item(&parsed)
    else {
        panic!("function")
    };
    assert!(matches!(type_ref(&parsed, *return_type), TypeRef::Error));
    let FunctionBody::Block(block) = body else {
        panic!("block body")
    };
    assert!(matches!(
        parsed
            .ast()
            .statements()
            .get(*block)
            .expect("block")
            .payload(),
        Statement::Block { elements } if elements.is_empty()
    ));
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0014"],
        "{text:?}: {:?}",
        parsed.diagnostics()
    );
}

#[test]
fn committed_function_body_shape_never_backtracks_into_a_second_body() {
    let (_, expression) = parsed("fun f(): Unit = x\n{}");
    let Item::Function {
        form: FunctionForm::Explicit { body, .. },
        ..
    } = item(&expression)
    else {
        panic!("function")
    };
    assert!(matches!(body, FunctionBody::Expression { .. }));
    assert_eq!(
        expression
            .diagnostics()
            .last()
            .expect("trailing block")
            .code()
            .to_string(),
        "L0013"
    );

    let (_, block) = parsed("fun f(): Unit {} = x");
    let Item::Function {
        form: FunctionForm::Explicit { body, .. },
        ..
    } = item(&block)
    else {
        panic!("function")
    };
    assert!(matches!(body, FunctionBody::Block(_)));
    assert_eq!(
        block
            .diagnostics()
            .last()
            .expect("trailing expression body")
            .code()
            .to_string(),
        "L0013"
    );
}

#[test]
fn recovery_respects_owned_closers_nested_delimiters_and_lexical_owners() {
    for text in [
        "fun f(x: T = g([a, b]), y: U): R",
        "fun f(x: T = [a, b): R",
        "fun <T (bad> f(): R",
        "fun <T (bad)> f(): R",
        r#"fun f(x: T = g("${a, b}", [c, d]), y: U): R"#,
        "fun f(x: T = \"a\\\n, y: U): R",
        "fun f(x: T = \"${\"inner\n}tail\", y: U): R",
    ] {
        let (_, parsed) = parsed(text);
        let codes = parsed
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect::<Vec<_>>();
        assert!(
            codes
                .iter()
                .any(|code| matches!(code.as_str(), "L0025" | "L0027")),
            "{text:?}: {codes:?}"
        );
        assert!(
            !codes.iter().any(|code| code == "L0013"),
            "{text:?}: {codes:?}"
        );
    }
}

#[test]
fn default_recovery_keeps_the_parameter_owner_and_next_parameter() {
    for text in [
        "fun f(x: T = g([a, b]), y: U): R",
        r#"fun f(x: T = g("${a, b}", [c, d]), y: U): R"#,
    ] {
        let (_, parsed) = parsed(text);
        let Item::Function { parameters, .. } = item(&parsed) else {
            panic!("{text:?}: function")
        };
        assert_eq!(parameters.len(), 2, "{text:?}: {:?}", parsed.diagnostics());
        let codes = parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>();
        assert_eq!(codes.iter().filter(|code| *code == "L0027").count(), 1);
        assert!(
            !codes.iter().any(|code| matches!(
                code.as_str(),
                "L0010" | "L0013" | "L0014" | "L0021" | "L0025"
            )),
            "{text:?}: {codes:?}"
        );
    }

    let text = "fun f(x: T = [a, b): R";
    let (_, parsed) = parsed(text);
    let Item::Function { parameters, .. } = item(&parsed) else {
        panic!("function")
    };
    assert_eq!(parameters.len(), 1);
    let codes = parsed
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect::<Vec<_>>();
    assert_eq!(codes, ["L0027"]);
    let diagnostic = &parsed.diagnostics()[0];
    assert_eq!(
        diagnostic.primary_span().end(),
        text.find(')').expect("parameter-list closer")
    );
}

#[test]
fn terminal_lexical_recovery_inside_a_default_exits_each_exact_owner() {
    for (text, lexical_code) in [
        (r#"fun f(x: T = "${a"#, "L0005"),
        (r#"fun f(x: T = "${"inner"#, "L0004"),
        ("fun f(x: T = \"abc\\", "L0006"),
    ] {
        let (_, parsed) = parsed(text);
        let codes = parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            codes.iter().filter(|code| *code == "L0027").count(),
            1,
            "{text:?}: {codes:?}"
        );
        assert_eq!(
            codes.iter().filter(|code| *code == lexical_code).count(),
            1,
            "{text:?}: {codes:?}"
        );
        assert!(
            !codes.iter().any(|code| matches!(
                code.as_str(),
                "L0010" | "L0013" | "L0014" | "L0021" | "L0025"
            )),
            "{text:?}: {codes:?}"
        );
    }
}

#[test]
fn source_identity_and_declaration_depth_budget_are_internal_boundaries() {
    let mut owner = SourceMap::new();
    let id = add_source(&mut owner, "val x = 1");
    let lexed = lex_parser_source_twice(&owner, id, "foreign declaration source");
    let mut foreign = SourceMap::new();
    add_source(&mut foreign, "val x = 1");
    assert_parser_error_twice(
        &foreign,
        &lexed,
        ParserInternalError::Source(SourceError::InvalidSourceId { source_id: id }),
        "foreign declaration source",
        parse_declaration,
    );

    let deep = format!("val x: {}T{} = 1", "A<".repeat(1_100), ">".repeat(1_100));
    let mut sources = SourceMap::new();
    let id = add_source(&mut sources, &deep);
    let lexed = lex_parser_source_twice(&sources, id, "declaration nesting budget");
    assert_parser_error_twice(
        &sources,
        &lexed,
        ParserInternalError::NestingLimitExceeded { limit: 1024 },
        "declaration nesting budget",
        parse_declaration,
    );
}

#[test]
fn contextual_keywords_parse_as_declaration_and_parameter_names() {
    for text in [
        "val value = 1",
        "val loop = 2",
        "val move = 3",
        "val borrow = 4",
        "val inout = 5",
        "val own = 6",
        "fun own(): Unit = Unit",
        "fun loop(): Unit = Unit",
        "fun value(value: Int, own: Int, borrow: Int, inout: Int): Int = value",
    ] {
        let (_, parsed) = parsed(text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
    }
}
