//! SPEC-0510 (F6 Slice 11): Unit tests for `owner_events::verify_terminal_owner_events`
//! and its downstream bridge into `GroupOwners::outside_lexical_symbol`.

use super::{GroupOwners, TerminalOwnerEvent, TerminalOwnerKind, owner_events};
use crate::lexer::{InvalidKind, Lexeme, LexemeKind, Symbol, TokenKind};
use crate::parser::ParserInternalError;

fn make_lexeme(kind: LexemeKind, start: usize, end: usize) -> Lexeme {
    Lexeme::synthetic_with_span_for_verification(kind, start, end)
}

#[test]
fn balanced_nesting_and_terminal_events_bridge_to_group_owners() {
    // Nested: StringStart(0..1) InterpolationStart(1..3) StringStart(3..4) [unterminated at 6]
    // Symbol('(') at 4..5 is inside inner string, then InterpolationEnd(6..7), StringEnd(7..8), Eof(8..8).
    let lexemes = vec![
        make_lexeme(LexemeKind::Token(TokenKind::StringStart), 0, 1),
        make_lexeme(LexemeKind::Token(TokenKind::InterpolationStart), 1, 3),
        make_lexeme(LexemeKind::Token(TokenKind::StringStart), 3, 4),
        make_lexeme(
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)),
            4,
            5,
        ),
        make_lexeme(LexemeKind::Token(TokenKind::InterpolationEnd), 6, 7),
        make_lexeme(LexemeKind::Token(TokenKind::StringEnd), 7, 8),
        make_lexeme(
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightParen)),
            8,
            9,
        ),
        make_lexeme(LexemeKind::Eof, 9, 9),
    ];
    let input_events = vec![TerminalOwnerEvent {
        offset: 6,
        kind: TerminalOwnerKind::String,
        opener: 3,
    }];
    let verified =
        owner_events::verify_terminal_owner_events(&lexemes, &input_events, &[], false, 9)
            .expect("valid nested stream with terminal event");
    assert_eq!(verified, input_events);

    // Downstream GroupOwners bridge: never underflows, filters inner '(', emits outer ')', ends at depth 0.
    let mut owners = GroupOwners::default();
    let mut emitted = Vec::new();
    for lex in &lexemes {
        if let Some(sym) = owners.outside_lexical_symbol(*lex, &verified) {
            emitted.push(sym);
        }
    }
    assert_eq!(emitted, vec![Symbol::RightParen]);
    assert_eq!(owners.lexical_depth(), 0);
    assert_eq!(owners.next_terminal_index(), verified.len());
}

#[test]
fn terminal_escape_synthesizes_string_owner_event() {
    let lexemes = vec![
        make_lexeme(LexemeKind::Token(TokenKind::StringStart), 0, 1),
        make_lexeme(LexemeKind::Invalid(InvalidKind::InvalidStringEscape), 2, 3),
        make_lexeme(
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)),
            4,
            5,
        ),
        make_lexeme(LexemeKind::Eof, 5, 5),
    ];
    let verified = owner_events::verify_terminal_owner_events(&lexemes, &[], &[(2, 3)], false, 5)
        .expect("terminal escape closes active string");
    assert_eq!(
        verified,
        vec![TerminalOwnerEvent {
            offset: 3,
            kind: TerminalOwnerKind::String,
            opener: 0,
        }]
    );

    let mut owners = GroupOwners::default();
    let mut emitted = Vec::new();
    for lex in &lexemes {
        if let Some(sym) = owners.outside_lexical_symbol(*lex, &verified) {
            emitted.push(sym);
        }
    }
    assert_eq!(emitted, vec![Symbol::LeftParen]);
    assert_eq!(owners.lexical_depth(), 0);
    assert_eq!(owners.next_terminal_index(), verified.len());
}

#[test]
fn eof_cascade_drain_requires_terminal_error_at_eof() {
    let lexemes = vec![
        make_lexeme(LexemeKind::Token(TokenKind::StringStart), 0, 1),
        make_lexeme(LexemeKind::Token(TokenKind::InterpolationStart), 2, 4),
        make_lexeme(LexemeKind::Eof, 5, 5),
    ];
    assert_eq!(
        owner_events::verify_terminal_owner_events(&lexemes, &[], &[], false, 5),
        Err(ParserInternalError::InvalidLexemeStream)
    );
    let verified = owner_events::verify_terminal_owner_events(&lexemes, &[], &[], true, 5)
        .expect("EOF cascade succeeds when terminal_error_at_eof is true");
    assert_eq!(
        verified,
        vec![
            TerminalOwnerEvent {
                offset: 5,
                kind: TerminalOwnerKind::Interpolation,
                opener: 2,
            },
            TerminalOwnerEvent {
                offset: 5,
                kind: TerminalOwnerKind::String,
                opener: 0,
            },
        ]
    );
}

#[test]
fn rejects_unmatched_closers_wrong_opener_and_unconsumed_events() {
    // Unmatched StringEnd
    let unmatched = vec![
        make_lexeme(LexemeKind::Token(TokenKind::StringEnd), 0, 1),
        make_lexeme(LexemeKind::Eof, 1, 1),
    ];
    assert_eq!(
        owner_events::verify_terminal_owner_events(&unmatched, &[], &[], false, 1),
        Err(ParserInternalError::InvalidLexemeStream)
    );

    // Wrong opener offset in terminal event
    let wrong_opener = vec![
        make_lexeme(LexemeKind::Token(TokenKind::StringStart), 0, 1),
        make_lexeme(LexemeKind::Eof, 4, 4),
    ];
    assert_eq!(
        owner_events::verify_terminal_owner_events(
            &wrong_opener,
            &[TerminalOwnerEvent {
                offset: 3,
                kind: TerminalOwnerKind::String,
                opener: 99,
            }],
            &[],
            false,
            4,
        ),
        Err(ParserInternalError::InvalidLexemeStream)
    );

    // Unconsumed terminal event (offset past EOF)
    let ok_lexemes = vec![make_lexeme(LexemeKind::Eof, 2, 2)];
    assert_eq!(
        owner_events::verify_terminal_owner_events(
            &ok_lexemes,
            &[TerminalOwnerEvent {
                offset: 10,
                kind: TerminalOwnerKind::String,
                opener: 0,
            }],
            &[],
            false,
            2,
        ),
        Err(ParserInternalError::InvalidLexemeStream)
    );
}
