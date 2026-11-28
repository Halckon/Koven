#![allow(unexpected_cfgs)]

//! Pure indexing, reverse-DP path-end, classification, and budget kernels for
//! `StrictCallTrialIndex` (`crates/lang-frontend/src/parser/trial.rs`).
//!
//! Extracted for SPEC-0507 (F6 tenth slice) so Lean 4 unbounded proofs and
//! Kani bounded direct-Rust harnesses verify the exact production logic.

use super::{ListSuccess, ListTrial, SignificantCallTrial};

/// Reverse-fills `raw_to_significant` so every raw checkpoint (including trivia
/// positions) maps in O(1) to the first significant lexeme at or after `raw`.
#[inline]
pub(super) fn fill_next_significant(raw_to_significant: &mut [Option<usize>]) {
    let mut next_significant = None;
    for slot in raw_to_significant.iter_mut().rev() {
        if let Some(current) = *slot {
            next_significant = Some(current);
        }
        *slot = next_significant;
    }
}

/// Overflow-safe recursion budget check for cached `StrictCallTrialIndex::query`.
///
/// Returns `true` iff `baseline_depth + additional_depth <= max_depth` without overflow.
#[inline]
pub(super) const fn check_recursion_budget(
    baseline_depth: usize,
    additional_depth: usize,
    max_depth: usize,
) -> bool {
    match baseline_depth.checked_add(additional_depth) {
        Some(depth) => depth <= max_depth,
        None => false,
    }
}

/// Single reverse-DP step for `path_ends[index]` over `Ident (. Ident)*`.
#[inline]
pub(super) const fn resolve_path_end(
    is_ident: bool,
    has_member: bool,
    member_end: Option<usize>,
    index: usize,
) -> Option<usize> {
    if !is_ident {
        return None;
    }
    if has_member {
        member_end
    } else {
        Some(index + 1)
    }
}

/// Classifies a `<...>` `ListTrial` and its immediate suffix token into a
/// `SignificantCallTrial` (`Match` vs `NoMatch`) without mutating parser state.
#[inline]
pub(super) const fn classify_call_trial(
    list: ListTrial,
    is_call_suffix: bool,
) -> SignificantCallTrial {
    match list.success {
        Some(ListSuccess { end, closing }) if is_call_suffix => SignificantCallTrial::Match {
            closing_significant: closing,
            suffix_significant: end,
            additional_depth: list.explored_depth,
        },
        Some(ListSuccess { closing, .. }) => SignificantCallTrial::NoMatch {
            additional_depth: list.explored_depth,
            closed_list_significant: Some(closing),
        },
        None => SignificantCallTrial::NoMatch {
            additional_depth: list.explored_depth,
            closed_list_significant: None,
        },
    }
}

#[cfg(kani)]
#[path = "kernel_kani.rs"]
mod kernel_kani;
#[cfg(test)]
#[path = "kernel_tests.rs"]
mod kernel_tests;
