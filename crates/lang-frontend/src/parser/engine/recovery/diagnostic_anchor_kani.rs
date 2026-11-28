//! SPEC-0506 (F6 ninth slice, Kani track): Direct-Rust bounded symbolic
//! verification harnesses for `finalize_anchor_prefix` and `prefix_has_anchor`.
//!
//! Evidence level: `validated (bounded Rust, len = 0..=8 arbitrary boolean marks)`.

use super::{finalize_anchor_prefix, prefix_has_anchor};

#[kani::proof]
fn verify_diagnostic_anchor_prefix_empty_and_oob() {
    let start: usize = kani::any();
    let end: usize = kani::any();
    assert!(!prefix_has_anchor(&[], start, end));

    let mut prefix = [0usize, 1, 0, 1];
    finalize_anchor_prefix(&mut prefix);
    if start >= prefix.len() || end >= prefix.len() || start >= end {
        assert!(!prefix_has_anchor(&prefix, start, end));
    }
}

#[kani::proof]
#[kani::unwind(10)]
fn verify_diagnostic_anchor_prefix_exact_range_len_8() {
    let len: usize = kani::any();
    kani::assume(len <= 8);

    let mut marks = [false; 8];
    let mut prefix = [0usize; 9];
    let mut i = 0usize;
    while i < len {
        let bit: bool = kani::any();
        marks[i] = bit;
        prefix[i + 1] = usize::from(bit);
        i += 1;
    }

    let slice = &mut prefix[..=len];
    finalize_anchor_prefix(slice);

    // A1: prefix[0] == 0.
    assert_eq!(slice[0], 0);

    // A2: Monotonicity for arbitrary p <= q <= len.
    let p: usize = kani::any();
    let q: usize = kani::any();
    kani::assume(p <= q && q <= len);
    assert!(slice[p] <= slice[q]);

    // A3 & A4: Exact interval anchor query soundness for arbitrary start, end.
    let start: usize = kani::any();
    let end: usize = kani::any();
    let got = prefix_has_anchor(slice, start, end);

    let mut expected = false;
    if start < end && end <= len {
        let mut k = start;
        while k < end {
            if marks[k] {
                expected = true;
            }
            k += 1;
        }
    }
    assert_eq!(got, expected);
}
