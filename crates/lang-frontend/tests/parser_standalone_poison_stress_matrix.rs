//! SPEC-0158 的四公开 Parser 入口 standalone lexical-poison 压力矩阵。

use lang_frontend::{
    ast::{ExpressionId, ItemId},
    diagnostic::Diagnostic,
    parser::{Expression, ExpressionAst, Item, Statement},
    source::{SourceId, SourceMap},
};

#[path = "support/parser_lexical_poisons.rs"]
mod parser_lexical_poisons;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_lexical_poisons::LEXICAL_POISONS;
use parser_test_assertions::{
    parse_block_twice, parse_declaration_twice, parse_expression_twice, parse_file_twice,
};

const POISON_COUNT: usize = 4_096;

fn add_source(source: String) -> (SourceMap, SourceId) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-standalone-poison-stress.ko", source)
        .expect("standalone poison stress source name must be unique");
    (sources, source_id)
}

fn repeated(separator: &str, poison: &str) -> String {
    (0..POISON_COUNT)
        .map(|_| poison)
        .collect::<Vec<_>>()
        .join(separator)
}

fn variables(poison: &str) -> String {
    (0..POISON_COUNT)
        .map(|index| format!("val x{index} = {poison}\n"))
        .collect()
}

fn assert_diagnostic_series(diagnostics: &[Diagnostic], code: &str, poison_len: usize) {
    assert_eq!(
        diagnostics.len(),
        POISON_COUNT,
        "unexpected diagnostics for {code}: {:?}",
        diagnostics
            .iter()
            .take(4)
            .map(|diagnostic| (
                diagnostic.code().to_string(),
                diagnostic.primary_span().start(),
                diagnostic.primary_span().end()
            ))
            .collect::<Vec<_>>()
    );
    assert!(diagnostics.iter().all(|diagnostic| {
        diagnostic.code().to_string() == code
            && diagnostic.primary_span().end() - diagnostic.primary_span().start() == poison_len
    }));
    assert!(
        diagnostics
            .windows(2)
            .all(|window| window[0].primary_span().start() < window[1].primary_span().start())
    );
}

fn assert_error_expression(ast: &ExpressionAst, expression: ExpressionId, poison_len: usize) {
    let node = ast
        .expressions()
        .get(expression)
        .expect("standalone poison expression");
    assert!(matches!(node.payload(), Expression::Error));
    assert_eq!(node.span().end() - node.span().start(), poison_len);
}

fn assert_error_expression_count(ast: &ExpressionAst) {
    assert_eq!(
        ast.expressions()
            .iter()
            .filter(|(_, node)| matches!(node.payload(), Expression::Error))
            .count(),
        POISON_COUNT
    );
}

fn assert_call_arguments(ast: &ExpressionAst, expression: ExpressionId, poison_len: usize) {
    let Expression::Call { arguments, .. } = ast
        .expressions()
        .get(expression)
        .expect("standalone poison call")
        .payload()
    else {
        panic!("standalone poison expression must remain a call")
    };
    assert_eq!(arguments.len(), POISON_COUNT);
    for argument in arguments {
        assert_eq!(argument.span.end() - argument.span.start(), poison_len);
        assert_error_expression(ast, argument.value, poison_len);
    }
    assert_error_expression_count(ast);
}

fn assert_variable_initializer(ast: &ExpressionAst, declaration: ItemId, poison_len: usize) {
    let Item::Variable { initializer, .. } = ast
        .items()
        .get(declaration)
        .expect("standalone poison variable")
        .payload()
    else {
        panic!("standalone poison declaration must remain a variable")
    };
    assert_error_expression(ast, *initializer, poison_len);
}

#[test]
fn every_public_entry_preserves_each_large_standalone_poison_stream() {
    assert_eq!(LEXICAL_POISONS.len(), 4);

    for poison in LEXICAL_POISONS {
        let arguments = repeated(",", poison.text);
        let context = format!("{} expression stress", poison.name);
        let (sources, source_id) = add_source(format!("call({arguments})"));
        let parsed = parse_expression_twice(&sources, source_id, &context);
        assert_diagnostic_series(parsed.diagnostics(), poison.code, poison.text.len());
        assert_call_arguments(parsed.ast(), parsed.root(), poison.text.len());

        let context = format!("{} declaration stress", poison.name);
        let (sources, source_id) = add_source(format!("val result = call({arguments})"));
        let parsed = parse_declaration_twice(&sources, source_id, &context);
        assert_diagnostic_series(parsed.diagnostics(), poison.code, poison.text.len());
        let Item::Variable { initializer, .. } = parsed
            .ast()
            .items()
            .get(parsed.root())
            .expect("standalone poison declaration root")
            .payload()
        else {
            panic!("standalone poison declaration must remain a variable")
        };
        assert_call_arguments(parsed.ast(), *initializer, poison.text.len());

        let context = format!("{} block stress", poison.name);
        let (sources, source_id) = add_source(format!("{{\n{}}}", variables(poison.text)));
        let parsed = parse_block_twice(&sources, source_id, &context);
        assert_diagnostic_series(parsed.diagnostics(), poison.code, poison.text.len());
        let Statement::Block { elements } = parsed
            .ast()
            .statements()
            .get(parsed.root())
            .expect("standalone poison block root")
            .payload()
        else {
            panic!("standalone poison block must retain its root")
        };
        assert_eq!(elements.len(), POISON_COUNT);
        for element in elements {
            let Statement::LocalVariable { declaration } = parsed
                .ast()
                .statements()
                .get(*element)
                .expect("standalone poison block element")
                .payload()
            else {
                panic!("standalone poison block element must remain a local variable")
            };
            assert_variable_initializer(parsed.ast(), *declaration, poison.text.len());
        }
        assert_error_expression_count(parsed.ast());

        let context = format!("{} file stress", poison.name);
        let (sources, source_id) = add_source(variables(poison.text));
        let parsed = parse_file_twice(&sources, source_id, &context);
        assert_diagnostic_series(parsed.diagnostics(), poison.code, poison.text.len());
        assert_eq!(parsed.roots().len(), POISON_COUNT);
        for root in parsed.roots() {
            assert_variable_initializer(parsed.ast(), *root, poison.text.len());
        }
        assert_error_expression_count(parsed.ast());
    }
}
