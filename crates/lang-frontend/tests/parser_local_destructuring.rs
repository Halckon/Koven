//! SPEC-0013 / SPEC-0122 局部 `val` 解构的公共 AST、诊断与恢复契约测试。

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    diagnostic::Diagnostic,
    parser::{
        Expression, Item, NameMarker, ParsedBlock, ParsedDeclaration, ParsedExpression, Statement,
    },
    source::{SourceId, SourceMap, Span},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{parse_block_twice, parse_declaration_twice, parse_expression_twice};

fn add_source(sources: &mut SourceMap, name: &str, text: &str) -> SourceId {
    sources.add_source(name, text).expect("unique source")
}

fn parsed_block(text: &str) -> (SourceMap, ParsedBlock) {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "block.ko", text);
    let parsed = parse_block_twice(&sources, source_id, text);
    (sources, parsed)
}

fn parsed_expression(text: &str) -> (SourceMap, ParsedExpression) {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "expression.ko", text);
    let parsed = parse_expression_twice(&sources, source_id, text);
    (sources, parsed)
}

fn parsed_declaration(text: &str) -> (SourceMap, ParsedDeclaration) {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "declaration.ko", text);
    let parsed = parse_declaration_twice(&sources, source_id, text);
    (sources, parsed)
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

fn marker_span(marker: NameMarker) -> Span {
    match marker {
        NameMarker::Present(span) | NameMarker::Missing(span) | NameMarker::Error(span) => span,
    }
}

fn fingerprints(diagnostics: &[Diagnostic]) -> Vec<(String, String, usize, usize)> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let span = diagnostic.primary_span();
            (
                diagnostic.code().to_string(),
                diagnostic.message().to_owned(),
                span.start(),
                span.end(),
            )
        })
        .collect()
}

fn expression(parsed: &ParsedBlock, id: ExpressionId) -> &Expression {
    parsed
        .ast()
        .expressions()
        .get(id)
        .expect("expression")
        .payload()
}

