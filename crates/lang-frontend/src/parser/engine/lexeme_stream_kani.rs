//! SPEC-0505 (F6 eighth slice, Kani track): Direct-Rust bounded symbolic verification
//! harnesses for `validate_lexeme_spans_core` in `engine.rs`.
//!
//! Evidence level: `validated (bounded Rust, len = 0..=4 arbitrary spans and source_len)`.

use super::validate_lexeme_spans_core;

#[kani::proof]
fn verify_validate_lexeme_stream_rejects_empty_and_missing_eof() {
    let source_len: usize = kani::any();
    assert!(!validate_lexeme_spans_core(&[], source_len));

    let s0: usize = kani::any();
    let e0: usize = kani::any();
    assert!(!validate_lexeme_spans_core(&[(s0, e0, false)], source_len));
}

#[kani::proof]
#[kani::unwind(6)]
fn verify_validate_lexeme_stream_partition_len_4() {
    let len: usize = kani::any();
    kani::assume(len >= 1 && len <= 4);

    let mut spans = [(0usize, 0usize, false); 4];
    let mut i = 0usize;
    while i < len {
        spans[i] = (kani::any(), kani::any(), kani::any());
        i += 1;
    }
    let source_len: usize = kani::any();

    if validate_lexeme_spans_core(&spans[..len], source_len) {
        // V2: Last element is an empty EOF at `source_len`.
        let last = spans[len - 1];
        assert!(last.2);
        assert_eq!(last.0, source_len);
        assert_eq!(last.1, source_len);

        // V4: First element starts at 0.
        assert_eq!(spans[0].0, 0);

        // V3 & V4: Every non-last element is non-EOF, strictly non-empty, and contiguous.
        let mut k = 0usize;
        while k + 1 < len {
            assert!(!spans[k].2);
            assert!(spans[k].0 < spans[k].1);
            assert_eq!(spans[k + 1].0, spans[k].1);
            k += 1;
        }

        // V5: Strict start monotonicity for arbitrary p < q < len.
        let p: usize = kani::any();
        let q: usize = kani::any();
        kani::assume(p < q && q < len);
        assert!(spans[p].0 < spans[q].0);
    }
}
