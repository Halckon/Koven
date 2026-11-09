//! An index owns its closing bracket, including when its key is malformed.

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{BinaryOperator, Expression, Item, NameMarker, Statement, StringPart, SyntaxAst},
    source::SourceMap,
};
use parser_test_assertions::{parse_block_twice, parse_expression_twice, parse_file_twice};

fn assert_index(
    sources: &SourceMap,
    ast: &SyntaxAst,
    id: ExpressionId,
    indexed: &str,
    key: &str,
    key_is_error: bool,
) {
    let node = ast.expressions().get(id).unwrap();
    assert_eq!(sources.slice(node.span()), Ok(indexed));
    let Expression::Index { receiver, index } = node.payload() else {
        panic!("expected index, got {:?}", node.payload());
    };
    let receiver = ast.expressions().get(*receiver).unwrap();
    assert_eq!(sources.slice(receiver.span()), Ok("xs"));
    let index = ast.expressions().get(*index).unwrap();
    assert_eq!(sources.slice(index.span()), Ok(key));
    if key_is_error {
        assert_eq!(index.payload(), &Expression::Error);
    } else {
        assert!(matches!(index.payload(), Expression::Name));
    }
}

fn assert_call_with_index(
    text: &str,
    indexed: &str,
    key: &str,
    key_is_error: bool,
    expected_diagnostics: &[(&str, &str)],
) {
    let mut sources = SourceMap::new();
    let id = sources.add_source("index-recovery.ko", text).unwrap();
    let parsed = parse_expression_twice(&sources, id, text);
    let diagnostics = parsed
        .diagnostics()
        .iter()
        .map(|d| {
            (
                d.code().to_string(),
                sources.slice(d.primary_span()).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let root = parsed.ast().expressions().get(parsed.root()).unwrap();
    let call = match root.payload() {
        Expression::String { parts } => {
            let [
                StringPart::Interpolation { expression, .. },
                StringPart::Text(tail),
            ] = parts.as_slice()
            else {
                panic!("expected interpolation and string tail, got {parts:?}");
            };
            assert_eq!(sources.slice(*tail), Ok("tail"));
            parsed.ast().expressions().get(*expression).unwrap()
        }
        _ => root,
    };
    let Expression::Call { arguments, .. } = call.payload() else {
        panic!("expected outer call, got {:?}", call.payload());
    };
    let [first, next] = arguments.as_slice() else {
        panic!("expected two arguments, got {arguments:?}");
    };
    assert_index(
        &sources,
        parsed.ast(),
        first.value,
        indexed,
        key,
        key_is_error,
    );
    assert_eq!(sources.slice(next.span), Ok("next"));
    assert_eq!(
        diagnostics
            .iter()
            .map(|(code, text)| (code.as_str(), *text))
            .collect::<Vec<_>>(),
        expected_diagnostics
    );
}

#[test]
fn comma_recovery_skips_the_bracket_in_a_nested_index() {
    assert_call_with_index(
        "f(xs[key, nested[item]], next)",
        "xs[key, nested[item]]",
        "key",
        false,
        &[("L0010", ",")],
    );
}

#[test]
fn comma_recovery_skips_the_bracket_in_an_interpolation() {
    assert_call_with_index(
        r#"f(xs[key, "${nested[item]}"], next)"#,
        r#"xs[key, "${nested[item]}"]"#,
        "key",
        false,
        &[("L0010", ",")],
    );
}

#[test]
fn unpaired_index_returns_the_first_caller_comma() {
    assert_call_with_index("f(xs[key, next)", "xs[key", "key", false, &[("L0010", "")]);
}

#[test]
fn malformed_key_suffix_is_not_reinterpreted_as_a_call_argument() {
    for (text, indexed, key, diagnostics) in [
        (
            "f(xs[key stray], next)",
            "xs[key stray]",
            "key stray",
            &[("L0010", "stray")][..],
        ),
        (
            "f(xs[key + β stray], next)",
            "xs[key + β stray]",
            "key + β stray",
            &[("L0001", "β"), ("L0010", "stray")][..],
        ),
    ] {
        assert_call_with_index(text, indexed, key, true, diagnostics);
    }
}

#[test]
fn unpaired_file_index_recovers_the_nested_suffix_and_leaves_the_next_declaration() {
    for (text, indexed) in [
        (
            "val x = xs[key, nested[item] + extra\nval next = 1",
            "xs[key, nested[item] + extra",
        ),
        ("val x = xs[key, nested(\nval next = 1", "xs[key, nested("),
        ("val x = xs[key, nested(;\nval next = 1", "xs[key, nested("),
        (
            "val x = xs[key, nested({ val inner = 0 })\nval next = 1",
            "xs[key, nested({ val inner = 0 })",
        ),
    ] {
        let mut sources = SourceMap::new();
        let id = sources.add_source("index-recovery.ko", text).unwrap();
        let parsed = parse_file_twice(&sources, id, text);
        let [first, next] = parsed.roots() else {
            panic!(
                "expected two file roots for {text:?}, got {:?}",
                parsed.roots()
            );
        };
        let ast = parsed.ast();
        let Item::Variable { initializer, .. } = ast.items().get(*first).unwrap().payload() else {
            panic!("expected variable declaration");
        };
        assert_index(&sources, ast, *initializer, indexed, "key", false);
        assert_eq!(
            sources.slice(ast.items().get(*next).unwrap().span()),
            Ok("val next = 1")
        );
        let [diagnostic] = parsed.diagnostics() else {
            panic!("expected one closing diagnostic for {text:?}");
        };
        assert_eq!(diagnostic.code().to_string(), "L0010");
        assert_eq!(sources.slice(diagnostic.primary_span()), Ok(","));
    }
}

#[test]
fn paired_poisoned_index_preserves_the_next_call_argument() {
    for (text, indexed, key) in [
        ("f(xs[key β], next)", "xs[key β]", "key β"),
        (
            "f(xs[β + key stray], next)",
            "xs[β + key stray]",
            "β + key stray",
        ),
        (r#""${f(xs[key β], next)}tail""#, "xs[key β]", "key β"),
    ] {
        assert_call_with_index(text, indexed, key, true, &[("L0001", "β")]);
    }
}

#[test]
fn paired_poisoned_index_preserves_the_binary_suffix_and_next_declaration() {
    let mut sources = SourceMap::new();
    let text = "val x = xs[key async] + tail\nval next = 1";
    let id = sources.add_source("index-recovery.ko", text).unwrap();
    let parsed = parse_file_twice(&sources, id, text);
    let [first, next] = parsed.roots() else {
        panic!("expected two file roots, got {:?}", parsed.roots());
    };
    let ast = parsed.ast();
    assert_eq!(
        sources.slice(ast.items().get(*next).unwrap().span()),
        Ok("val next = 1")
    );
    let Item::Variable { initializer, .. } = ast.items().get(*first).unwrap().payload() else {
        panic!("expected variable declaration");
    };
    let Expression::Binary {
        operator: BinaryOperator::Add,
        left,
        right,
        ..
    } = ast.expressions().get(*initializer).unwrap().payload()
    else {
        panic!("expected the index to be followed by addition");
    };
    assert_index(&sources, ast, *left, "xs[key async]", "key async", true);
    assert_eq!(
        sources.slice(ast.expressions().get(*right).unwrap().span()),
        Ok("tail")
    );
    let [diagnostic] = parsed.diagnostics() else {
        panic!("expected the lexical diagnostic only");
    };
    assert_eq!(diagnostic.code().to_string(), "L0002");
    assert_eq!(sources.slice(diagnostic.primary_span()), Ok("async"));
}

#[test]
fn a_nested_bracket_after_a_bad_type_is_not_the_index_closer() {
    assert_call_with_index(
        "f(xs[key as []], next)",
        "xs[key as []]",
        "key as []",
        true,
        &[("L0014", "[")],
    );
}

#[test]
fn unpaired_index_keeps_a_proven_lambda_local_and_the_block_separator_outer() {
    let mut sources = SourceMap::new();
    let text = "{ xs[key, { -> 1 }; next }";
    let id = sources.add_source("index-recovery.ko", text).unwrap();
    let parsed = parse_block_twice(&sources, id, text);
    let ast = parsed.ast();
    let Statement::Block { elements } = ast.statements().get(parsed.root()).unwrap().payload()
    else {
        panic!("expected block");
    };
    let [first, next] = elements.as_slice() else {
        panic!("expected two statements, got {elements:?}");
    };
    let Statement::Expression { expression } = ast.statements().get(*first).unwrap().payload()
    else {
        panic!("expected index statement");
    };
    assert_index(&sources, ast, *expression, "xs[key, { -> 1 }", "key", false);
    assert_eq!(
        sources.slice(ast.statements().get(*next).unwrap().span()),
        Ok("next")
    );
    let [diagnostic] = parsed.diagnostics() else {
        panic!(
            "expected the index diagnostic only: {:?}",
            parsed.diagnostics()
        );
    };
    assert_eq!(diagnostic.code().to_string(), "L0010");
    assert_eq!(sources.slice(diagnostic.primary_span()), Ok(","));
}

#[test]
fn adjacent_poison_stays_with_the_unpaired_key_before_a_caller_comma() {
    for (text, indexed, key, diagnostics) in [
        (
            "f(xs[key β, next)",
            "xs[key β",
            "key β",
            &[("L0001", "β"), ("L0010", "")][..],
        ),
        (
            "f(xs[key async, next)",
            "xs[key async",
            "key async",
            &[("L0002", "async"), ("L0010", "")][..],
        ),
        (
            "f(xs[key β async, next)",
            "xs[key β async",
            "key β async",
            &[("L0001", "β"), ("L0002", "async"), ("L0010", "")][..],
        ),
        (
            r#""${f(xs[key β, next)}tail""#,
            "xs[key β",
            "key β",
            &[("L0001", "β"), ("L0010", "")][..],
        ),
    ] {
        assert_call_with_index(text, indexed, key, true, diagnostics);
    }
}

#[test]
fn adjacent_poison_before_a_file_boundary_does_not_become_a_file_root() {
    let mut sources = SourceMap::new();
    let text = "val x = xs[key β\nval next = 1";
    let id = sources.add_source("index-recovery.ko", text).unwrap();
    let parsed = parse_file_twice(&sources, id, text);
    let ast = parsed.ast();
    let [first, next] = parsed.roots() else {
        panic!("expected two declarations, got {:?}", parsed.roots());
    };
    let first = ast.items().get(*first).unwrap();
    let Item::Variable { initializer, .. } = first.payload() else {
        panic!("expected first variable");
    };
    assert_index(&sources, ast, *initializer, "xs[key β", "key β", true);
    assert_eq!(
        sources.slice(ast.items().get(*next).unwrap().span()),
        Ok("val next = 1")
    );
    let [lexical, closing] = parsed.diagnostics() else {
        panic!("expected lexical and missing bracket diagnostics");
    };
    assert_eq!(lexical.code().to_string(), "L0001");
    assert_eq!(closing.code().to_string(), "L0010");
    assert_eq!(
        closing.primary_span().start(),
        text.find("val next").unwrap()
    );
    assert!(closing.primary_span().is_empty());
}

fn local_names<'a>(
    sources: &'a SourceMap,
    ast: &SyntaxAst,
    elements: &[StatementId],
) -> Vec<&'a str> {
    elements
        .iter()
        .filter_map(|element| {
            let Statement::LocalVariable { declaration } =
                ast.statements().get(*element).unwrap().payload()
            else {
                return None;
            };
            let Item::Variable {
                name: NameMarker::Present(name),
                ..
            } = ast.items().get(*declaration).unwrap().payload()
            else {
                return None;
            };
            Some(sources.slice(*name).unwrap())
        })
        .collect()
}

#[test]
fn a_paired_index_leaves_the_block_semicolon_and_next_declaration() {
    for (key, key_is_error) in [("key", false), ("key β", true), ("β", true), ("", true)] {
        let text = format!("{{ xs[{key}; val saved = 1\n]\nval later = saved }}");
        let mut sources = SourceMap::new();
        let id = sources.add_source("index-semicolon.ko", &text).unwrap();
        let parsed = parse_block_twice(&sources, id, &text);
        let ast = parsed.ast();
        let Statement::Block { elements } = ast.statements().get(parsed.root()).unwrap().payload()
        else {
            panic!("expected block");
        };
        let Statement::Expression { expression } =
            ast.statements().get(elements[0]).unwrap().payload()
        else {
            panic!("expected first index expression");
        };
        assert_index(
            &sources,
            ast,
            *expression,
            &format!("xs[{key}"),
            key,
            key_is_error,
        );
        assert_eq!(local_names(&sources, ast, elements), ["saved", "later"]);
        let closing = parsed
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.code().to_string() == "L0010")
            .unwrap();
        assert_eq!(closing.primary_span().start(), text.find(';').unwrap());
        assert!(closing.primary_span().is_empty());
    }
}

#[test]
fn nested_paired_indexes_leave_the_inherited_block_separator_after_poison() {
    for key in ["β", "key β", ""] {
        let text = format!("{{ xs[ys[{key}; val saved = 1\n]]\nval later = saved }}");
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("nested-index-semicolon.ko", &text)
            .unwrap();
        let parsed = parse_block_twice(&sources, id, &text);
        let ast = parsed.ast();
        let Statement::Block { elements } = ast.statements().get(parsed.root()).unwrap().payload()
        else {
            panic!("expected block");
        };
        assert_eq!(
            local_names(&sources, ast, elements),
            ["saved", "later"],
            "{text}"
        );
        let first = ast.statements().get(elements[0]).unwrap();
        assert_eq!(
            sources.slice(first.span()),
            Ok(&text[2..text.find(';').unwrap()])
        );
    }
}

#[test]
fn a_paired_index_comma_tail_cannot_hide_a_block_separator_in_a_nested_index() {
    let text = "{ xs[key, { -> val local = 1; val inner = 2 }, junk[other; /* next element */ var saved = 1\n], rest]\nval later = saved }";
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("index-comma-semicolon.ko", text)
        .unwrap();
    let parsed = parse_block_twice(&sources, id, text);
    let ast = parsed.ast();
    let Statement::Block { elements } = ast.statements().get(parsed.root()).unwrap().payload()
    else {
        panic!("expected block");
    };
    assert_eq!(local_names(&sources, ast, elements), ["saved", "later"]);
    let Statement::Expression { expression } = ast.statements().get(elements[0]).unwrap().payload()
    else {
        panic!("expected first index expression");
    };
    assert_index(
        &sources,
        ast,
        *expression,
        "xs[key, { -> val local = 1; val inner = 2 }, junk[other",
        "key",
        false,
    );
}

#[test]
fn a_paired_index_comma_tail_leaves_recognizable_block_statements() {
    for (statement, is_expected) in [
        (
            "borrow /* binding */ val saved = source",
            (|node: &Statement| matches!(node, Statement::LocalVariable { .. }))
                as fn(&Statement) -> bool,
        ),
        ("while (flag) { work() }", |node| {
            matches!(node, Statement::While { .. })
        }),
        ("for (item in source) { work() }", |node| {
            matches!(node, Statement::For { .. })
        }),
        ("loop { break }", |node| {
            matches!(node, Statement::Loop { .. })
        }),
    ] {
        let text = format!("{{ xs[key, 2; {statement}\n]\nval later = 0 }}");
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("index-statement-separator.ko", &text)
            .unwrap();
        let parsed = parse_block_twice(&sources, id, &text);
        let ast = parsed.ast();
        let Statement::Block { elements } = ast.statements().get(parsed.root()).unwrap().payload()
        else {
            panic!("expected block");
        };
        let next = ast.statements().get(elements[1]).unwrap();
        assert_eq!(sources.slice(next.span()), Ok(statement));
        assert!(
            is_expected(next.payload()),
            "{statement}: {:?}",
            next.payload()
        );
        assert_eq!(
            sources.slice(
                ast.statements()
                    .get(*elements.last().unwrap())
                    .unwrap()
                    .span()
            ),
            Ok("val later = 0")
        );
    }
}

#[test]
fn an_index_in_a_call_argument_leaves_the_enclosing_block_declaration() {
    for (prefix, key, closers) in [
        ("f(xs[", "key", "])"),
        ("f(g(xs[", "β", "]))"),
        ("f(input = xs[", "", "])"),
        ("f(xs[", "key β", "])"),
        ("f(xs[", "key +", "])"),
        ("f(xs[", "key as", "])"),
        ("f(xs[", "key is", "])"),
        ("f(xs[", "key as List<", "])"),
        ("f(xs[", "key, junk[other", "]])"),
    ] {
        let before_separator = format!("{prefix}{key}");
        let text = format!("{{ {before_separator}; val saved = 1\n{closers}\nval later = saved }}");
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("index-argument-boundary.ko", &text)
            .unwrap();
        let parsed = parse_block_twice(&sources, id, &text);
        let ast = parsed.ast();
        let Statement::Block { elements } = ast.statements().get(parsed.root()).unwrap().payload()
        else {
            panic!("expected block");
        };
        assert_eq!(
            local_names(&sources, ast, elements),
            ["saved", "later"],
            "{text}"
        );
        assert_eq!(
            sources.slice(ast.statements().get(elements[0]).unwrap().span()),
            Ok(before_separator.as_str())
        );
    }
}

#[test]
fn an_index_in_a_call_argument_keeps_an_internal_invalid_semicolon() {
    for key in ["key", "key as"] {
        let text = format!("{{ f(xs[{key}; 2], next); tail }}");
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("index-argument-local-tail.ko", &text)
            .unwrap();
        let parsed = parse_block_twice(&sources, id, &text);
        let ast = parsed.ast();
        let Statement::Block { elements } = ast.statements().get(parsed.root()).unwrap().payload()
        else {
            panic!("expected block");
        };
        let [call, tail] = elements.as_slice() else {
            panic!("expected call and following statement: {elements:?}");
        };
        let Statement::Expression { expression } = ast.statements().get(*call).unwrap().payload()
        else {
            panic!("expected expression");
        };
        let Expression::Call { arguments, .. } =
            ast.expressions().get(*expression).unwrap().payload()
        else {
            panic!("expected call");
        };
        assert_eq!(arguments.len(), 2);
        assert_eq!(
            sources.slice(arguments[0].span),
            Ok(format!("xs[{key}; 2]").as_str())
        );
        assert_eq!(sources.slice(arguments[1].span), Ok("next"));
        assert_eq!(
            sources.slice(ast.statements().get(*tail).unwrap().span()),
            Ok("tail")
        );
    }
}

#[test]
fn unpaired_index_leaves_the_block_separator_and_respects_structural_line_breaks() {
    for (text, adjacent) in [
        ("{ xs[key β; next }", true),
        ("{ xs[key\nβ; next }", false),
        ("{ xs[key /*\n*/ β; next }", false),
    ] {
        let mut sources = SourceMap::new();
        let id = sources.add_source("index-recovery.ko", text).unwrap();
        let parsed = parse_block_twice(&sources, id, text);
        let ast = parsed.ast();
        let Statement::Block { elements } = ast.statements().get(parsed.root()).unwrap().payload()
        else {
            panic!("expected block");
        };
        let first = ast.statements().get(elements[0]).unwrap();
        let Statement::Expression { expression } = first.payload() else {
            panic!("expected index statement");
        };
        if adjacent {
            assert_eq!(elements.len(), 2, "{text}");
            assert_index(&sources, ast, *expression, "xs[key β", "key β", true);
            let [lexical, closing] = parsed.diagnostics() else {
                panic!("expected lexical and missing bracket diagnostics: {text}");
            };
            assert_eq!(lexical.code().to_string(), "L0001");
            assert_eq!(closing.code().to_string(), "L0010");
            assert_eq!(closing.primary_span().start(), text.find(';').unwrap());
            assert!(closing.primary_span().is_empty());
        } else {
            assert_eq!(elements.len(), 3, "{text}");
            assert_index(&sources, ast, *expression, "xs[key", "key", false);
            let second = ast.statements().get(elements[1]).unwrap();
            assert_eq!(second.payload(), &Statement::Error);
            assert_eq!(sources.slice(second.span()), Ok("β"));
            let [lexical] = parsed.diagnostics() else {
                panic!("expected the lexical diagnostic only: {text}");
            };
            assert_eq!(lexical.code().to_string(), "L0001");
        }
        let next = ast.statements().get(*elements.last().unwrap()).unwrap();
        assert_eq!(sources.slice(next.span()), Ok("next"));
    }
}

#[test]
fn an_unterminated_interpolation_keeps_its_terminal_lexer_diagnostics() {
    let mut sources = SourceMap::new();
    let text = r#""${xs[key β"#;
    let id = sources.add_source("index-recovery.ko", text).unwrap();
    let lexed = parser_test_assertions::lex_parser_source_twice(&sources, id, text);
    let parsed = parse_expression_twice(&sources, id, text);
    assert_eq!(parsed.diagnostics(), lexed.diagnostics());
}

#[test]
fn adjacent_poison_does_not_turn_a_caller_block_into_a_trailing_lambda() {
    for separator in [" ", "\n"] {
        let mut sources = SourceMap::new();
        let text = format!("{{ xs[key β{separator}{{ next }}; tail }}");
        let id = sources.add_source("index-recovery.ko", &text).unwrap();
        let parsed = parse_block_twice(&sources, id, &text);
        let ast = parsed.ast();
        let Statement::Block { elements } = ast.statements().get(parsed.root()).unwrap().payload()
        else {
            panic!("expected outer block");
        };
        let [first, next, tail] = elements.as_slice() else {
            panic!("expected a bad expression and two siblings: {text:?}");
        };
        assert_eq!(
            sources.slice(ast.statements().get(*first).unwrap().span()),
            Ok("xs[key β")
        );
        let next = ast.statements().get(*next).unwrap();
        assert!(matches!(next.payload(), Statement::Block { .. }));
        assert_eq!(sources.slice(next.span()), Ok("{ next }"));
        assert_eq!(
            sources.slice(ast.statements().get(*tail).unwrap().span()),
            Ok("tail")
        );
    }
}
