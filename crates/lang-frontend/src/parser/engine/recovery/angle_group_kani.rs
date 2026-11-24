//! SPEC-0490 (F6 Kani track): Bounded direct-Rust symbolic verification harnesses
//! for `match_angle_group_delimiters` vs. `OrdinaryGroups::visit`.
//!
//! Evidence level:
//! - Exact equivalence (`match_angle_group_delimiters` vs. `OrdinaryGroups::visit`):
//!   `validated (bounded Rust, delimiters.len() in 0..=2 with lexeme_len <= 6, plus delimiters.len() == 3 with lexeme_len in {3, 6})`
//! - P4 strict-successor / non-zero closer & bounds safety:
//!   `validated (bounded Rust, delimiters.len() == 4, lexeme_len == 8)`
//!
//! Each delimiter count `N` is verified as a separate compile-time constant harness so
//! CBMC can constant-fold the `boundaries` table dimensions and avoid symbolic-length
//! heap-array explosion on standard CI runners.

use super::angle_groups::match_angle_group_delimiters;
use super::ordinary_groups::OrdinaryGroups;
use crate::lexer::Symbol;
use std::num::NonZeroUsize;

const MAX_LEXEME_LEN: usize = 6;

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

fn symbolic_delimiters_with_len<const N: usize>(lexeme_len: usize) -> [(usize, usize, bool); N] {
    let mut delimiters = [(0usize, 0usize, false); N];
    let mut next_raw = 0usize;
    let mut i = 0usize;
    while i < N {
        let raw: usize = kani::any();
        let remaining = N - i;
        kani::assume(raw >= next_raw && raw <= lexeme_len - remaining);
        next_raw = raw + 1;
        let kind: usize = kani::any();
        kani::assume(kind < 3);
        let opening: bool = kani::any();
        delimiters[i] = (raw, kind, opening);
        i += 1;
    }
    delimiters
}

fn check_equivalence_with_len<const N: usize>(lexeme_len: usize) {
    let delimiters = symbolic_delimiters_with_len::<N>(lexeme_len);
    let indexed = match_angle_group_delimiters(&delimiters, lexeme_len);
    assert_eq!(indexed.len(), lexeme_len);

    if lexeme_len == 0 {
        return;
    }
    // Universally quantified slot check: proving for an arbitrary symbolic `slot < lexeme_len`
    // is equivalent to checking every slot in `0..lexeme_len`, while avoiding an O(n^2)
    // nested symbolic loop and auxiliary heap allocations.
    let slot: usize = kani::any();
    kani::assume(slot < lexeme_len);

    let mut opener_idx = None;
    let mut i = 0usize;
    while i < N {
        if delimiters[i].0 == slot && delimiters[i].2 {
            opener_idx = Some(i);
        }
        i += 1;
    }

    let expected = if let Some(start) = opener_idx {
        let mut groups = OrdinaryGroups::with_capacity(N);
        let mut found = None;
        let mut j = 0usize;
        while j < N {
            if j >= start && found.is_none() {
                let (raw, k, op) = delimiters[j];
                if groups.visit(raw, symbol_from_pair(k, op)) == Some(slot) {
                    found = Some(raw);
                }
            }
            j += 1;
        }
        found
    } else {
        None
    };

    assert_eq!(indexed[slot].map(NonZeroUsize::get), expected);
}

fn check_equivalence_symbolic_lexeme_len<const N: usize>() {
    let lexeme_len: usize = kani::any();
    kani::assume(lexeme_len >= N && lexeme_len <= MAX_LEXEME_LEN);
    check_equivalence_with_len::<N>(lexeme_len);
}

#[kani::proof]
#[kani::unwind(9)]
fn verify_angle_group_closers_equivalence_len_0() {
    check_equivalence_symbolic_lexeme_len::<0>();
}

#[kani::proof]
#[kani::unwind(9)]
fn verify_angle_group_closers_equivalence_len_1() {
    check_equivalence_symbolic_lexeme_len::<1>();
}

#[kani::proof]
#[kani::unwind(9)]
fn verify_angle_group_closers_equivalence_len_2() {
    check_equivalence_symbolic_lexeme_len::<2>();
}

#[kani::proof]
#[kani::unwind(9)]
fn verify_angle_group_closers_equivalence_len_3_dense() {
    check_equivalence_with_len::<3>(3);
}

#[kani::proof]
#[kani::unwind(9)]
fn verify_angle_group_closers_equivalence_len_3_sparse() {
    check_equivalence_with_len::<3>(MAX_LEXEME_LEN);
}

#[kani::proof]
#[kani::unwind(9)]
fn verify_angle_group_closer_strictly_after_opener_and_nonzero_len_4() {
    const N: usize = 4;
    const L: usize = 8;
    let delimiters = symbolic_delimiters_with_len::<N>(L);
    let indexed = match_angle_group_delimiters(&delimiters, L);
    assert_eq!(indexed.len(), L);

    let slot: usize = kani::any();
    kani::assume(slot < L);

    if let Some(closer) = indexed[slot] {
        let mut is_opener = false;
        let mut i = 0usize;
        while i < N {
            if delimiters[i].0 == slot && delimiters[i].2 {
                is_opener = true;
            }
            i += 1;
        }
        assert!(is_opener);
        let closer_raw = closer.get();
        assert!(closer_raw > slot);
        assert!(closer_raw < L);
        assert!(closer_raw >= 1);
    }
}
