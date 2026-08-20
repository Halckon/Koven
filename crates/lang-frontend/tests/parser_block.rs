//! SPEC-0009 的独立 block、statement 顺序与 owner 恢复测试。

use lang_frontend::{
    ast::StatementId,
    diagnostic::{Diagnostic, Severity},
    lexer::lex,
    parser::{
        Expression, Item, ParsedBlock, ParserInternalError, Statement, VariableKind, parse_block,
    },
    source::{SourceId, SourceMap},
};

fn add_source(sources: &mut SourceMap, text: &str) -> SourceId {
    sources.add_source("case.ko", text).expect("unique source")
}

fn parsed(text: &str) -> (SourceMap, ParsedBlock) {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, text);
    let lexed = lex(&sources, source_id).expect("lex");
    let parsed = parse_block(&sources, &lexed).expect("parse");
    (sources, parsed)
}

fn parsed_ok(text: &str) -> (SourceMap, ParsedBlock) {
    let result = parsed(text);
    assert!(
        result.1.diagnostics().is_empty(),
        "{text:?}: {:?}",
        result.1.diagnostics()
    );
    result
}

fn statement(parsed: &ParsedBlock, id: StatementId) -> &Statement {
    parsed
        .ast()
        .statements()
        .get(id)
        .expect("statement")
        .payload()
}

fn root_elements(parsed: &ParsedBlock) -> &[StatementId] {
    let Statement::Block { elements } = statement(parsed, parsed.root()) else {
        panic!("block root")
    };
    elements
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
fn empty_and_nested_blocks_preserve_typed_roots_and_exact_spans() {
    let (_, empty) = parsed_ok("{}");
    assert_eq!(empty.source_id(), empty.ast().source_id());
    let empty_root = empty.ast().statements().get(empty.root()).expect("root");
    assert_eq!((empty_root.span().start(), empty_root.span().end()), (0, 2));
    assert!(root_elements(&empty).is_empty());

    let (_, nested) = parsed_ok("{{}}");
    let elements = root_elements(&nested);
    assert_eq!(elements.len(), 1);
    let child = nested.ast().statements().get(elements[0]).expect("nested");
    assert_eq!((child.span().start(), child.span().end()), (1, 3));
    assert!(matches!(child.payload(), Statement::Block { elements } if elements.is_empty()));
}

#[test]
fn local_variables_expressions_and_nested_blocks_keep_source_order() {
    let text = "{ x + y val x = 1 var y: Int = 2 {} }";
    let (_, parsed) = parsed_ok(text);
    let elements = root_elements(&parsed);
    assert_eq!(elements.len(), 4);

    let Statement::Expression { expression } = statement(&parsed, elements[0]) else {
        panic!("expression statement")
    };
    assert!(matches!(
        parsed
            .ast()
            .expressions()
            .get(*expression)
            .expect("expression")
            .payload(),
        Expression::Binary { .. }
    ));
    for (id, expected_kind) in [
        (elements[1], VariableKind::Val),
        (elements[2], VariableKind::Var),
    ] {
        let Statement::LocalVariable { declaration } = statement(&parsed, id) else {
            panic!("local variable")
        };
        assert!(matches!(
            parsed
                .ast()
                .items()
                .get(*declaration)
                .expect("declaration")
                .payload(),
            Item::Variable { kind, .. } if *kind == expected_kind
        ));
    }
    assert!(matches!(
        statement(&parsed, elements[3]),
        Statement::Block { elements } if elements.is_empty()
    ));
}

#[test]
fn trivia_never_creates_or_removes_statement_boundaries() {
    for text in [
        "{val x=1 val y=2}",
        "{\n val x = 1\n // boundary-looking trivia\n val y = 2\n}",
        "{/*a*/val/*b*/x/*c*/=/*d*/1/*e*/val y=2}",
    ] {
        let (_, parsed) = parsed_ok(text);
        assert_eq!(root_elements(&parsed).len(), 2, "{text:?}");
    }
}

#[test]
fn adjacent_expression_starters_are_not_silently_split() {
    let (_, parsed) = parsed("{ x y }");
    assert_eq!(root_elements(&parsed).len(), 1);
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0013"]
    );
}

#[test]
fn initializer_does_not_stop_at_an_ordinary_expression_starter() {
    let (_, parsed) = parsed("{ val x = 1 x }");
    let elements = root_elements(&parsed);
    assert_eq!(elements.len(), 1);
    assert!(matches!(
        statement(&parsed, elements[0]),
        Statement::LocalVariable { .. }
    ));
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0013"]
    );
}

