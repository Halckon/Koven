//! The lexer owns poisoned super-name regions; parser recovery must preserve the caller.

use lang_frontend::{
    lexer::lex,
    parser::{Expression, Statement, StringPart, parse_block, parse_expression, parse_file},
    source::SourceMap,
};

#[test]
fn super_interface_separator_and_member_poison_keep_one_lexical_root_cause() {
    for text in [
        "super<I>.async",
        "super<I>.#",
        "super<I> async",
        "super<I> #",
        "super async",
        "super #",
    ] {
        let mut sources = SourceMap::new();
        let source = sources.add_source("main.ko", text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        assert_eq!(lexed.diagnostics().len(), 1, "{text}");
        for _ in 0..2 {
            let parsed = parse_expression(&sources, &lexed).unwrap();
            assert_eq!(
                parsed.diagnostics().len(),
                1,
                "{text}: {:?}",
                parsed.diagnostics()
            );
            let expected = &lexed.diagnostics()[0];
            let actual = &parsed.diagnostics()[0];
            assert_eq!(actual.code(), expected.code(), "{text}");
            assert_eq!(actual.primary_span(), expected.primary_span(), "{text}");
            assert!(matches!(
                parsed
                    .ast()
                    .expressions()
                    .get(parsed.root())
                    .unwrap()
                    .payload(),
                Expression::Error
            ));
        }
    }
}

#[test]
fn super_poison_does_not_consume_the_next_call_argument() {
    for operand in [
        "super async",
        "super<I> async",
        "super<I>.async",
        "super<I>.async()",
        "super<I>.async.",
        "a.async.",
    ] {
        let text = format!("f({operand}, sentinel)");
        let mut sources = SourceMap::new();
        let source = sources.add_source("main.ko", &text).unwrap();
        let parsed = parse_expression(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert_eq!(
            parsed.diagnostics().len(),
            1,
            "{text}: {:?}",
            parsed.diagnostics()
        );
        assert_eq!(parsed.diagnostics()[0].code().to_string(), "L0002");
        let Expression::Call { arguments, .. } = parsed
            .ast()
            .expressions()
            .get(parsed.root())
            .unwrap()
            .payload()
        else {
            panic!("{text}: outer call lost")
        };
        assert_eq!(arguments.len(), 2, "{text}");
        assert!(matches!(
            parsed
                .ast()
                .expressions()
                .get(arguments[0].value)
                .unwrap()
                .payload(),
            Expression::Error
        ));
        let sentinel = parsed.ast().expressions().get(arguments[1].value).unwrap();
        assert!(matches!(sentinel.payload(), Expression::Name));
        assert_eq!(sources.slice(sentinel.span()).unwrap(), "sentinel");
    }
}

#[test]
fn poisoned_postfix_keeps_the_explicit_block_separator_and_next_statement() {
    for operand in [
        "a.async.",
        "super<I>.async.",
        "a.async[x",
        "super<I>.async(x",
        "a.async[{ -> 1 }",
        "a.async[g<T, U>{ 1 }",
    ] {
        for gap in [" ", "\n"] {
            let text = format!("{{ {operand};{gap}sentinel }}");
            let mut sources = SourceMap::new();
            let source = sources.add_source("main.ko", &text).unwrap();
            let parsed = parse_block(&sources, &lex(&sources, source).unwrap()).unwrap();
            assert_eq!(
                parsed.diagnostics().len(),
                1,
                "{text}: {:?}",
                parsed.diagnostics()
            );
            assert_eq!(parsed.diagnostics()[0].code().to_string(), "L0002");
            let Statement::Block { elements } = parsed
                .ast()
                .statements()
                .get(parsed.root())
                .unwrap()
                .payload()
            else {
                panic!("block root")
            };
            assert_eq!(elements.len(), 2, "{text}");
            let first = parsed.ast().statements().get(elements[0]).unwrap();
            assert_eq!(sources.slice(first.span()), Ok(operand), "{text}");
            let Statement::Expression { expression } = first.payload() else {
                panic!("{text}: expected first expression statement");
            };
            assert_eq!(
                parsed
                    .ast()
                    .expressions()
                    .get(*expression)
                    .unwrap()
                    .payload(),
                &Expression::Error,
                "{text}"
            );

            let Statement::Expression { expression } = parsed
                .ast()
                .statements()
                .get(elements[1])
                .unwrap()
                .payload()
            else {
                panic!("sentinel statement")
            };
            let sentinel = parsed.ast().expressions().get(*expression).unwrap();
            assert!(matches!(sentinel.payload(), Expression::Name));
            assert_eq!(sources.slice(sentinel.span()).unwrap(), "sentinel");
        }
    }
}

#[test]
fn error_postfix_index_keeps_the_callers_comma() {
    for (text, suffix) in [
        ("f(a.async[x, next)", "a.async[x"),
        ("f(a.async[nested(x, y), next)", "a.async[nested(x, y)"),
        ("f(a.async[g<T, U>(x), next)", "a.async[g<T, U>(x)"),
        ("f(a.async[g!!<T, U>(x), next)", "a.async[g!!<T, U>(x)"),
        (
            "f(a.async[g<T, U>{ -> 1 }, next)",
            "a.async[g<T, U>{ -> 1 }",
        ),
        ("f(a.async[x < a, next)", "a.async[x < a"),
        (
            r#"f(a.async["${nested[item]}", stray], next)"#,
            r#"a.async["${nested[item]}", stray]"#,
        ),
    ] {
        let mut sources = SourceMap::new();
        let id = sources.add_source("error-postfix.ko", text).unwrap();
        let parsed = parse_expression(&sources, &lex(&sources, id).unwrap()).unwrap();
        let root = parsed.ast().expressions().get(parsed.root()).unwrap();
        let Expression::Call { arguments, .. } = root.payload() else {
            panic!("{text}: expected outer call");
        };
        let [first, next] = arguments.as_slice() else {
            panic!("{text}: expected two call arguments, got {arguments:?}");
        };
        assert_eq!(sources.slice(first.span), Ok(suffix), "{text}");
        assert_eq!(sources.slice(next.span), Ok("next"), "{text}");
        assert_eq!(
            parsed.diagnostics().len(),
            1,
            "{text}: {:?}",
            parsed.diagnostics()
        );
        assert_eq!(parsed.diagnostics()[0].code().to_string(), "L0002");
    }
}

#[test]
fn an_unclosed_error_postfix_cannot_hide_the_next_declaration() {
    for (text, initializer) in [
        (
            "val x = a.async[nested(\nval next = 1",
            "val x = a.async[nested(",
        ),
        (
            "val x = a.async[nested(; val next = 1",
            "val x = a.async[nested(",
        ),
        (
            "val x = a.async(nested[\nval next = 1",
            "val x = a.async(nested[",
        ),
    ] {
        let mut sources = SourceMap::new();
        let id = sources.add_source("error-postfix.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let roots = parsed
            .roots()
            .iter()
            .map(|item| {
                sources
                    .slice(parsed.ast().items().get(*item).unwrap().span())
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(roots, [initializer, "val next = 1"]);
        assert_eq!(parsed.diagnostics().len(), 1);
        assert_eq!(parsed.diagnostics()[0].code().to_string(), "L0002");
    }
}

#[test]
fn unclosed_error_postfix_keeps_the_interpolation_closer_and_tail() {
    let mut sources = SourceMap::new();
    let text = r#""${a.async[nested(}tail""#;
    let id = sources.add_source("error-postfix.ko", text).unwrap();
    let parsed = parse_expression(&sources, &lex(&sources, id).unwrap()).unwrap();
    let root = parsed.ast().expressions().get(parsed.root()).unwrap();
    let Expression::String { parts } = root.payload() else {
        panic!("expected string");
    };
    let [
        StringPart::Interpolation { expression, .. },
        StringPart::Text(tail),
    ] = parts.as_slice()
    else {
        panic!("expected interpolation and tail: {parts:?}");
    };
    let expression = parsed.ast().expressions().get(*expression).unwrap();
    assert_eq!(expression.payload(), &Expression::Error);
    assert_eq!(sources.slice(expression.span()), Ok("a.async[nested("));
    assert_eq!(sources.slice(*tail), Ok("tail"));
    assert_eq!(parsed.diagnostics().len(), 1, "{:?}", parsed.diagnostics());
    assert_eq!(parsed.diagnostics()[0].code().to_string(), "L0002");
}

#[test]
fn unpaired_error_postfix_preserves_a_caller_block_without_lambda_header() {
    let mut sources = SourceMap::new();
    let text = "{ a.async[x\n{ nested }; next }";
    let id = sources.add_source("error-postfix.ko", text).unwrap();
    let parsed = parse_block(&sources, &lex(&sources, id).unwrap()).unwrap();
    let ast = parsed.ast();
    let Statement::Block { elements } = ast.statements().get(parsed.root()).unwrap().payload()
    else {
        panic!("expected block");
    };
    let [first, nested, next] = elements.as_slice() else {
        panic!("expected three statements, got {elements:?}");
    };
    assert_eq!(
        sources.slice(ast.statements().get(*first).unwrap().span()),
        Ok("a.async[x")
    );
    let nested = ast.statements().get(*nested).unwrap();
    assert!(matches!(nested.payload(), Statement::Block { .. }));
    assert_eq!(sources.slice(nested.span()), Ok("{ nested }"));
    assert_eq!(
        sources.slice(ast.statements().get(*next).unwrap().span()),
        Ok("next")
    );
    assert_eq!(parsed.diagnostics().len(), 1, "{:?}", parsed.diagnostics());
    assert_eq!(parsed.diagnostics()[0].code().to_string(), "L0002");
}

#[test]
fn only_a_local_contiguous_typed_call_can_hide_a_recovery_comma() {
    for (text, first, middle) in [
        (
            "f(a.async[g<T, U>\n{ -> 1 }, next)",
            "a.async[g<T",
            "U>\n{ -> 1 }",
        ),
        ("f(a.async[<T, U>(x), next)", "a.async[<T", "U>(x)"),
        ("f(a.async[? <T, U>(x), next)", "a.async[? <T", "U>(x)"),
    ] {
        let mut sources = SourceMap::new();
        let id = sources.add_source("error-postfix.ko", text).unwrap();
        let parsed = parse_expression(&sources, &lex(&sources, id).unwrap()).unwrap();
        let root = parsed.ast().expressions().get(parsed.root()).unwrap();
        let Expression::Call { arguments, .. } = root.payload() else {
            panic!("{text}: expected outer call");
        };
        let spans = arguments
            .iter()
            .map(|arg| sources.slice(arg.span).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(spans, [first, middle, "next"], "{text}");
        assert_eq!(
            parsed.diagnostics().len(),
            1,
            "{text}: {:?}",
            parsed.diagnostics()
        );
        assert_eq!(parsed.diagnostics()[0].code().to_string(), "L0002");
    }
}
