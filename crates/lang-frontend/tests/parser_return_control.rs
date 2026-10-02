//! SPEC-0241: a return operand begins before block-element soft stops apply.
use lang_frontend::{
    lexer::lex,
    parser::{Expression, ParsedFile, ParserInternalError, parse_file},
    source::SourceMap,
};

fn parse(text: &str) -> (SourceMap, ParsedFile) {
    let mut sources = SourceMap::new();
    let id = sources.add_source("return-control.ko", text).unwrap();
    let lexed = lex(&sources, id).unwrap();
    let parsed = parse_file(&sources, &lexed).unwrap();
    let repeated = parse_file(&sources, &lexed).unwrap();
    assert_eq!(parsed.diagnostics(), repeated.diagnostics());
    (sources, parsed)
}

fn returns(parsed: &ParsedFile) -> Vec<Option<&Expression>> {
    parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            let Expression::Return { value, .. } = node.payload() else {
                return None;
            };
            Some(value.map(|id| parsed.ast().expressions().get(id).unwrap().payload()))
        })
        .collect()
}

#[test]
fn return_if_is_one_operand_in_a_function_block() {
    let text = "fun choose(n: Int): Int { return if (n > 0) 1 else 0 }";
    let (sources, parsed) = parse(text);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert!(matches!(
        returns(&parsed)[..],
        [Some(Expression::If { .. })]
    ));
    let node = parsed
        .ast()
        .expressions()
        .iter()
        .find(|(_, node)| matches!(node.payload(), Expression::Return { .. }))
        .unwrap()
        .1;
    assert_eq!(
        sources.slice(node.span()).unwrap(),
        "return if (n > 0) 1 else 0"
    );
}

#[test]
fn return_when_is_one_operand_in_a_function_block() {
    let (_, parsed) = parse("fun choose(n: Int): Int { return when (n) { else -> 1 } }");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert!(matches!(
        returns(&parsed)[..],
        [Some(Expression::When { .. })]
    ));
}

#[test]
fn return_control_values_work_in_nested_blocks_and_lambdas() {
    let (_, parsed) = parse(
        "fun choose(): Unit { { return if (true) 1 else 0 }; val action = { return when { true -> 1; else -> 0 } } }",
    );
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert!(matches!(
        returns(&parsed)[..],
        [Some(Expression::If { .. }), Some(Expression::When { .. })]
    ));
}

#[test]
fn return_control_values_nest_without_stealing_outer_else() {
    let (_, parsed) = parse(
        "fun choose(): Int { if (true) return if (false) 1 else 2 else return when { true -> 3; else -> 4 } }",
    );
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert!(matches!(
        returns(&parsed)[..],
        [Some(Expression::If { .. }), Some(Expression::When { .. })]
    ));
}

#[test]
fn newline_and_comment_newline_leave_a_bare_return() {
    for separator in ["\n", "\r\n", "/*\n*/", "// comment\n"] {
        let (_, parsed) = parse(&format!(
            "fun choose(): Unit {{ return{separator}if (true) 1 else 0 }}"
        ));
        assert!(
            parsed.diagnostics().is_empty(),
            "{separator:?}: {:?}",
            parsed.diagnostics()
        );
        assert!(matches!(returns(&parsed)[..], [None]));
    }
}

#[test]
fn same_line_comment_preserves_return_control_operand() {
    let (_, parsed) =
        parse("fun choose(): Int { return /* same line */ when { true -> 1; else -> 0 } }");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert!(matches!(
        returns(&parsed)[..],
        [Some(Expression::When { .. })]
    ));
}

#[test]
fn separators_and_closers_preserve_bare_returns() {
    for text in [
        "fun choose(): Unit { return; if (true) 1 else 0 }",
        "fun choose(): Unit { return }",
        "fun choose(): Unit = return",
    ] {
        let (_, parsed) = parse(text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{text}: {:?}",
            parsed.diagnostics()
        );
        assert!(matches!(returns(&parsed)[..], [None]));
    }
}

#[test]
fn caller_delimiters_and_else_stay_with_their_owner() {
    for text in [
        "fun choose(): Unit { use(return, 1) }",
        "fun choose(): Unit { use(return) }",
        "fun choose(): Unit { if (true) return else return }",
    ] {
        let (_, parsed) = parse(text);
        assert!(
            parsed.diagnostics().is_empty(),
            "{text}: {:?}",
            parsed.diagnostics()
        );
        assert!(returns(&parsed).iter().all(Option::is_none));
    }
}

#[test]
fn returned_if_without_else_remains_a_value_context_error() {
    let (sources, parsed) = parse("fun choose(): Int { return if (true) 1 }");
    assert_eq!(parsed.diagnostics().len(), 1, "{:?}", parsed.diagnostics());
    assert_eq!(parsed.diagnostics()[0].code().to_string(), "L0057");
    assert_eq!(
        sources
            .slice(parsed.diagnostics()[0].primary_span())
            .unwrap(),
        ""
    );
    assert!(matches!(
        returns(&parsed)[..],
        [Some(Expression::If { .. })]
    ));
}

#[test]
fn completed_return_value_does_not_swallow_the_next_control_element() {
    let (_, parsed) =
        parse("fun choose(): Int { return if (true) 1 else 0\nwhen { true -> 2; else -> 3 } }");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert!(matches!(
        returns(&parsed)[..],
        [Some(Expression::If { .. })]
    ));
    let whens = parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), Expression::When { .. }))
        .count();
    assert_eq!(whens, 1);
}

#[test]
fn same_line_nested_return_operands_preserve_the_existing_recursion_limit() {
    let (_, parsed) = parse("fun choose(): Int { return return return 1 }");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let operands = returns(&parsed);
    assert_eq!(operands.len(), 3);
    assert_eq!(
        operands
            .iter()
            .filter(|value| matches!(value, Some(Expression::Return { .. })))
            .count(),
        2
    );

    for text in [
        format!("fun choose(): Int {{ {}1 }}", "return ".repeat(1_024)),
        format!(
            "fun choose(): Int {{ {}1{} }}",
            "return if (true) ".repeat(1_024),
            " else 0".repeat(1_024)
        ),
    ] {
        let mut sources = SourceMap::new();
        let id = sources.add_source("return-limit.ko", text).unwrap();
        let lexed = lex(&sources, id).unwrap();
        for _ in 0..2 {
            assert!(matches!(
                parse_file(&sources, &lexed),
                Err(ParserInternalError::NestingLimitExceeded { limit: 1_024 })
            ));
        }
    }
}
