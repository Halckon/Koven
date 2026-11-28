use super::expression_kernel::{
    GroupNonPoisonTailAction, WhenConditionDispatch, group_non_poison_tail_action,
    is_group_declaration_tail_kind, is_when_body_line_break_infix_stop_kind,
    prefix_operator_for_kind, return_parses_value, string_missing_closer_emits_diagnostic,
    when_condition_dispatch,
};
use super::{Expression, PrefixOperator, Statement, WhenCondition, parse, parse_block};
use crate::lexer::{Keyword, LexemeKind, Symbol, TokenKind, lex_test_source_twice};
use crate::source::SourceMap;

#[test]
fn prefix_operator_and_when_condition_dispatch_match_spec() {
    assert_eq!(
        prefix_operator_for_kind(LexemeKind::Token(TokenKind::Symbol(Symbol::Bang))),
        Some(PrefixOperator::Not)
    );
    assert_eq!(
        prefix_operator_for_kind(LexemeKind::Token(TokenKind::Symbol(Symbol::Plus))),
        Some(PrefixOperator::Plus)
    );
    assert_eq!(
        prefix_operator_for_kind(LexemeKind::Token(TokenKind::Symbol(Symbol::Minus))),
        Some(PrefixOperator::Minus)
    );
    assert_eq!(
        prefix_operator_for_kind(LexemeKind::Token(TokenKind::Symbol(Symbol::Star))),
        None
    );

    for (kind, expected) in [
        (
            LexemeKind::Token(TokenKind::Keyword(Keyword::Is)),
            WhenConditionDispatch::TypeTest { negated: false },
        ),
        (
            LexemeKind::Token(TokenKind::Symbol(Symbol::BangIs)),
            WhenConditionDispatch::TypeTest { negated: true },
        ),
        (
            LexemeKind::Token(TokenKind::Keyword(Keyword::In)),
            WhenConditionDispatch::Contains { negated: false },
        ),
        (
            LexemeKind::Token(TokenKind::Symbol(Symbol::BangIn)),
            WhenConditionDispatch::Contains { negated: true },
        ),
    ] {
        assert!(is_when_body_line_break_infix_stop_kind(kind));
        assert_eq!(when_condition_dispatch(kind, false), expected);
        assert_eq!(when_condition_dispatch(kind, true), expected);
    }

    let ident = LexemeKind::Token(TokenKind::Identifier);
    assert!(!is_when_body_line_break_infix_stop_kind(ident));
    assert_eq!(
        when_condition_dispatch(ident, true),
        WhenConditionDispatch::Expression
    );
    assert_eq!(
        when_condition_dispatch(ident, false),
        WhenConditionDispatch::ExpectedWhenEntry
    );
}

#[test]
fn return_value_and_string_missing_closer_guards_cover_all_flags() {
    for can_start in [false, true] {
        for has_line_break in [false, true] {
            for in_stops in [false, true] {
                for is_control in [false, true] {
                    assert_eq!(
                        return_parses_value(can_start, has_line_break, in_stops, is_control),
                        can_start && !has_line_break && (!in_stops || is_control)
                    );
                }
            }
        }
    }

    for lexer_owns in [false, true] {
        for at_end in [false, true] {
            for terminal_eof in [false, true] {
                assert_eq!(
                    string_missing_closer_emits_diagnostic(lexer_owns, at_end, terminal_eof),
                    !(lexer_owns || at_end && terminal_eof)
                );
            }
        }
    }
}

#[test]
fn group_non_poison_tail_action_respects_priority_order() {
    assert!(is_group_declaration_tail_kind(
        LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)),
        false
    ));
    assert!(!is_group_declaration_tail_kind(
        LexemeKind::Token(TokenKind::Keyword(Keyword::Fun)),
        false
    ));
    assert!(is_group_declaration_tail_kind(
        LexemeKind::Token(TokenKind::Keyword(Keyword::Fun)),
        true
    ));

    assert_eq!(
        group_non_poison_tail_action(true, true, true, false),
        GroupNonPoisonTailAction::AtRightParen
    );
    assert_eq!(
        group_non_poison_tail_action(false, true, true, false),
        GroupNonPoisonTailAction::RecoverDeclarationTail
    );
    assert_eq!(
        group_non_poison_tail_action(false, false, true, true),
        GroupNonPoisonTailAction::TryConfirmedCloser
    );
    assert_eq!(
        group_non_poison_tail_action(false, false, false, false),
        GroupNonPoisonTailAction::TryConfirmedCloser
    );
    assert_eq!(
        group_non_poison_tail_action(false, false, false, true),
        GroupNonPoisonTailAction::NoSkip
    );
}

#[test]
fn expression_production_entries_preserve_when_conditions_and_return_line_break() {
    let when_src = "when (x) {\n    is Int -> 1\n    !is String -> 2\n    in a -> 3\n    !in b -> 4\n    else -> 5\n}";
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("when-conds.ko", when_src)
        .expect("unique source");
    let lexed = lex_test_source_twice(&sources, id, "when condition dispatch");
    let parsed = parse(&sources, &lexed).expect("when must parse");
    assert!(
        parsed.diagnostics().is_empty(),
        "unexpected diagnostics: {:?}",
        parsed.diagnostics()
    );
    let Expression::When { entries, .. } = parsed
        .ast()
        .expressions()
        .get(parsed.root())
        .expect("when root")
        .payload()
    else {
        panic!("expected Expression::When");
    };
    assert_eq!(entries.len(), 5);
    assert!(matches!(
        entries[0].conditions.as_slice(),
        [WhenCondition::TypeTest { negated: false, .. }]
    ));
    assert!(matches!(
        entries[1].conditions.as_slice(),
        [WhenCondition::TypeTest { negated: true, .. }]
    ));
    assert!(matches!(
        entries[2].conditions.as_slice(),
        [WhenCondition::Contains { negated: false, .. }]
    ));
    assert!(matches!(
        entries[3].conditions.as_slice(),
        [WhenCondition::Contains { negated: true, .. }]
    ));
    assert!(entries[4].else_span.is_some());

    // return followed by newline stops without consuming next-line expression
    let ret_src = "{\n    return\n    x\n}";
    let mut ret_sources = SourceMap::new();
    let ret_id = ret_sources
        .add_source("return-lb.ko", ret_src)
        .expect("unique source");
    let ret_lexed = lex_test_source_twice(&ret_sources, ret_id, "return line break");
    let ret_parsed = parse_block(&ret_sources, &ret_lexed).expect("block must parse");
    assert!(
        ret_parsed.diagnostics().is_empty(),
        "unexpected diagnostics: {:?}",
        ret_parsed.diagnostics()
    );
    let Statement::Block { elements } = ret_parsed
        .ast()
        .statements()
        .get(ret_parsed.root())
        .expect("block root")
        .payload()
    else {
        panic!("expected Statement::Block");
    };
    assert_eq!(elements.len(), 2);
    let Statement::Expression { expression } = ret_parsed
        .ast()
        .statements()
        .get(elements[0])
        .expect("first stmt")
        .payload()
    else {
        panic!("expected return stmt");
    };
    assert!(matches!(
        ret_parsed
            .ast()
            .expressions()
            .get(*expression)
            .expect("return expr")
            .payload(),
        Expression::Return { value: None, .. }
    ));
}
