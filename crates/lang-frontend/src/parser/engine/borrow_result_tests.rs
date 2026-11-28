use super::borrow_result::{
    ExplicitResultAction, borrow_local_contract_is_invalid, explicit_result_action,
    is_borrow_local_start_tail,
};
use super::declaration_head::{is_extension_receiver_follow_kind, is_extension_receiver_head_kind};
use super::{FunctionForm, FunctionResultSource, Item, parse_declaration};
use crate::lexer::{Keyword, LexemeKind, Symbol, TokenKind, lex_test_source_twice};
use crate::source::SourceMap;

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
        assert!(is_extension_receiver_head_kind(true, Some(kind)));
        assert!(!is_extension_receiver_head_kind(false, Some(kind)));
    }
    assert!(!is_extension_receiver_head_kind(true, None));
    assert!(!is_extension_receiver_head_kind(false, None));

    for other in [
        LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)),
        LexemeKind::Token(TokenKind::Symbol(Symbol::Colon)),
        LexemeKind::Token(TokenKind::Keyword(Keyword::Val)),
        LexemeKind::Eof,
    ] {
        assert!(!is_extension_receiver_follow_kind(other));
        assert!(!is_extension_receiver_head_kind(true, Some(other)));
    }
}

#[test]
fn explicit_result_production_entry_distinguishes_borrow_and_carrier_ast() {
    for (text, expect_borrow) in [
        ("fun f(x: Int): borrow Int from x = x", true),
        ("fun f(x: Int): Int from x = x", false),
    ] {
        let mut sources = SourceMap::new();
        let source_id = sources
            .add_source("explicit-result-source.ko", text)
            .expect("test source name must be unique");
        let lexed = lex_test_source_twice(&sources, source_id, "explicit result source");
        let parsed = parse_declaration(&sources, &lexed).expect("function must parse");
        assert!(
            parsed.diagnostics().is_empty(),
            "{text:?} unexpectedly emitted diagnostics: {:?}",
            parsed.diagnostics()
        );
        let Item::Function { form, .. } = parsed
            .ast()
            .items()
            .get(parsed.root())
            .expect("function root")
            .payload()
        else {
            panic!("{text:?}: expected Item::Function");
        };
        let FunctionForm::Explicit { result_source, .. } = form else {
            panic!("{text:?}: expected FunctionForm::Explicit");
        };
        if expect_borrow {
            assert!(
                matches!(result_source, Some(FunctionResultSource::Borrow(_))),
                "{text:?}: expected FunctionResultSource::Borrow, got {result_source:?}"
            );
        } else {
            assert!(
                matches!(result_source, Some(FunctionResultSource::Carrier(_))),
                "{text:?}: expected FunctionResultSource::Carrier, got {result_source:?}"
            );
        }
    }
}

#[test]
fn function_head_production_entry_rejects_non_identifier_before_dot_as_extension_receiver() {
    let text = "fun 1.foo() {}";
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("non-ident-extension.ko", text)
        .expect("test source name must be unique");
    let lexed = lex_test_source_twice(&sources, source_id, "non-ident extension receiver");
    let parsed = parse_declaration(&sources, &lexed).expect("recovery must remain user diagnostic");
    assert!(
        !parsed.diagnostics().is_empty(),
        "non-identifier function head must emit a diagnostic"
    );
    let Item::Function {
        extension_receiver, ..
    } = parsed
        .ast()
        .items()
        .get(parsed.root())
        .expect("function root")
        .payload()
    else {
        panic!("expected Item::Function");
    };
    assert!(
        extension_receiver.is_none(),
        "non-identifier followed by '.' must not enter extension receiver branch"
    );
}
