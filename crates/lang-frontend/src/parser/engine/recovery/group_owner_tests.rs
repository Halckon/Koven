//! Exhaustive Rust unit tests mirroring SPEC-0498 (F6 third slice) G1–G4 properties
//! on production `GroupOwners::outside_lexical_symbol`.

use super::{GroupOwners, TerminalOwnerEvent, TerminalOwnerKind};
use crate::lexer::{LexemeKind, Symbol, TokenKind};

const EVENT_KINDS: [LexemeKind; 6] = [
    LexemeKind::Token(TokenKind::StringStart),
    LexemeKind::Token(TokenKind::InterpolationStart),
    LexemeKind::Token(TokenKind::StringEnd),
    LexemeKind::Token(TokenKind::InterpolationEnd),
    LexemeKind::Token(TokenKind::Symbol(Symbol::LeftParen)),
    LexemeKind::Token(TokenKind::Identifier),
];

#[test]
fn group_owners_preserves_depth_and_filters_nested_symbols_exhaustively() {
    let mut checked = 0usize;

    // Enumerate all valid owner/symbol sequences of length 1..=5 with optional terminal drain.
    for len in 1..=5 {
        let total = 6usize.pow(len as u32);
        for mut code in 0..total {
            let mut kinds = Vec::with_capacity(len);
            for _ in 0..len {
                kinds.push(EVENT_KINDS[code % 6]);
                code /= 6;
            }
            for drain_at in [None, Some(2usize), Some(4usize)] {
                let mut ref_stack = Vec::new();
                let mut terminals = Vec::new();
                let mut valid = true;
                for (idx, &kind) in kinds.iter().enumerate() {
                    if drain_at == Some(idx) {
                        if let Some(top) = ref_stack.pop() {
                            terminals.push(TerminalOwnerEvent {
                                offset: idx,
                                kind: top,
                                opener: 0,
                            });
                        } else {
                            valid = false;
                            break;
                        }
                    }
                    match kind {
                        LexemeKind::Token(TokenKind::StringStart) => {
                            ref_stack.push(TerminalOwnerKind::String);
                        }
                        LexemeKind::Token(TokenKind::InterpolationStart) => {
                            ref_stack.push(TerminalOwnerKind::Interpolation);
                        }
                        LexemeKind::Token(TokenKind::StringEnd)
                            if ref_stack.pop() != Some(TerminalOwnerKind::String) =>
                        {
                            valid = false;
                            break;
                        }
                        LexemeKind::Token(TokenKind::InterpolationEnd)
                            if ref_stack.pop() != Some(TerminalOwnerKind::Interpolation) =>
                        {
                            valid = false;
                            break;
                        }
                        _ => {}
                    }
                }
                if !valid {
                    continue;
                }

                let mut owners = GroupOwners::default();
                let mut shadow_depth = 0usize;
                let mut next_term = 0usize;
                for (idx, &kind) in kinds.iter().enumerate() {
                    while next_term < terminals.len() && terminals[next_term].offset <= idx {
                        shadow_depth -= 1;
                        next_term += 1;
                    }
                    let post_drain = shadow_depth;
                    match kind {
                        LexemeKind::Token(
                            TokenKind::StringStart | TokenKind::InterpolationStart,
                        ) => {
                            shadow_depth += 1;
                        }
                        LexemeKind::Token(TokenKind::StringEnd | TokenKind::InterpolationEnd) => {
                            shadow_depth -= 1;
                        }
                        _ => {}
                    }
                    let prev_term = owners.next_terminal_index();
                    let emitted = owners.outside_lexical_symbol_parts(idx, kind, &terminals);
                    assert_eq!(owners.lexical_depth(), shadow_depth);
                    assert_eq!(owners.next_terminal_index(), next_term);
                    assert!(owners.next_terminal_index() >= prev_term);
                    assert!(owners.next_terminal_index() <= terminals.len());
                    let expected = match kind {
                        LexemeKind::Token(TokenKind::Symbol(sym)) if post_drain == 0 => Some(sym),
                        _ => None,
                    };
                    assert_eq!(emitted, expected);
                }
                checked += 1;
            }
        }
    }
    assert!(
        checked > 1000,
        "expected substantial valid sequence coverage, got {checked}"
    );
}
