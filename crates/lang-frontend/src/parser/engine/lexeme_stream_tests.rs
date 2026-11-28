use super::validate_lexeme_spans_core;

#[test]
fn validate_lexeme_spans_core_accepts_valid_partitions_and_rejects_malformed_streams() {
    // Valid empty source (single empty EOF at 0..0).
    assert!(validate_lexeme_spans_core(&[(0, 0, true)], 0));
    // Valid multi-token contiguous partition ending with empty EOF.
    assert!(validate_lexeme_spans_core(
        &[(0, 3, false), (3, 4, false), (4, 10, false), (10, 10, true)],
        10
    ));

    // Empty stream rejected.
    assert!(!validate_lexeme_spans_core(&[], 0));
    // Missing EOF rejected.
    assert!(!validate_lexeme_spans_core(&[(0, 3, false)], 3));
    // Gap between spans rejected.
    assert!(!validate_lexeme_spans_core(
        &[(0, 2, false), (3, 5, false), (5, 5, true)],
        5
    ));
    // Overlap between spans rejected.
    assert!(!validate_lexeme_spans_core(
        &[(0, 4, false), (3, 5, false), (5, 5, true)],
        5
    ));
    // Empty non-EOF token rejected.
    assert!(!validate_lexeme_spans_core(
        &[(0, 0, false), (0, 0, true)],
        0
    ));
    // Non-empty EOF rejected.
    assert!(!validate_lexeme_spans_core(&[(0, 1, true)], 1));
    // Early EOF before trailing token rejected.
    assert!(!validate_lexeme_spans_core(
        &[(0, 0, true), (0, 1, false)],
        1
    ));
    // Wrong source_len rejected.
    assert!(!validate_lexeme_spans_core(
        &[(0, 3, false), (3, 3, true)],
        4
    ));
}
