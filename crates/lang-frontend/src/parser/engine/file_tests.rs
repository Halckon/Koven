use super::boundary::ReceiverModifierPolicy;
use super::file::{
    FileHeaderSeparatorDecision, FileRootSeparatorDiagnostic, ModifierScanState,
    ModifierStepOutcome, ModifierToken, file_header_separator_decision,
    file_root_separator_diagnostic, run_declaration_modifiers, step_declaration_modifier,
};
use crate::lexer::{Keyword, LexemeKind, Symbol, TokenKind};

#[test]
fn file_header_separator_matches_eof_newline_semicolon_and_missing() {
    let semi = LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon));
    let kw_fun = LexemeKind::Token(TokenKind::Keyword(Keyword::Fun));
    assert_eq!(
        file_header_separator_decision(LexemeKind::Eof, false),
        FileHeaderSeparatorDecision::Satisfied
    );
    assert_eq!(
        file_header_separator_decision(kw_fun, true),
        FileHeaderSeparatorDecision::Satisfied
    );
    assert_eq!(
        file_header_separator_decision(semi, false),
        FileHeaderSeparatorDecision::ConsumeSemicolon
    );
    assert_eq!(
        file_header_separator_decision(kw_fun, false),
        FileHeaderSeparatorDecision::Missing
    );
}

#[test]
fn file_root_separator_distinguishes_header_and_declaration_diagnostics() {
    let kw_pkg = LexemeKind::Token(TokenKind::Keyword(Keyword::Package));
    let kw_imp = LexemeKind::Token(TokenKind::Keyword(Keyword::Import));
    let kw_fun = LexemeKind::Token(TokenKind::Keyword(Keyword::Fun));
    let semi = LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon));
    assert_eq!(
        file_root_separator_diagnostic(true, true, false, kw_pkg),
        FileRootSeparatorDiagnostic::ExpectedFileHeaderSeparator
    );
    assert_eq!(
        file_root_separator_diagnostic(true, true, false, kw_imp),
        FileRootSeparatorDiagnostic::ExpectedFileHeaderSeparator
    );
    assert_eq!(
        file_root_separator_diagnostic(true, true, false, kw_fun),
        FileRootSeparatorDiagnostic::ExpectedDeclarationSeparator
    );
    assert_eq!(
        file_root_separator_diagnostic(true, true, true, kw_fun),
        FileRootSeparatorDiagnostic::None
    );
    assert_eq!(
        file_root_separator_diagnostic(true, true, false, semi),
        FileRootSeparatorDiagnostic::None
    );
}

#[test]
fn declaration_modifiers_enforce_visibility_override_receiver_order() {
    let (state, errors) = run_declaration_modifiers(
        true,
        ReceiverModifierPolicy::Allowed,
        ModifierScanState::default(),
        &[
            ModifierToken::Visibility,
            ModifierToken::Override,
            ModifierToken::Receiver,
        ],
    );
    assert_eq!(errors, 0);
    assert!(state.has_visibility && state.saw_override && state.saw_receiver);

    let (_, errors) = run_declaration_modifiers(
        true,
        ReceiverModifierPolicy::Allowed,
        ModifierScanState::default(),
        &[ModifierToken::Override, ModifierToken::Visibility],
    );
    assert_eq!(errors, 1);

    let (state, outcome) = step_declaration_modifier(
        true,
        ReceiverModifierPolicy::NotRecognized,
        ModifierScanState::default(),
        ModifierToken::Receiver,
    );
    assert_eq!(outcome, ModifierStepOutcome::Stop);
    assert!(!state.saw_receiver);
}