#[test]
fn block_destructuring_preserves_one_initializer_and_exact_source_order() {
    let text = "{ val (left, right) = pair val z = 1 }";
    let (sources, parsed) = parsed_block(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let elements = root_elements(&parsed);
    assert_eq!(elements.len(), 2);

    let node = parsed
        .ast()
        .statements()
        .get(elements[0])
        .expect("destructuring");
    let Statement::LocalDestructuring {
        val_span,
        left_paren_span,
        bindings,
        right_paren_span: Some(right_paren_span),
        equals_span: Some(equals_span),
        initializer,
    } = node.payload()
    else {
        panic!("local destructuring")
    };
    assert_eq!(sources.slice(*val_span).unwrap(), "val");
    assert_eq!(sources.slice(*left_paren_span).unwrap(), "(");
    assert_eq!(sources.slice(*right_paren_span).unwrap(), ")");
    assert_eq!(sources.slice(*equals_span).unwrap(), "=");
    assert_eq!(bindings.len(), 2);
    assert_eq!(sources.slice(marker_span(bindings[0])).unwrap(), "left");
    assert_eq!(sources.slice(marker_span(bindings[1])).unwrap(), "right");
    assert!(matches!(
        bindings.as_slice(),
        [NameMarker::Present(_), NameMarker::Present(_)]
    ));
    assert!(matches!(
        expression(&parsed, *initializer),
        Expression::Name
    ));
    assert_eq!(
        sources.slice(node.span()).unwrap(),
        "val (left, right) = pair"
    );
}

#[test]
fn lambda_body_dispatch_commits_the_same_local_destructuring_variant() {
    let text = "{ val (a, b) = pair }";
    let (_sources, parsed) = parsed_expression(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let Expression::Lambda { body, .. } = parsed
        .ast()
        .expressions()
        .get(parsed.root())
        .expect("lambda")
        .payload()
    else {
        panic!("lambda root")
    };
    let Statement::LambdaBody { elements } = parsed
        .ast()
        .statements()
        .get(*body)
        .expect("lambda body")
        .payload()
    else {
        panic!("lambda body")
    };
    assert_eq!(elements.len(), 1);
    assert!(matches!(
        parsed
            .ast()
            .statements()
            .get(elements[0])
            .unwrap()
            .payload(),
        Statement::LocalDestructuring { .. }
    ));
}

#[test]
fn l0040_l0041_and_l0044_lock_binding_list_priorities() {
    for (text, code, start, end) in [
        ("{ val () = x }", "L0040", 7, 7),
        ("{ val (,a) = x }", "L0040", 7, 8),
        ("{ val (a b) = x }", "L0041", 9, 9),
        ("{ val (a,) = x }", "L0044", 8, 9),
    ] {
        let (_, parsed) = parsed_block(text);
        let actual = fingerprints(parsed.diagnostics());
        assert!(
            actual
                .iter()
                .any(|item| item.0 == code && item.2 == start && item.3 == end),
            "{text:?}: {actual:?}"
        );
        assert!(matches!(
            statement(&parsed, root_elements(&parsed)[0]),
            Statement::LocalDestructuring { .. }
        ));
    }

    let (_, missing_after_separator) = parsed_block("{ val (a, = x }");
    let actual = fingerprints(missing_after_separator.diagnostics());
    assert!(
        actual
            .iter()
            .any(|item| item.0 == "L0040" && item.2 == 10 && item.3 == 10),
        "{actual:?}"
    );

    let (_, typed_then_missing) = parsed_block("{ val (a: T, = x }");
    let codes = typed_then_missing
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect::<Vec<_>>();
    assert!(codes.iter().any(|code| code == "L0042"), "{codes:?}");
    assert!(codes.iter().any(|code| code == "L0040"), "{codes:?}");
}

#[test]
fn unsupported_patterns_and_body_prefixes_use_l0042_without_constructing_wrong_ast() {
    for text in ["{ val (_) = x }", "{ val ((a)) = x }", "{ val (a: T) = x }"] {
        let (_, parsed) = parsed_block(text);
        assert!(
            fingerprints(parsed.diagnostics())
                .iter()
                .any(|item| item.0 == "L0042"),
            "{text:?}"
        );
        assert!(matches!(
            statement(&parsed, root_elements(&parsed)[0]),
            Statement::LocalDestructuring { .. }
        ));
    }
    for text in [
        "{ var (a) = x val y = 1 }",
        "{ const val (a) = x val y = 1 }",
    ] {
        let (_, parsed) = parsed_block(text);
        assert_eq!(fingerprints(parsed.diagnostics())[0].0, "L0042", "{text:?}");
        let elements = root_elements(&parsed);
        assert_eq!(elements.len(), 2, "{text:?}: {:?}", parsed.diagnostics());
        assert!(matches!(statement(&parsed, elements[0]), Statement::Error));
        assert!(matches!(
            statement(&parsed, elements[1]),
            Statement::LocalVariable { .. }
        ));
    }
}

#[test]
fn independent_declaration_entry_rejects_all_destructuring_contexts_with_l0043() {
    for text in ["val (a) = x", "var (a) = x", "const val (a) = x"] {
        let (_, parsed) = parsed_declaration(text);
        assert_eq!(
            fingerprints(parsed.diagnostics()),
            [(
                "L0043".to_owned(),
                "unsupported destructuring context".to_owned(),
                text.find('(').unwrap(),
                text.find('(').unwrap() + 1
            )],
            "{text:?}"
        );
        assert!(matches!(
            parsed.ast().items().get(parsed.root()).unwrap().payload(),
            Item::Error
        ));
    }
}

#[test]
fn initializer_separator_and_value_recovery_use_l0045_and_l0046() {
    let (_, missing_equals) = parsed_block("{ val (a) x }");
    assert_eq!(
        fingerprints(missing_equals.diagnostics())[0],
        (
            "L0045".to_owned(),
            "expected destructuring initializer separator".to_owned(),
            10,
            10,
        )
    );
    let Statement::LocalDestructuring {
        equals_span,
        initializer,
        ..
    } = statement(&missing_equals, root_elements(&missing_equals)[0])
    else {
        panic!("destructuring")
    };
    assert!(equals_span.is_none());
    assert!(matches!(
        expression(&missing_equals, *initializer),
        Expression::Name
    ));

    let (_, missing_value) = parsed_block("{ val (a) = }");
    assert_eq!(
        fingerprints(missing_value.diagnostics())[0],
        (
            "L0046".to_owned(),
            "expected destructuring initializer".to_owned(),
            12,
            12,
        )
    );
}

#[test]
fn missing_pattern_closer_preserves_equals_and_initializer() {
    let text = "{ val (a = pair }";
    let (_, parsed) = parsed_block(text);
    assert_eq!(fingerprints(parsed.diagnostics())[0].0, "L0010");
    assert!(
        !fingerprints(parsed.diagnostics())
            .iter()
            .any(|item| item.0 == "L0045")
    );
    let Statement::LocalDestructuring {
        right_paren_span,
        equals_span: Some(_),
        initializer,
        ..
    } = statement(&parsed, root_elements(&parsed)[0])
    else {
        panic!("destructuring")
    };
    assert!(right_paren_span.is_none());
    assert!(matches!(
        expression(&parsed, *initializer),
        Expression::Name
    ));
}

#[test]
fn missing_pattern_closer_at_element_boundary_suppresses_initializer_separator_cascade() {
    let text = "{ val (a }";
    let (_, parsed) = parsed_block(text);
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0010"],
        "{:?}",
        parsed.diagnostics()
    );
    let Statement::LocalDestructuring {
        right_paren_span,
        equals_span,
        initializer,
        ..
    } = statement(&parsed, root_elements(&parsed)[0])
    else {
        panic!("destructuring")
    };
    assert!(right_paren_span.is_none());
    assert!(equals_span.is_none());
    assert!(matches!(
        expression(&parsed, *initializer),
        Expression::Error
    ));
}

#[test]
fn recovery_keeps_following_local_element_and_source_identity() {
    let text = "{ val (a: Box<(T, U)>) = x val y = 1 }";
    let (sources, parsed) = parsed_block(text);
    assert!(
        parsed
            .diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0042")
    );
    let elements = root_elements(&parsed);
    assert_eq!(elements.len(), 2, "{:?}", parsed.diagnostics());
    for id in elements {
        assert_eq!(
            parsed
                .ast()
                .statements()
                .get(*id)
                .unwrap()
                .span()
                .source_id(),
            parsed.source_id()
        );
    }
    assert_eq!(
        sources
            .slice(parsed.ast().statements().get(elements[1]).unwrap().span())
            .unwrap(),
        "val y = 1"
    );
}

#[test]
fn lexical_string_roots_inside_patterns_do_not_gain_destructuring_cascades() {
    for (text, lexical_code) in [
        ("{ val (\"bad\\q\", a) = x }", "L0006"),
        ("{ val (\"bad\n, a) = x }", "L0004"),
    ] {
        let (_, parsed) = parsed_block(text);
        let codes = parsed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>();
        assert!(
            codes.iter().any(|code| code == lexical_code),
            "{text:?}: {codes:?}"
        );
        assert!(
            !codes
                .iter()
                .any(|code| matches!(code.as_str(), "L0040" | "L0041" | "L0042")),
            "{text:?}: {codes:?}"
        );
        assert!(matches!(
            statement(&parsed, root_elements(&parsed)[0]),
            Statement::LocalDestructuring { .. }
        ));
    }
}

#[test]
fn nested_unsupported_pattern_stays_inside_its_binding_owner() {
    let text = "{ val ((a, b), c) = pair val z = 1 }";
    let (_, parsed) = parsed_block(text);
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code().to_string() == "L0042")
            .count(),
        1,
        "{:?}",
        parsed.diagnostics()
    );
    let elements = root_elements(&parsed);
    assert_eq!(elements.len(), 2);
    let Statement::LocalDestructuring { bindings, .. } = statement(&parsed, elements[0]) else {
        panic!("destructuring")
    };
    assert!(matches!(
        bindings.as_slice(),
        [NameMarker::Error(_), NameMarker::Present(_)]
    ));
}

