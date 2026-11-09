//! An index owns its closing bracket, including when its key is malformed.

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use lang_frontend::{
    ast::ExpressionId,
    parser::{BinaryOperator, Expression, Item, StringPart, SyntaxAst},
    source::SourceMap,
};
use parser_test_assertions::{parse_expression_twice, parse_file_twice};

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
