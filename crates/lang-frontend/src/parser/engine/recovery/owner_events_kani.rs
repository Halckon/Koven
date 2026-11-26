//! SPEC-0510 (F6 Slice 11, Kani track): Direct-Rust bounded symbolic verification
//! harnesses for `owner_events::verify_terminal_owner_events_into` (zero-allocation
//! core called by `verify_terminal_owner_events`) and its downstream TCB bridge into
//! `GroupOwners::outside_lexical_symbol`.
//!
//! Evidence level: `validated (bounded Rust, len = 3 contiguous lexemes incl. Eof, <= 2 terminal events)`.

use super::{GroupOwners, TerminalOwnerEvent, TerminalOwnerKind, owner_events};
use crate::lexer::{InvalidKind, Lexeme, LexemeKind, Symbol, TokenKind};
use crate::parser::ParserInternalError;

const DUMMY_EVENT: TerminalOwnerEvent = TerminalOwnerEvent {
    offset: 0,
    kind: TerminalOwnerKind::String,
    opener: 0,
};

fn symbolic_prefix_kind() -> LexemeKind {
    let tag: u8 = kani::any();
    kani::assume(tag < 6);
    match tag {
        0 => LexemeKind::Token(TokenKind::StringStart),
        1 => LexemeKind::Token(TokenKind::InterpolationStart),
        2 => LexemeKind::Token(TokenKind::StringEnd),
        3 => LexemeKind::Token(TokenKind::InterpolationEnd),
        4 => LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)),
        _ => LexemeKind::Token(TokenKind::Identifier),
    }
}

#[kani::proof]
#[kani::unwind(5)]
fn verify_owner_events_soundness_monotonicity_and_group_owners_bridge() {
    let source_len = 2usize;
    let k0 = symbolic_prefix_kind();
    let k1 = symbolic_prefix_kind();
    let lexemes = [
        Lexeme::synthetic_with_span_for_verification(k0, 0, 1),
        Lexeme::synthetic_with_span_for_verification(k1, 1, 2),
        Lexeme::synthetic_with_span_for_verification(LexemeKind::Eof, 2, 2),
    ];

    let mut opens = 0usize;
    let mut explicit_closes = 0usize;
    for lex in &lexemes[..2] {
        match lex.kind() {
            LexemeKind::Token(TokenKind::StringStart | TokenKind::InterpolationStart) => {
                opens += 1;
            }
            LexemeKind::Token(TokenKind::StringEnd | TokenKind::InterpolationEnd) => {
                explicit_closes += 1;
            }
            _ => {}
        }
    }

    let term_count: usize = kani::any();
    kani::assume(term_count <= 2);
    let t0_off: u8 = kani::any();
    let t1_off: u8 = kani::any();
    kani::assume(t0_off <= t1_off && t1_off <= 2);
    let t0_opener: u8 = kani::any();
    let t1_opener: u8 = kani::any();
    kani::assume(t0_opener <= 1 && t1_opener <= 1);
    let all_terms = [
        TerminalOwnerEvent {
            offset: usize::from(t0_off),
            kind: if kani::any() {
                TerminalOwnerKind::String
            } else {
                TerminalOwnerKind::Interpolation
            },
            opener: usize::from(t0_opener),
        },
        TerminalOwnerEvent {
            offset: usize::from(t1_off),
            kind: if kani::any() {
                TerminalOwnerKind::String
            } else {
                TerminalOwnerKind::Interpolation
            },
            opener: usize::from(t1_opener),
        },
    ];
    let input_events = &all_terms[..term_count];
    let terminal_error_at_eof: bool = kani::any();

    let mut active_buf = [(TerminalOwnerKind::String, 0); 3];
    let mut verified_buf = [DUMMY_EVENT; 5];

    if let Ok(verified_len) = owner_events::verify_terminal_owner_events_into(
        &lexemes,
        input_events,
        &[],
        terminal_error_at_eof,
        source_len,
        &mut active_buf,
        &mut verified_buf,
    ) {
        let verified = &verified_buf[..verified_len];

        // 1. Exact global balance at EOF: opens == explicit_closes + verified.len()
        assert_eq!(opens, explicit_closes + verified.len());

        // 2. Monotonicity & source_len upper bound of verified event offsets
        let mut prev_ev_off = 0usize;
        let mut j = 0usize;
        while j < verified.len() {
            assert!(verified[j].offset >= prev_ev_off);
            assert!(verified[j].offset <= source_len);
            prev_ev_off = verified[j].offset;
            j += 1;
        }

        // 3. Direct Rust Slice 11 -> Slice 3 bridge: GroupOwners never underflows,
        //    consumes all verified events, and terminates at lexical_depth == 0.
        let mut owners = GroupOwners::default();
        let mut k = 0usize;
        while k < lexemes.len() {
            let _ = owners.outside_lexical_symbol(lexemes[k], verified);
            k += 1;
        }
        assert_eq!(owners.lexical_depth(), 0);
        assert_eq!(owners.next_terminal_index(), verified.len());
    }
}

