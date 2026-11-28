//! SPEC-0492 (F6 second slice, Kani track): Bounded direct-Rust symbolic verification
//! harnesses for `OrdinaryGroups`, `RecoveryBoundaries::include_unclosed`, and
//! `GroupClosers::confirmed_closer`.
//!
//! Evidence level:
//! - `OrdinaryGroups` stack-count invariant (`O2`/`O3`): `validated (bounded Rust, len in 1..=2)`
//! - `RecoveryBoundaries::include_unclosed` monoid & non-shielding (`O1`): `validated (all symbolic values)`
//! - `GroupClosers::confirmed_closer` stop-set antitonicity, cursor monotonicity & safety (`O4`/`O5`):
//!   `validated (all symbolic cursors, watermarks, enclosing flags, and stop masks)`

use super::ordinary_groups::{GroupClose, GroupClosers, OrdinaryGroups, RecoveryBoundaries};
use crate::lexer::Symbol;
use crate::parser::engine::Stops;
use std::num::NonZeroUsize;

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

fn symbolic_opt_nz() -> Option<NonZeroUsize> {
    let raw: usize = kani::any();
    NonZeroUsize::new(raw)
}

fn symbolic_boundaries() -> RecoveryBoundaries {
    RecoveryBoundaries {
        local: symbolic_opt_nz(),
        caller_else: symbolic_opt_nz(),
        caller_block: symbolic_opt_nz(),
        block_statement_separator: symbolic_opt_nz(),
    }
}

fn stops_from_flags(has_else: bool, has_block: bool, has_sep: bool) -> Stops {
    let mut delimiters = 0u16;
    if has_else {
        delimiters |= Stops::ELSE;
    }
    if has_sep {
        delimiters |= Stops::BLOCK_STATEMENT_SEPARATOR;
    }
    Stops {
        delimiters,
        block_elements: has_block,
        ..Stops::ROOT
    }
}

#[kani::proof]
fn verify_recovery_boundaries_monoid_and_non_shielding() {
    let a = symbolic_boundaries();
    let b = symbolic_boundaries();
    let c = symbolic_boundaries();

    // Associativity: (a ⊔ b) ⊔ c == a ⊔ (b ⊔ c)
    let mut ab = a;
    ab.include_unclosed(b);
    let mut ab_c = ab;
    ab_c.include_unclosed(c);

    let mut bc = b;
    bc.include_unclosed(c);
    let mut a_bc = a;
    a_bc.include_unclosed(bc);
    assert_eq!(ab_c, a_bc);

    // Commutativity: a ⊔ b == b ⊔ a
    let mut ba = b;
    ba.include_unclosed(a);
    assert_eq!(ab, ba);

    // Idempotence: a ⊔ a == a
    let mut aa = a;
    aa.include_unclosed(a);
    assert_eq!(aa, a);

    // Non-shielding: any child watermark >= cursor remains >= cursor after merge.
    let cursor: usize = kani::any();
    let at_or_after = |opt: Option<NonZeroUsize>| opt.is_some_and(|raw| raw.get() >= cursor);
    if at_or_after(b.local) {
        assert!(at_or_after(ab.local));
    }
    if at_or_after(b.caller_else) {
        assert!(at_or_after(ab.caller_else));
    }
    if at_or_after(b.caller_block) {
        assert!(at_or_after(ab.caller_block));
    }
    if at_or_after(b.block_statement_separator) {
        assert!(at_or_after(ab.block_statement_separator));
    }
}