#[test]
fn expected_block_covers_only_the_first_token_but_error_root_consumes_the_region() {
    let text = "answer trailing   ";
    let (_, result) = parsed(text);
    assert_eq!(
        fingerprints(text),
        [(
            "L0028".to_owned(),
            Severity::Error,
            "expected block".to_owned(),
            0,
            6
        )]
    );
    let root = result.ast().statements().get(result.root()).expect("root");
    assert_eq!((root.span().start(), root.span().end()), (0, 15));
    assert!(matches!(root.payload(), Statement::Error));

    let (_, eof) = parsed("");
    let diagnostic = eof.diagnostics().first().expect("expected block");
    assert_eq!(diagnostic.code().to_string(), "L0028");
    assert!(diagnostic.primary_span().is_empty());
    assert!(
        eof.ast()
            .statements()
            .get(eof.root())
            .expect("error root")
            .span()
            .is_empty()
    );
}

#[test]
fn lexer_poison_is_not_duplicated_as_expected_block_or_element() {
    for text in ["async", "{ async }"] {
        let (_, parsed) = parsed(text);
        let codes = parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>();
        assert_eq!(codes, ["L0002"], "{text:?}: {codes:?}");
    }
}

#[test]
fn unsupported_elements_have_the_dedicated_stable_diagnostic() {
    for text in [
        "{ const val }",
        "{ fun }",
        "{ return }",
        "{ break }",
        "{ continue }",
        "{ if }",
        "{ when }",
        "{ super }",
        "{ for }",
        "{ while }",
        "{ loop }",
        "{ value class }",
        "{ class }",
        "{ interface }",
        "{ enum class }",
        "{ object }",
        "{ companion object }",
    ] {
        let (_, parsed) = parsed(text);
        let actual = parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.code().to_string(),
                    diagnostic.severity(),
                    diagnostic.message().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual[0].0, "L0030", "{text:?}: {actual:?}");
        assert_eq!(actual[0].1, Severity::Error);
        assert_eq!(actual[0].2, "unsupported block element");
        assert!(matches!(
            statement(&parsed, root_elements(&parsed)[0]),
            Statement::Error
        ));
    }
}

#[test]
fn unknown_non_expression_token_uses_expected_element_and_recovers() {
    let text = "{ ) val y = 2 }";
    let (_, parsed) = parsed(text);
    let elements = root_elements(&parsed);
    assert_eq!(elements.len(), 2);
    assert!(matches!(statement(&parsed, elements[0]), Statement::Error));
    assert!(matches!(
        statement(&parsed, elements[1]),
        Statement::LocalVariable { .. }
    ));
    assert_eq!(
        fingerprints(text)[0],
        (
            "L0029".to_owned(),
            Severity::Error,
            "expected block element".to_owned(),
            2,
            3,
        )
    );
}

#[test]
fn semicolon_is_a_parser_error_inside_blocks_not_a_lexer_error_or_separator() {
    let text = "{ ; val y = 2 }";
    let (_, parsed) = parsed(text);
    assert_eq!(root_elements(&parsed).len(), 2);
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0029"]
    );
}

#[test]
fn local_name_recovery_preserves_the_owner_closer_and_next_declaration() {
    for (text, expected_elements) in [("{ val + }", 1), ("{ val + val y = 2 }", 2)] {
        let (_, parsed) = parsed(text);
        let elements = root_elements(&parsed);
        assert_eq!(elements.len(), expected_elements, "{text:?}");
        assert!(
            elements
                .iter()
                .all(|id| matches!(statement(&parsed, *id), Statement::LocalVariable { .. }))
        );
        assert_eq!(
            parsed
                .ast()
                .statements()
                .get(parsed.root())
                .expect("block")
                .span()
                .end(),
            text.len(),
            "{text:?}"
        );
        assert!(
            parsed
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0018"),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
    }
}

#[test]
fn missing_initializer_preserves_each_structural_boundary() {
    for (text, expected_elements) in [
        ("{ val x = }", 1),
        ("{ val x = val y = 2 }", 2),
        ("{ val x = return }", 2),
    ] {
        let (_, parsed) = parsed(text);
        assert_eq!(root_elements(&parsed).len(), expected_elements, "{text:?}");
        let expected_expression = parsed
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code().to_string() == "L0009")
            .collect::<Vec<_>>();
        assert_eq!(
            expected_expression.len(),
            1,
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        assert!(expected_expression[0].primary_span().is_empty());
        assert!(
            !parsed
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0029")
        );
    }

    let (_, lambda_initializer) = parsed_ok("{ val x = {} }");
    assert_eq!(root_elements(&lambda_initializer).len(), 1);
}

#[test]
fn lexical_braces_and_element_like_text_do_not_close_or_split_a_block() {
    for text in [r#"{ "} val return" }"#, r#"{ "${answer}" val result = 1 }"#] {
        let (_, parsed) = parsed_ok(text);
        let expected = if text.contains("result") { 2 } else { 1 };
        assert_eq!(root_elements(&parsed).len(), expected, "{text:?}");
    }
}

#[test]
fn terminal_lexer_owner_errors_do_not_cascade_into_block_diagnostics() {
    for (text, expected_code) in [
        ("{ \"abc", "L0004"),
        (r#"{ "${answer"#, "L0005"),
        ("{ \"abc\\", "L0006"),
    ] {
        let (_, parsed) = parsed(text);
        let codes = parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            codes,
            [expected_code],
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
        assert!(
            !codes
                .iter()
                .any(|code| matches!(code.as_str(), "L0010" | "L0029" | "L0030"))
        );
    }
}

#[test]
fn element_like_tokens_inside_expression_delimiters_do_not_leak_to_dispatch() {
    let text = "{ ::(val x = 1) val y = 2 }";
    let (_, parsed) = parsed(text);
    let elements = root_elements(&parsed);
    assert_eq!(elements.len(), 2, "{:?}", parsed.diagnostics());
    assert!(matches!(
        statement(&parsed, elements[0]),
        Statement::Expression { .. }
    ));
    assert!(matches!(
        statement(&parsed, elements[1]),
        Statement::LocalVariable { .. }
    ));
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code().to_string() == "L0018")
            .count(),
        0
    );
}