#[kani::proof]
#[kani::unwind(5)]
fn verify_owner_events_rejects_unclosed_and_unconsumed() {
    let source_len: u8 = kani::any();
    kani::assume((2..=8).contains(&source_len));
    let src = usize::from(source_len);

    let mut active_buf = [(TerminalOwnerKind::String, 0); 3];
    let mut verified_buf = [DUMMY_EVENT; 4];

    // Case A: Unclosed StringStart at EOF with terminal_error_at_eof == false must fail.
    let unclosed = [
        Lexeme::synthetic_with_span_for_verification(
            LexemeKind::Token(TokenKind::StringStart),
            0,
            1,
        ),
        Lexeme::synthetic_with_span_for_verification(LexemeKind::Eof, src, src),
    ];
    let res_unclosed = owner_events::verify_terminal_owner_events_into(
        &unclosed,
        &[],
        &[],
        false,
        src,
        &mut active_buf,
        &mut verified_buf,
    );
    assert_eq!(res_unclosed, Err(ParserInternalError::InvalidLexemeStream));

    // Case B: TerminalOwnerEvent with offset > source_len is never consumed and must fail.
    let extra_offset: u8 = kani::any();
    kani::assume(extra_offset > source_len && extra_offset <= 12);
    let eof_only = [Lexeme::synthetic_with_span_for_verification(
        LexemeKind::Eof,
        src,
        src,
    )];
    let unconsumed = [TerminalOwnerEvent {
        offset: usize::from(extra_offset),
        kind: TerminalOwnerKind::String,
        opener: 0,
    }];
    let res_unconsumed = owner_events::verify_terminal_owner_events_into(
        &eof_only,
        &unconsumed,
        &[],
        true,
        src,
        &mut active_buf,
        &mut verified_buf,
    );
    assert_eq!(
        res_unconsumed,
        Err(ParserInternalError::InvalidLexemeStream)
    );

    // Case C: Terminal escape inside active String synthesizes a matching String event
    //         and bridges cleanly to GroupOwners (depth == 0 at EOF).
    let esc_lexemes = [
        Lexeme::synthetic_with_span_for_verification(
            LexemeKind::Token(TokenKind::StringStart),
            0,
            1,
        ),
        Lexeme::synthetic_with_span_for_verification(
            LexemeKind::Invalid(InvalidKind::InvalidStringEscape),
            1,
            2,
        ),
        Lexeme::synthetic_with_span_for_verification(LexemeKind::Eof, src, src),
    ];
    let verified_len = owner_events::verify_terminal_owner_events_into(
        &esc_lexemes,
        &[],
        &[(1, 2)],
        false,
        src,
        &mut active_buf,
        &mut verified_buf,
    )
    .expect("terminal escape closes active string");
    assert_eq!(verified_len, 1);
    assert_eq!(verified_buf[0].offset, 2);
    assert_eq!(verified_buf[0].kind, TerminalOwnerKind::String);
    assert_eq!(verified_buf[0].opener, 0);
    let mut owners = GroupOwners::default();
    for lex in esc_lexemes {
        let _ = owners.outside_lexical_symbol(lex, &verified_buf[..verified_len]);
    }
    assert_eq!(owners.lexical_depth(), 0);
    assert_eq!(owners.next_terminal_index(), 1);
}
