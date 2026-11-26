#![allow(unexpected_cfgs)]

//! Pure prefix-sum kernel for `LexicalRecoveryIndex::diagnostic_anchor_prefix`.
//!
//! Extracted for SPEC-0506 (F6 ninth slice) so Kani harnesses and exhaustive
//! unit tests verify the exact production construction and range-query logic.

#[inline]
pub(super) fn finalize_anchor_prefix(marks: &mut [usize]) {
    let mut count = 0usize;
    for mark in marks {
        count += *mark;
        *mark = count;
    }
}

#[inline]
pub(super) fn prefix_has_anchor(prefix: &[usize], start: usize, end: usize) -> bool {
    match (prefix.get(start), prefix.get(end)) {
        (Some(before), Some(after)) => before < after,
        _ => false,
    }
}

#[cfg(kani)]
#[path = "diagnostic_anchor_kani.rs"]
mod diagnostic_anchor_kani;
#[cfg(test)]
#[path = "diagnostic_anchor_tests.rs"]
mod diagnostic_anchor_tests;