#[test]
fn long_binding_lists_are_deterministic_and_keep_every_binding_once() {
    let parse_count = |count| {
        let bindings = (0..count)
            .map(|index| format!("x{index}"))
            .collect::<Vec<_>>();
        let text = format!("{{ val ({}) = source }}", bindings.join(", "));
        let first = parsed_block(&text).1;
        let second = parsed_block(&text).1;
        assert!(first.diagnostics().is_empty());
        assert_eq!(
            fingerprints(first.diagnostics()),
            fingerprints(second.diagnostics())
        );
        let Statement::LocalDestructuring {
            bindings: actual, ..
        } = statement(&first, root_elements(&first)[0])
        else {
            panic!("destructuring")
        };
        assert!(
            actual
                .iter()
                .all(|marker| matches!(marker, NameMarker::Present(_)))
        );
        actual.len()
    };

    let first = parse_count(256);
    let doubled = parse_count(512);
    assert_eq!(first, 256);
    assert_eq!(doubled, first * 2);

    let parse_empty_count = |count| {
        let text = format!("{{ val ({}last) = source }}", ",".repeat(count));
        let parsed = parsed_block(&text).1;
        let diagnostics = parsed
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code().to_string() == "L0040")
            .count();
        let Statement::LocalDestructuring { bindings, .. } =
            statement(&parsed, root_elements(&parsed)[0])
        else {
            panic!("destructuring")
        };
        (diagnostics, bindings.len())
    };
    let error_first = parse_empty_count(128);
    let error_doubled = parse_empty_count(256);
    assert_eq!(error_first, (128, 129));
    assert_eq!(error_doubled.0, error_first.0 * 2);
    assert_eq!(error_doubled.1 - 1, (error_first.1 - 1) * 2);
}