fn check_ordinary_groups_invariant<const N: usize>() {
    let mut groups = OrdinaryGroups::with_capacity(N);
    let mut i = 0usize;
    while i < N {
        let kind: usize = kani::any();
        kani::assume(kind < 3);
        let opening: bool = kani::any();
        let before_active = groups.active_counts();

        let matched = groups.visit(i, symbol_from_pair(kind, opening));
        let after_active = groups.active_counts();

        if opening {
            assert!(matched.is_none());
            assert_eq!(after_active[kind], before_active[kind] + 1);
        } else {
            assert_eq!(matched.is_some(), before_active[kind] > 0);
            if let Some(opener_raw) = matched {
                assert!(opener_raw < i);
            }
        }
        i += 1;
    }

    let final_counts = groups.stack_kind_counts();
    let final_active = groups.active_counts();
    assert_eq!(final_active[0], final_counts[0]);
    assert_eq!(final_active[1], final_counts[1]);
    assert_eq!(final_active[2], final_counts[2]);
}

#[kani::proof]
#[kani::unwind(4)]
fn verify_ordinary_groups_active_invariant_len_1() {
    check_ordinary_groups_invariant::<1>();
}

#[kani::proof]
#[kani::unwind(5)]
fn verify_ordinary_groups_active_invariant_len_2() {
    check_ordinary_groups_invariant::<2>();
}

#[kani::proof]
fn verify_confirmed_closer_monotonicity_and_safety() {
    let boundaries = symbolic_boundaries();
    let has_enclosing: bool = kani::any();
    let enclosing_closed: bool = kani::any();
    let closer: usize = kani::any();
    let enclosing_closer: usize = kani::any();

    // Layout: opener 0 is outer (if any), opener 1 is inner under test.
    let inner_close = GroupClose {
        closer,
        enclosing: has_enclosing.then(|| NonZeroUsize::new(1).unwrap()),
        boundaries,
    };
    let (by_opener, pairs) = if has_enclosing && enclosing_closed {
        let outer_close = GroupClose {
            closer: enclosing_closer,
            enclosing: None,
            boundaries: RecoveryBoundaries::default(),
        };
        (
            vec![NonZeroUsize::new(1), NonZeroUsize::new(2)],
            vec![outer_close, inner_close],
        )
    } else {
        (vec![None, NonZeroUsize::new(1)], vec![inner_close])
    };
    let closers = GroupClosers::from_parts(by_opener, pairs);

    let e1: bool = kani::any();
    let b1: bool = kani::any();
    let s1: bool = kani::any();
    let e2: bool = kani::any();
    let b2: bool = kani::any();
    let s2: bool = kani::any();
    kani::assume((!e1 || e2) && (!b1 || b2) && (!s1 || s2));
    let stops1 = stops_from_flags(e1, b1, s1);
    let stops2 = stops_from_flags(e2, b2, s2);

    let cursor1: usize = kani::any();
    let cursor2: usize = kani::any();
    kani::assume(cursor1 <= cursor2);

    let r_c1_s1 = closers.confirmed_closer(1, cursor1, stops1);
    let r_c1_s2 = closers.confirmed_closer(1, cursor1, stops2);
    let r_c2_s1 = closers.confirmed_closer(1, cursor2, stops1);

    // O4a: stop-set antitonicity (s1 <= s2 => confirmed under s2 implies confirmed under s1).
    if let Some(c) = r_c1_s2 {
        assert_eq!(r_c1_s1, Some(c));
    }
    // O4b: cursor monotonicity (cursor1 <= cursor2 => confirmed at c1 implies confirmed at c2).
    if let Some(c) = r_c1_s1 {
        assert_eq!(r_c2_s1, Some(c));
    }
    // O5: boundary safety whenever confirmed.
    if let Some(c) = r_c1_s1 {
        assert_eq!(c, closer);
        assert!(!has_enclosing || enclosing_closed);
        let at_or_after = |opt: Option<NonZeroUsize>| opt.is_some_and(|raw| raw.get() >= cursor1);
        assert!(!at_or_after(boundaries.local));
        if e1 {
            assert!(!at_or_after(boundaries.caller_else));
        }
        if b1 {
            assert!(!at_or_after(boundaries.caller_block));
        }
        if s1 {
            assert!(!at_or_after(boundaries.block_statement_separator));
        }
    }
}
