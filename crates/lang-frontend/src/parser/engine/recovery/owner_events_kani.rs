//! SPEC-0510 (F6 Slice 11, Kani track): Direct-Rust bounded symbolic verification
//! harnesses for `owner_events::verify_terminal_owner_events_with` (using fixed stack
//! buffers) and its downstream TCB bridge into `GroupOwners::outside_lexical_symbol`.
//!
//! Evidence level: `validated (bounded Rust, 2 prefix lexemes + Eof, 0..=2 terminal events)`.

use super::{
    GroupOwners, TerminalOwnerEvent, TerminalOwnerKind,
    owner_events::{self, OwnerEventBuffers},
};
use crate::lexer::{InvalidKind, Lexeme, LexemeKind, Symbol, TokenKind};
use crate::parser::ParserInternalError;

const DUMMY_EVENT: TerminalOwnerEvent = TerminalOwnerEvent {
    offset: 0,
    kind: TerminalOwnerKind::String,
    opener: 0,
};

struct FixedOwnerBuffers<const A: usize, const V: usize> {
    active_buf: [(TerminalOwnerKind, usize); A],
    active_len: usize,
    verified_buf: [TerminalOwnerEvent; V],
    verified_len: usize,
}

impl<const A: usize, const V: usize> FixedOwnerBuffers<A, V> {
    const fn new() -> Self {
        Self {
            active_buf: [(TerminalOwnerKind::String, 0); A],
            active_len: 0,
            verified_buf: [DUMMY_EVENT; V],
            verified_len: 0,
        }
    }

    fn verified(&self) -> &[TerminalOwnerEvent] {
        &self.verified_buf[..self.verified_len]
    }
}

impl<const A: usize, const V: usize> OwnerEventBuffers for FixedOwnerBuffers<A, V> {
    fn push_active(&mut self, kind: TerminalOwnerKind, opener: usize) {
        self.active_buf[self.active_len] = (kind, opener);
        self.active_len += 1;
    }

    fn last_active(&self) -> Option<(TerminalOwnerKind, usize)> {
        if self.active_len == 0 {
            None
        } else {
            Some(self.active_buf[self.active_len - 1])
        }
    }

    fn pop_active(&mut self) -> Option<(TerminalOwnerKind, usize)> {
        if self.active_len == 0 {
            None
        } else {
            self.active_len -= 1;
            Some(self.active_buf[self.active_len])
        }
    }

    fn is_active_empty(&self) -> bool {
        self.active_len == 0
    }

    fn push_verified(&mut self, event: TerminalOwnerEvent) {
        self.verified_buf[self.verified_len] = event;
        self.verified_len += 1;
    }
}

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

    let mut buffers = FixedOwnerBuffers::<3, 5>::new();
    if owner_events::verify_terminal_owner_events_with(
        &lexemes,
        input_events,
        &[],
        terminal_error_at_eof,
        source_len,
        &mut buffers,
    )
    .is_ok()
    {
        let verified = buffers.verified();

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

    // Case A: Unclosed StringStart at EOF with terminal_error_at_eof == false must fail.
    let unclosed = [
        Lexeme::synthetic_with_span_for_verification(
            LexemeKind::Token(TokenKind::StringStart),
            0,
            1,
        ),
        Lexeme::synthetic_with_span_for_verification(LexemeKind::Eof, src, src),
    ];
    let mut buf_a = FixedOwnerBuffers::<3, 4>::new();
    let res_unclosed = owner_events::verify_terminal_owner_events_with(
        &unclosed,
        &[],
        &[],
        false,
        src,
        &mut buf_a,
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
    let mut buf_b = FixedOwnerBuffers::<3, 4>::new();
    let res_unconsumed = owner_events::verify_terminal_owner_events_with(
        &eof_only,
        &unconsumed,
        &[],
        true,
        src,
        &mut buf_b,
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
    let mut buf_c = FixedOwnerBuffers::<3, 4>::new();
    owner_events::verify_terminal_owner_events_with(
        &esc_lexemes,
        &[],
        &[(1, 2)],
        false,
        src,
        &mut buf_c,
    )
    .expect("terminal escape closes active string");
    let verified = buf_c.verified();
    assert_eq!(verified.len(), 1);
    assert_eq!(verified[0].offset, 2);
    assert_eq!(verified[0].kind, TerminalOwnerKind::String);
    assert_eq!(verified[0].opener, 0);
    let mut owners = GroupOwners::default();
    for lex in esc_lexemes {
        let _ = owners.outside_lexical_symbol(lex, verified);
    }
    assert_eq!(owners.lexical_depth(), 0);
    assert_eq!(owners.next_terminal_index(), 1);

    // Case D: Any due terminal event whose kind OR opener mismatches the active stack top
    //         must be strictly rejected.
    let open_is_string: bool = kani::any();
    let ev_is_string: bool = kani::any();
    let ev_opener: u8 = kani::any();
    kani::assume(ev_opener <= 3);
    kani::assume(open_is_string != ev_is_string || ev_opener != 0);
    let open_lex = Lexeme::synthetic_with_span_for_verification(
        if open_is_string {
            LexemeKind::Token(TokenKind::StringStart)
        } else {
            LexemeKind::Token(TokenKind::InterpolationStart)
        },
        0,
        1,
    );
    let mismatch_lexemes = [
        open_lex,
        Lexeme::synthetic_with_span_for_verification(LexemeKind::Eof, src, src),
    ];
    let bad_event = [TerminalOwnerEvent {
        offset: 1,
        kind: if ev_is_string {
            TerminalOwnerKind::String
        } else {
            TerminalOwnerKind::Interpolation
        },
        opener: usize::from(ev_opener),
    }];
    let mut buf_d = FixedOwnerBuffers::<3, 4>::new();
    let res_mismatch = owner_events::verify_terminal_owner_events_with(
        &mismatch_lexemes,
        &bad_event,
        &[],
        true,
        src,
        &mut buf_d,
    );
    assert_eq!(res_mismatch, Err(ParserInternalError::InvalidLexemeStream));

    // Case E: Explicit closer of the wrong owner kind must be rejected.
    let wrong_close_lex = Lexeme::synthetic_with_span_for_verification(
        if open_is_string {
            LexemeKind::Token(TokenKind::InterpolationEnd)
        } else {
            LexemeKind::Token(TokenKind::StringEnd)
        },
        1,
        2,
    );
    let wrong_close_stream = [
        open_lex,
        wrong_close_lex,
        Lexeme::synthetic_with_span_for_verification(LexemeKind::Eof, src, src),
    ];
    let mut buf_e = FixedOwnerBuffers::<3, 4>::new();
    let res_wrong_close = owner_events::verify_terminal_owner_events_with(
        &wrong_close_stream,
        &[],
        &[],
        true,
        src,
        &mut buf_e,
    );
    assert_eq!(
        res_wrong_close,
        Err(ParserInternalError::InvalidLexemeStream)
    );
}
