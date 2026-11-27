use super::postfix::{
    call_argument_boundary_decision, has_recovery_callee_end_kinds, is_postfix_start_kind,
    is_recovery_callee_ignored_kind, is_recovery_callee_terminal_kind,
    trailing_lambda_attaches_after_call, typed_call_suffix_continues_kind,
};
use crate::lexer::{Keyword, LexemeKind, Symbol, TokenKind, TriviaKind};

#[test]
fn postfix_start_kind_matches_exact_seven_symbols() {
    for symbol in [
        Symbol::Dot,
        Symbol::QuestionDot,
        Symbol::LeftParen,
        Symbol::LeftBracket,
        Symbol::BangBang,
        Symbol::Question,
        Symbol::ColonColon,
    ] {
        assert!(is_postfix_start_kind(LexemeKind::Token(TokenKind::Symbol(
            symbol
        ))));
    }
    assert!(!is_postfix_start_kind(LexemeKind::Token(
        TokenKind::Symbol(Symbol::LeftBrace)
    )));
    assert!(!is_postfix_start_kind(LexemeKind::Token(
        TokenKind::Symbol(Symbol::Less)
    )));
}

#[test]
fn typed_call_suffix_stops_only_on_line_broken_left_brace() {
    let lbrace = LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace));
    let lparen = LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen));
    assert!(typed_call_suffix_continues_kind(lbrace, false));
    assert!(!typed_call_suffix_continues_kind(lbrace, true));
    assert!(typed_call_suffix_continues_kind(lparen, true));
}

#[test]
fn trailing_lambda_attaches_only_after_closed_call_without_when_entry() {
    let lbrace = LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace));
    let lparen = LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen));
    assert!(trailing_lambda_attaches_after_call(true, lbrace, false));
    assert!(!trailing_lambda_attaches_after_call(false, lbrace, false));
    assert!(!trailing_lambda_attaches_after_call(true, lbrace, true));
    assert!(!trailing_lambda_attaches_after_call(true, lparen, false));
}

#[test]
fn call_argument_boundary_matches_delimiters_outer_stops_and_block_separator() {
    let comma = LexemeKind::Token(TokenKind::Symbol(Symbol::Comma));
    let rparen = LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen));
    let ident = LexemeKind::Token(TokenKind::Identifier);
    assert!(call_argument_boundary_decision(
        LexemeKind::Eof,
        false,
        true,
        false
    ));
    assert!(call_argument_boundary_decision(comma, false, true, false));
    assert!(call_argument_boundary_decision(rparen, false, true, false));
    assert!(call_argument_boundary_decision(ident, true, false, false));
    assert!(!call_argument_boundary_decision(ident, true, true, false));
    assert!(call_argument_boundary_decision(ident, false, true, true));
}

#[test]
fn recovery_callee_end_skips_trailing_trivia_and_postfix_markers() {
    let ident = LexemeKind::Token(TokenKind::Identifier);
    let trivia = LexemeKind::Trivia(TriviaKind::Whitespace);
    let question = LexemeKind::Token(TokenKind::Symbol(Symbol::Question));
    let bang_bang = LexemeKind::Token(TokenKind::Symbol(Symbol::BangBang));
    let kw_if = LexemeKind::Token(TokenKind::Keyword(Keyword::If));

    assert!(is_recovery_callee_ignored_kind(question));
    assert!(is_recovery_callee_terminal_kind(ident));
    assert!(has_recovery_callee_end_kinds(&[
        kw_if, ident, question, trivia, bang_bang
    ]));
    assert!(!has_recovery_callee_end_kinds(&[
        ident, kw_if, question, trivia
    ]));
    assert!(!has_recovery_callee_end_kinds(&[trivia, question]));
    assert!(!has_recovery_callee_end_kinds(&[]));
}
