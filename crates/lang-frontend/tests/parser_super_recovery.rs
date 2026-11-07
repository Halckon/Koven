//! The lexer owns poisoned super-name regions; parser recovery must preserve the caller.

use lang_frontend::{
    lexer::lex,
    parser::{Expression, Statement, parse_block, parse_expression},
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
    for operand in ["a.async.", "super<I>.async."] {
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
