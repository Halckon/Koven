//! SPEC-0490 (F6 Kani track): Bounded direct-Rust symbolic verification harnesses
//! for `match_angle_group_delimiters` vs. `OrdinaryGroups::visit`.
//!
//! Evidence level: `validated (bounded Rust, delimiters.len() <= 6, lexeme_len <= 12)`
//! — directly checks panic-freedom, bounds safety, `NonZeroUsize` non-lossiness, and
//! exact equivalence on the production Rust implementation across all strictly
//! increasing delimiter index layouts (including the final slot `lexeme_len - 1`).

use super::angle_groups::match_angle_group_delimiters;
use super::ordinary_groups::OrdinaryGroups;
use crate::lexer::Symbol;
use std::num::NonZeroUsize;

const MAX_LEN: usize = 6;
const MAX_LEXEME_LEN: usize = 12;

fn symbol_from_pair(kind: usize, opening: bool) -> Symbol {
    match (kind, opening) {
        (0, true) => Symbol::LeftParen,
        (0, false) => Symbol::RightParen,
        (1, true) => Symbol::LeftBracket,
        (1, false) => Symbol::RightBracket,
        (2, true) => Symbol::LeftBrace,
        (2, false) => Symbol::RightBrace,
        _ => unreachable!(),
    }
}

fn symbolic_delimiters() -> (Vec<(usize, usize, bool)>, usize) {
    let lexeme_len: usize = kani::any();
    kani::assume(lexeme_len <= MAX_LEXEME_LEN);
    let len: usize = kani::any();
    kani::assume(len <= MAX_LEN && len <= lexeme_len);
    let mut delimiters = Vec::with_capacity(len);
    let mut next_raw = 0usize;
    let mut i = 0usize;
    while i < len {
        let raw: usize = kani::any();
        let remaining = len - i;
        kani::assume(raw >= next_raw && raw + remaining <= lexeme_len);
        next_raw = raw + 1;
        let kind: usize = kani::any();
        kani::assume(kind < 3);
        let opening: bool = kani::any();
        delimiters.push((raw, kind, opening));
        i += 1;
    }
    (delimiters, lexeme_len)
}

#[kani::proof]
#[kani::unwind(14)]
fn verify_angle_group_closers_equivalence_bounded() {
    let (delimiters, lexeme_len) = symbolic_delimiters();
    let indexed = match_angle_group_delimiters(&delimiters, lexeme_len);
    assert_eq!(indexed.len(), lexeme_len);

    let mut i = 0usize;
    while i < delimiters.len() {
        let (opener_raw, _kind, opening) = delimiters[i];
        if opening {
            let mut groups = OrdinaryGroups::default();
            let mut expected = None;
            let mut j = i;
            while j < delimiters.len() {
                let (raw, k, op) = delimiters[j];
                if groups.visit(raw, symbol_from_pair(k, op)) == Some(opener_raw) {
                    expected = Some(raw);
                    break;
                }
                j += 1;
            }
            assert_eq!(indexed[opener_raw].map(NonZeroUsize::get), expected);
        } else {
            assert_eq!(indexed[opener_raw], None);
        }
        i += 1;
    }
}

#[kani::proof]
#[kani::unwind(14)]
fn verify_angle_group_closer_strictly_after_opener_and_nonzero() {
    let (delimiters, lexeme_len) = symbolic_delimiters();
    let indexed = match_angle_group_delimiters(&delimiters, lexeme_len);

    let mut i = 0usize;
    while i < delimiters.len() {
        let (raw, _, opening) = delimiters[i];
        if let Some(closer) = indexed[raw] {
            assert!(opening);
            let closer_raw = closer.get();
            assert!(closer_raw > raw);
            assert!(closer_raw < lexeme_len);
            assert!(closer_raw >= 1);
        }
        i += 1;
    }
}
