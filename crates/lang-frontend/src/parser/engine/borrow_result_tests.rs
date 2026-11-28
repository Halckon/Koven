use super::borrow_result::{
    ExplicitResultAction, borrow_local_contract_is_invalid, explicit_result_action,
    is_borrow_local_start_tail,
};
use super::declaration_head::{is_extension_receiver_follow_kind, is_extension_receiver_head_kind};
use crate::lexer::{Keyword, LexemeKind, Symbol, TokenKind};

#[test]
fn explicit_result_action_and_borrow_local_contract_cover_all_combinations() {
    assert_eq!(
        explicit_result_action(false, false),
        ExplicitResultAction::PlainTypeOnly
    );
    assert_eq!(
        explicit_result_action(true, false),
        ExplicitResultAction::MissingBorrowFrom
    );
    assert_eq!(
        explicit_result_action(true, true),
        ExplicitResultAction::ParseBorrowSource
    );
    assert_eq!(
        explicit_result_action(false, true),
        ExplicitResultAction::ParseCarrierSource
    );

    for is_borrow in [false, true] {
        for immutable in [false, true] {
            for conditional in [false, true] {
                let valid = is_borrow && immutable && !conditional;
                assert_eq!(
                    borrow_local_contract_is_invalid(is_borrow, immutable, conditional),
                    !valid
                );
            }
        }
    }
}

#[test]
fn borrow_local_start_tail_matches_direct_and_conditional_forms() {
    let val = LexemeKind::Token(TokenKind::Keyword(Keyword::Val));
    let var = LexemeKind::Token(TokenKind::Keyword(Keyword::Var));
    let question = LexemeKind::Token(TokenKind::Symbol(Symbol::Question));
    let ident = LexemeKind::Token(TokenKind::Identifier);

    assert!(is_borrow_local_start_tail(val, None));
    assert!(is_borrow_local_start_tail(var, None));
    assert!(is_borrow_local_start_tail(question, Some(val)));
    assert!(is_borrow_local_start_tail(question, Some(var)));
    assert!(!is_borrow_local_start_tail(question, None));
    assert!(!is_borrow_local_start_tail(question, Some(ident)));
    assert!(!is_borrow_local_start_tail(ident, Some(val)));
}

#[test]
fn extension_receiver_head_matches_dot_less_and_question_only() {
    for sym in [Symbol::Dot, Symbol::Less, Symbol::Question] {
        let kind = LexemeKind::Token(TokenKind::Symbol(sym));
        assert!(is_extension_receiver_follow_kind(kind));
        assert!(is_extension_receiver_head_kind(true, kind));
        assert!(!is_extension_receiver_head_kind(false, kind));
    }

    for other in [
        LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)),
        LexemeKind::Token(TokenKind::Symbol(Symbol::Colon)),
        LexemeKind::Token(TokenKind::Keyword(Keyword::Val)),
        LexemeKind::Eof,
    ] {
        assert!(!is_extension_receiver_follow_kind(other));
        assert!(!is_extension_receiver_head_kind(true, other));
    }
}
