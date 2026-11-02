//! Parser-produced operands exercise the common logical identity, not type validity.
use super::*;
use crate::{lexer::lex, parser::parse_file, source::SourceMap};

fn operand(text: &str) -> (SourceMap, crate::parser::ParsedFile, ExpressionId) {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("index.ko", format!("fun index(): Int = {text}"))
        .unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(
        lexed.diagnostics().is_empty(),
        "{text}: {:?}",
        lexed.diagnostics()
    );
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{text}: {:?}",
        parsed.diagnostics()
    );
    let expressions = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| (sources.slice(node.span()).unwrap() == text).then_some(id))
        .collect::<Vec<_>>();
    let [expression] = expressions.as_slice() else {
        panic!("unique original operand: {text}")
    };
    let expression = *expression;
    (sources, parsed, expression)
}

#[test]
fn parsed_index_spelling_groups_and_signs_preserve_logical_identity() {
    for (text, expected) in [
        ("42", 42),
        ("000_042", 42),
        ("0x2a", 42),
        ("0B101010", 42),
        ("0x2AL", 42),
        ("0B101010U", 42),
        ("42uL", 42),
        ("((42))", 42),
        ("+(0x2a)", 42),
        ("-(+42)", -42),
        ("-(-42)", 42),
        ("-0", 0),
        ("170141183460469231731687303715884105727", i128::MAX),
        ("-170141183460469231731687303715884105727", -i128::MAX),
    ] {
        let (sources, parsed, expression) = operand(text);
        assert_eq!(
            element_index_identity(&sources, &parsed, expression).unwrap(),
            ElementIndexIdentity::Known(expected),
            "{text}"
        );
    }
}

#[test]
fn dynamic_or_unrepresentable_index_is_unknown_without_constant_folding() {
    for text in [
        "index",
        "(index)",
        "+index",
        "-index",
        "1 + 1",
        "read()",
        "true",
        "170141183460469231731687303715884105728",
        "-170141183460469231731687303715884105728",
        "340282366920938463463374607431768211455",
        "340282366920938463463374607431768211456",
    ] {
        let (sources, parsed, expression) = operand(text);
        assert_eq!(
            element_index_identity(&sources, &parsed, expression).unwrap(),
            ElementIndexIdentity::Unknown,
            "{text}"
        );
    }
}

#[test]
fn index_lookup_preserves_source_and_ast_errors() {
    let (_, parsed, expression) = operand("42");
    let foreign_sources = SourceMap::new();
    assert!(matches!(
        element_index_identity(&foreign_sources, &parsed, expression),
        Err(OwnershipCheckingError::Source(_))
    ));
    let (sources, short, _) = operand("0");
    let (_, larger, _) = operand("((1 + 2) + (3 + 4))");
    let outside = larger
        .ast()
        .expressions()
        .iter()
        .map(|(id, _)| id)
        .find(|id| short.ast().expressions().get(*id).is_err())
        .unwrap();
    assert!(matches!(
        element_index_identity(&sources, &short, outside),
        Err(OwnershipCheckingError::Ast(_))
    ));
}