#[test]
fn destructuring_recovery_preserves_an_inherited_mismatched_hard_closer() {
    let text = "items[{ val (a]";
    let (_, parsed) = parsed_expression(text);
    assert!(matches!(
        parsed
            .ast()
            .expressions()
            .get(parsed.root())
            .expect("root")
            .payload(),
        Expression::Index { .. }
    ));
    assert_eq!(
        parsed
            .ast()
            .expressions()
            .get(parsed.root())
            .unwrap()
            .span()
            .end(),
        text.len()
    );
    assert_eq!(
        parsed
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code().to_string() == "L0010")
            .count(),
        2,
        "{:?}",
        parsed.diagnostics()
    );
    assert!(
        !parsed
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0013")
    );
}

#[test]
fn destructuring_spans_keep_the_selected_source_map_identity() {
    let mut sources = SourceMap::new();
    add_source(&mut sources, "unrelated.ko", "{}");
    let text = "{ val (a, b) = pair }";
    let source_id = add_source(&mut sources, "selected.ko", text);
    let parsed = parse_block_twice(&sources, source_id, text);
    assert!(parsed.diagnostics().is_empty());
    let node = parsed
        .ast()
        .statements()
        .get(root_elements(&parsed)[0])
        .expect("destructuring");
    assert_eq!(node.span().source_id(), source_id);
    let Statement::LocalDestructuring {
        bindings,
        initializer,
        ..
    } = node.payload()
    else {
        panic!("destructuring")
    };
    assert!(
        bindings
            .iter()
            .all(|marker| marker_span(*marker).source_id() == source_id)
    );
    assert_eq!(
        parsed
            .ast()
            .expressions()
            .get(*initializer)
            .unwrap()
            .span()
            .source_id(),
        source_id
    );
}
