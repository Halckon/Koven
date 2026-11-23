//! Pure 8-state backward DP matching for ordinary delimiters outside lexical owners.
//!
//! Extracted from `recovery.rs` so both production `angle_group_closers` and
//! F6 verification harnesses (`#[cfg(kani)]` / exhaustive tests) target the
//! exact same implementation with zero overhead.

use std::num::NonZeroUsize;

/// Match each opener in `delimiters` (where each entry is `(raw_index, kind, opening)`
/// with `kind ∈ {0, 1, 2}` and strictly increasing `raw_index < lexeme_len`) against
/// the closer that a fresh `OrdinaryGroups` starting at that opener would select.
pub(super) fn match_angle_group_delimiters(
    delimiters: &[(usize, usize, bool)],
    lexeme_len: usize,
) -> Vec<Option<NonZeroUsize>> {
    let eof = delimiters.len();
    let mut boundaries = vec![[eof; 8]; eof + 1];
    let mut matched = vec![None; lexeme_len];
    let closes = |boundary: usize, kind: usize| {
        delimiters
            .get(boundary)
            .is_some_and(|&(_, found, opening)| !opening && found == kind)
    };
    for (index, &(raw, kind, opening)) in delimiters.iter().enumerate().rev() {
        let shape = 1 << kind;
        for active in 0..8 {
            boundaries[index][active] = if opening {
                let end = boundaries[index + 1][active | shape];
                if closes(end, kind) {
                    boundaries[end + 1][active]
                } else {
                    // 异形 closer 仍交给实际拥有它的祖先，孤立右花括号是硬边界。
                    end
                }
            } else if active & shape != 0 || kind == 2 {
                index
            } else {
                boundaries[index + 1][active]
            };
        }
        if opening {
            let end = boundaries[index + 1][shape];
            if closes(end, kind) {
                matched[raw] = NonZeroUsize::new(delimiters[end].0);
            }
        }
    }
    matched
}