#[test]
fn element_like_tokens_inside_type_delimiters_do_not_become_local_declarations() {
    for text in ["{ val x: A<val> = 1 }", "{ val x: (val) -> R = 1 }"] {
        let (_, parsed) = parsed(text);
        let elements = root_elements(&parsed);
        assert_eq!(elements.len(), 1, "{text:?}: {:?}", parsed.diagnostics());
        assert!(matches!(
            statement(&parsed, elements[0]),
            Statement::LocalVariable { .. }
        ));
        assert_eq!(parsed.ast().items().len(), 1, "{text:?}");
        assert!(!parsed.diagnostics().is_empty(), "{text:?}");
    }
}

#[test]
fn braces_inside_generic_type_recover_without_leaking_a_nested_statement() {
    let text = "{ val x: A<{}> = 1 }";
    let (_, parsed) = parsed(text);
    let elements = root_elements(&parsed);
    assert_eq!(elements.len(), 1, "{:?}", parsed.diagnostics());
    assert!(matches!(
        statement(&parsed, elements[0]),
        Statement::LocalVariable { .. }
    ));
    assert_eq!(parsed.ast().items().len(), 1);
    assert_eq!(parsed.ast().statements().len(), 2);
    assert_eq!(
        parsed
            .ast()
            .statements()
            .iter()
            .filter(|(_, node)| matches!(node.payload(), Statement::Block { .. }))
            .count(),
        1,
        "only the real outer block may exist"
    );
    let codes = parsed
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect::<Vec<_>>();
    assert!(codes.iter().any(|code| code == "L0014"), "{codes:?}");
    assert!(
        !codes
            .iter()
            .any(|code| matches!(code.as_str(), "L0029" | "L0030")),
        "{codes:?}"
    );
}

#[test]
fn left_brace_after_cast_operator_stays_owned_by_block_dispatch() {
    let text = "{ x as {} }";
    let (_, parsed) = parsed(text);
    let elements = root_elements(&parsed);
    assert_eq!(elements.len(), 2, "{:?}", parsed.diagnostics());
    assert!(matches!(
        statement(&parsed, elements[0]),
        Statement::Expression { .. }
    ));
    assert!(matches!(
        statement(&parsed, elements[1]),
        Statement::Block { .. }
    ));
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0014"]
    );
}

#[test]
fn nested_closer_belongs_to_the_innermost_owner() {
    let text = "{{}";
    let (_, parsed) = parsed(text);
    assert_eq!(
        fingerprints(text),
        [(
            "L0010".to_owned(),
            Severity::Error,
            "expected closing delimiter".to_owned(),
            3,
            3
        )]
    );
    let outer = parsed.ast().statements().get(parsed.root()).expect("outer");
    assert_eq!((outer.span().start(), outer.span().end()), (0, 3));
    let inner_id = root_elements(&parsed)[0];
    let inner = parsed.ast().statements().get(inner_id).expect("inner");
    assert_eq!((inner.span().start(), inner.span().end()), (1, 3));
}

#[test]
fn completed_block_reuses_the_existing_trailing_token_diagnostic() {
    assert_eq!(
        fingerprints("{} tail"),
        [(
            "L0013".to_owned(),
            Severity::Error,
            "unexpected trailing token".to_owned(),
            3,
            7
        )]
    );
}

#[test]
fn source_identity_and_nested_block_budget_are_internal_boundaries() {
    let mut owner = SourceMap::new();
    let id = add_source(&mut owner, "{}");
    let lexed = lex(&owner, id).expect("lex");
    let mut foreign = SourceMap::new();
    add_source(&mut foreign, "{}");
    assert!(matches!(
        parse_block(&foreign, &lexed),
        Err(ParserInternalError::Source(_))
    ));

    let deep = format!("{}{}", "{".repeat(1_100), "}".repeat(1_100));
    let mut sources = SourceMap::new();
    let id = add_source(&mut sources, &deep);
    let lexed = lex(&sources, id).expect("lex");
    assert!(matches!(
        parse_block(&sources, &lexed),
        Err(ParserInternalError::NestingLimitExceeded { limit: 1024 })
    ));
}
