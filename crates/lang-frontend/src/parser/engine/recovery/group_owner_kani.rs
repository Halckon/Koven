//! SPEC-0498 (F6 third slice, Kani track): Direct-Rust bounded symbolic verification
//! harness for `GroupOwners::outside_lexical_symbol` lexical-depth preservation,
//! terminal-event monotonicity, and top-level delimiter isolation.
//!
//! Evidence level: `validated (bounded Rust, len = 4 lexemes, <= 2 terminal events)`.

use super::{GroupOwners, TerminalOwnerEvent, TerminalOwnerKind};
use crate::lexer::{LexemeKind, Symbol, TokenKind};

const MAX_LEXEMES: usize = 4;

fn symbolic_lexeme_kind() -> LexemeKind {
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
#[kani::unwind(6)]
fn verify_group_owners_depth_and_filtering_len_4() {
    let mut owners = GroupOwners::default();
    let mut ref_stack = [TerminalOwnerKind::String; MAX_LEXEMES];
    let mut ref_depth = 0usize;

    let term_count: usize = kani::any();
    kani::assume(term_count <= 2);
    let t0_off: usize = kani::any();
    let t1_off: usize = kani::any();
    kani::assume(t0_off <= t1_off && t1_off <= 16);
    let t0_is_str: bool = kani::any();
    let t1_is_str: bool = kani::any();
    let all_terms = [
        TerminalOwnerEvent {
            offset: t0_off,
            kind: if t0_is_str {
                TerminalOwnerKind::String
            } else {
                TerminalOwnerKind::Interpolation
            },
            opener: 0,
        },
        TerminalOwnerEvent {
            offset: t1_off,
            kind: if t1_is_str {
                TerminalOwnerKind::String
            } else {
                TerminalOwnerKind::Interpolation
            },
            opener: 0,
        },
    ];
    let terminals = &all_terms[..term_count];
    let mut ref_next_term = 0usize;
    let mut prev_offset = 0usize;

    let mut i = 0usize;
    while i < MAX_LEXEMES {
        let start: usize = kani::any();
        kani::assume(start >= prev_offset && start <= 16);
        prev_offset = start;
        let kind = symbolic_lexeme_kind();

        // Advance reference owner stack across due terminal events (matching LexicalRecoveryIndex::new).
        while ref_next_term < term_count && terminals[ref_next_term].offset <= start {
            kani::assume(
                ref_depth > 0 && ref_stack[ref_depth - 1] == terminals[ref_next_term].kind,
            );
            ref_depth -= 1;
            ref_next_term += 1;
        }
        let post_drain_depth = ref_depth;

        // Enforce valid StringEnd / InterpolationEnd pairing on the reference stack.
        match kind {
            LexemeKind::Token(TokenKind::StringStart) => {
                ref_stack[ref_depth] = TerminalOwnerKind::String;
                ref_depth += 1;
            }
            LexemeKind::Token(TokenKind::InterpolationStart) => {
                ref_stack[ref_depth] = TerminalOwnerKind::Interpolation;
                ref_depth += 1;
            }
            LexemeKind::Token(TokenKind::StringEnd) => {
                kani::assume(
                    ref_depth > 0 && ref_stack[ref_depth - 1] == TerminalOwnerKind::String,
                );
                ref_depth -= 1;
            }
            LexemeKind::Token(TokenKind::InterpolationEnd) => {
                kani::assume(
                    ref_depth > 0 && ref_stack[ref_depth - 1] == TerminalOwnerKind::Interpolation,
                );
                ref_depth -= 1;
            }
            _ => {}
        }

        let prev_term_idx = owners.next_terminal_index();
        let emitted = owners.outside_lexical_symbol_parts(start, kind, terminals);

        assert_eq!(owners.lexical_depth(), ref_depth);
        assert_eq!(owners.next_terminal_index(), ref_next_term);
        assert!(owners.next_terminal_index() >= prev_term_idx);
        assert!(owners.next_terminal_index() <= terminals.len());

        let expected = match kind {
            LexemeKind::Token(TokenKind::Symbol(sym)) if post_drain_depth == 0 => Some(sym),
            _ => None,
        };
        assert_eq!(emitted, expected);
        i += 1;
    }
}
