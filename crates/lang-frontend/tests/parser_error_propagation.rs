//! SPEC-0063 postfix `?` 的 Phase 1 AST、消歧与边界测试。

use lang_frontend::{
    ast::ExpressionId,
    diagnostic::Diagnostic,
    lexer::lex,
    parser::{Expression, ParsedExpression, Statement, parse_block, parse_expression, parse_file},
    source::SourceMap,
};

fn parse(text: &str) -> (SourceMap, ParsedExpression) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("propagation.ko", text)
        .expect("unique source");
    let lexed = lex(&sources, source_id).expect("lex");
    let parsed = parse_expression(&sources, &lexed).expect("parse");
    (sources, parsed)
}

fn payload(parsed: &ParsedExpression, id: ExpressionId) -> &Expression {
    parsed
        .ast()
        .expressions()
        .get(id)
        .expect("expression")
        .payload()
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

#[test]
fn propagate_joins_the_existing_left_associative_postfix_chain() {
    let (_, parsed) = parse("(load()?).field?!!");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let root = parsed.ast().expressions().get(parsed.root()).unwrap();
    assert_eq!((root.span().start(), root.span().end()), (0, 18));

    let Expression::NonNullAssert { operand, .. } = root.payload() else {
        panic!("outer non-null assertion")
    };
    let Expression::Propagate {
        value,
        question_span,
    } = payload(&parsed, *operand)
    else {
        panic!("member propagation")
    };
    assert_eq!((question_span.start(), question_span.end()), (15, 16));
    let Expression::Member {
        receiver,
        safe,
        name_span,
        ..
    } = payload(&parsed, *value)
    else {
        panic!("member")
    };
    assert!(!safe);
    assert_eq!((name_span.start(), name_span.end()), (10, 15));
    let Expression::Group { expression } = payload(&parsed, *receiver) else {
        panic!("grouped propagation")
    };
    let Expression::Propagate {
        value,
        question_span,
    } = payload(&parsed, *expression)
    else {
        panic!("call propagation")
    };
    assert_eq!((question_span.start(), question_span.end()), (7, 8));
    assert!(matches!(payload(&parsed, *value), Expression::Call { .. }));
}

#[test]
fn question_safe_call_elvis_and_nullable_type_remain_distinct() {
    for text in [
        "result?",
        "result?.member",
        "result ?: fallback",
        "item as Result<T, E>?",
        "load()?.member?",
        "(load()?).member",
        "array[index]?",
        "(source?)[index]",
    ] {
        let (_, parsed) = parse(text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{text:?}: {:?}",
            parsed.diagnostics()
        );
    }

    let (_, propagated) = parse("result?");
    assert!(matches!(
        payload(&propagated, propagated.root()),
        Expression::Propagate { .. }
    ));
    let (_, safe) = parse("result?.member");
    assert!(matches!(
        payload(&safe, safe.root()),
        Expression::Member { safe: true, .. }
    ));
    let (_, elvis) = parse("result ?: fallback");
    assert!(matches!(
        payload(&elvis, elvis.root()),
        Expression::Binary { .. }
    ));
}

#[test]
fn consecutive_questions_create_one_node_per_real_token() {
    let (_, parsed) = parse("result??");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let Expression::Propagate {
        value,
        question_span,
    } = payload(&parsed, parsed.root())
    else {
        panic!("outer propagation")
    };
    assert_eq!((question_span.start(), question_span.end()), (7, 8));
    let Expression::Propagate { question_span, .. } = payload(&parsed, *value) else {
        panic!("inner propagation")
    };
    assert_eq!((question_span.start(), question_span.end()), (6, 7));
}

#[test]
fn phase_one_accepts_propagation_in_lambda_and_named_function_bodies() {
    let (_, lambda) = parse("{ input -> decode(input)? }");
    assert!(
        lambda.diagnostics().is_empty(),
        "{:?}",
        lambda.diagnostics()
    );

    let text = "fun decode(input: Input): Result<Output, DecodeError> { return parse(input)? }";
    let mut sources = SourceMap::new();
    let source_id = sources.add_source("file.ko", text).expect("unique source");
    let lexed = lex(&sources, source_id).expect("lex");
    let parsed = parse_file(&sources, &lexed).expect("parse file");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );

    let block_text = "{ val output = parse(input)?\n val next = output }";
    let mut block_sources = SourceMap::new();
    let block_id = block_sources
        .add_source("block.ko", block_text)
        .expect("unique source");
    let block_lexed = lex(&block_sources, block_id).expect("lex");
    let block = parse_block(&block_sources, &block_lexed).expect("parse block");
    assert!(block.diagnostics().is_empty(), "{:?}", block.diagnostics());
}

#[test]
fn a_question_without_a_left_operand_uses_the_existing_expression_error() {
    let (_, parsed) = parse("?");
    assert_eq!(codes(parsed.diagnostics()), ["L0009"]);
    let span = parsed.diagnostics()[0].primary_span();
    assert_eq!((span.start(), span.end()), (0, 1));
    assert!(matches!(payload(&parsed, parsed.root()), Expression::Error));
}

#[test]
fn a_missing_operand_preserves_the_next_block_element() {
    let text = "{ val failed = ?\n val next = 1 }";
    let mut sources = SourceMap::new();
    let source_id = sources.add_source("recovery.ko", text).unwrap();
    let lexed = lex(&sources, source_id).expect("lex");
    let parsed = parse_block(&sources, &lexed).expect("parse block");
    assert_eq!(codes(parsed.diagnostics()), ["L0009"]);
    let Statement::Block { elements } = parsed
        .ast()
        .statements()
        .get(parsed.root())
        .expect("root block")
        .payload()
    else {
        panic!("block")
    };
    assert_eq!(elements.len(), 2);
}

#[test]
fn propagation_preserves_source_identity_and_exact_composite_span() {
    let mut sources = SourceMap::new();
    let _first = sources.add_source("first.ko", "other?").unwrap();
    let second = sources.add_source("second.ko", "result?").unwrap();
    let lexed = lex(&sources, second).expect("lex");
    let parsed = parse_expression(&sources, &lexed).expect("parse");
    let root = parsed.ast().expressions().get(parsed.root()).unwrap();
    assert_eq!(root.span().source_id(), second);
    assert_eq!((root.span().start(), root.span().end()), (0, 7));
    let Expression::Propagate { question_span, .. } = root.payload() else {
        panic!("propagation")
    };
    assert_eq!(question_span.source_id(), second);
}

#[test]
fn long_postfix_propagation_chains_are_iterative_and_linear_in_ast_size() {
    fn node_count(questions: usize) -> usize {
        let text = format!("result{}", "?".repeat(questions));
        let (_, parsed) = parse(&text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
        parsed.ast().expressions().len()
    }

    let small = node_count(512);
    let large = node_count(1024);
    assert_eq!(small, 513);
    assert_eq!(large, 1025);
    assert_eq!(large - small, 512);
}
