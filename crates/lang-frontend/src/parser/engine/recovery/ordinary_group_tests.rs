//! Exhaustive Rust unit tests mirroring SPEC-0492 (F6 second slice) O1–O5 properties
//! on production `OrdinaryGroups`, `RecoveryBoundaries`, and `GroupClosers::confirmed_closer`.

use super::ordinary_groups::{GroupClose, GroupClosers, OrdinaryGroups, RecoveryBoundaries};
use crate::lexer::Symbol;
use crate::parser::engine::Stops;
use std::num::NonZeroUsize;

const SYMBOLS: [(usize, bool, Symbol); 6] = [
    (0, true, Symbol::LeftParen),
    (0, false, Symbol::RightParen),
    (1, true, Symbol::LeftBracket),
    (1, false, Symbol::RightBracket),
    (2, true, Symbol::LeftBrace),
    (2, false, Symbol::RightBrace),
];

#[test]
fn ordinary_groups_active_invariant_holds_exhaustively_up_to_len_5() {
    let mut seq = Vec::with_capacity(5);
    let mut checked = 0usize;
    for target_len in 0..=5 {
        let total = 6usize.pow(target_len as u32);
        for mut code in 0..total {
            seq.clear();
            for _ in 0..target_len {
                seq.push(SYMBOLS[code % 6]);
                code /= 6;
            }
            let mut groups = OrdinaryGroups::with_capacity(target_len);
            for (raw, &(kind, opening, symbol)) in seq.iter().enumerate() {
                let before = groups.stack_kind_counts();
                assert_eq!(groups.active_counts(), before);
                let matched = groups.visit(raw, symbol);
                let after = groups.stack_kind_counts();
                assert_eq!(groups.active_counts(), after);
                if opening {
                    assert!(matched.is_none());
                    assert_eq!(after[kind], before[kind] + 1);
                } else {
                    assert_eq!(matched.is_some(), before[kind] > 0);
                    if let Some(opener) = matched {
                        assert!(opener < raw);
                    }
                }
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 1 + 6 + 36 + 216 + 1296 + 7776);
}

#[test]
fn recovery_boundaries_include_unclosed_is_monoid_and_non_shielding() {
    let opts = [
        None,
        NonZeroUsize::new(1),
        NonZeroUsize::new(3),
        NonZeroUsize::new(5),
    ];
    for &a_l in &opts {
        for &b_l in &opts {
            for &c_l in &opts {
                let a = RecoveryBoundaries {
                    local: a_l,
                    caller_else: b_l,
                    caller_block: c_l,
                    block_statement_separator: a_l,
                };
                let b = RecoveryBoundaries {
                    local: b_l,
                    caller_else: c_l,
                    caller_block: a_l,
                    block_statement_separator: c_l,
                };
                let c = RecoveryBoundaries {
                    local: c_l,
                    caller_else: a_l,
                    caller_block: b_l,
                    block_statement_separator: b_l,
                };

                let mut ab = a;
                ab.include_unclosed(b);
                let mut ab_c = ab;
                ab_c.include_unclosed(c);

                let mut bc = b;
                bc.include_unclosed(c);
                let mut a_bc = a;
                a_bc.include_unclosed(bc);
                assert_eq!(ab_c, a_bc);

                let mut ba = b;
                ba.include_unclosed(a);
                assert_eq!(ab, ba);

                let mut aa = a;
                aa.include_unclosed(a);
                assert_eq!(aa, a);

                for cursor in 0..=6 {
                    if b.local.is_some_and(|raw| raw.get() >= cursor) {
                        assert!(ab.local.is_some_and(|raw| raw.get() >= cursor));
                    }
                    if b.caller_else.is_some_and(|raw| raw.get() >= cursor) {
                        assert!(ab.caller_else.is_some_and(|raw| raw.get() >= cursor));
                    }
                }
            }
        }
    }
}

#[test]
fn confirmed_closer_satisfies_monotonicity_and_boundary_safety() {
    let opts = [None, NonZeroUsize::new(2), NonZeroUsize::new(4)];
    let flag_sets = [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (false, false, true),
        (true, true, false),
        (true, false, true),
        (false, true, true),
        (true, true, true),
    ];
    let to_stops = |(e, b, s): (bool, bool, bool)| {
        let mut delimiters = 0u16;
        if e {
            delimiters |= Stops::ELSE;
        }
        if s {
            delimiters |= Stops::BLOCK_STATEMENT_SEPARATOR;
        }
        Stops {
            delimiters,
            block_elements: b,
            ..Stops::ROOT
        }
    };

    for &local in &opts {
        for &caller_else in &opts {
            for &caller_block in &opts {
                for &sep in &opts {
                    let boundaries = RecoveryBoundaries {
                        local,
                        caller_else,
                        caller_block,
                        block_statement_separator: sep,
                    };
                    for has_enclosing in [false, true] {
                        for enclosing_closed in [false, true] {
                            let inner = GroupClose {
                                closer: 9,
                                enclosing: has_enclosing.then(|| NonZeroUsize::new(1).unwrap()),
                                boundaries,
                            };
                            let closers = if has_enclosing && enclosing_closed {
                                GroupClosers::from_parts(
                                    vec![NonZeroUsize::new(1), NonZeroUsize::new(2)],
                                    vec![
                                        GroupClose {
                                            closer: 12,
                                            enclosing: None,
                                            boundaries: RecoveryBoundaries::default(),
                                        },
                                        inner,
                                    ],
                                )
                            } else {
                                GroupClosers::from_parts(
                                    vec![None, NonZeroUsize::new(1)],
                                    vec![inner],
                                )
                            };
                            for &f1 in &flag_sets {
                                let s1 = to_stops(f1);
                                for &f2 in &flag_sets {
                                    if (!f1.0 || f2.0) && (!f1.1 || f2.1) && (!f1.2 || f2.2) {
                                        let s2 = to_stops(f2);
                                        for c1 in 0..=5 {
                                            let r1_s1 = closers.confirmed_closer(1, c1, s1);
                                            let r1_s2 = closers.confirmed_closer(1, c1, s2);
                                            if let Some(cl) = r1_s2 {
                                                assert_eq!(r1_s1, Some(cl));
                                            }
                                            for c2 in c1..=5 {
                                                let r2_s1 = closers.confirmed_closer(1, c2, s1);
                                                if let Some(cl) = r1_s1 {
                                                    assert_eq!(r2_s1, Some(cl));
                                                }
                                            }
                                            if let Some(cl) = r1_s1 {
                                                assert_eq!(cl, 9);
                                                assert!(!has_enclosing || enclosing_closed);
                                                assert!(!local.is_some_and(|raw| raw.get() >= c1));
                                                if f1.0 {
                                                    assert!(
                                                        !caller_else
                                                            .is_some_and(|raw| raw.get() >= c1)
                                                    );
                                                }
                                                if f1.1 {
                                                    assert!(
                                                        !caller_block
                                                            .is_some_and(|raw| raw.get() >= c1)
                                                    );
                                                }
                                                if f1.2 {
                                                    assert!(
                                                        !sep.is_some_and(|raw| raw.get() >= c1)
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
