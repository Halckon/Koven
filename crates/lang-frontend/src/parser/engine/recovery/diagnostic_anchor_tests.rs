use super::{finalize_anchor_prefix, prefix_has_anchor};

#[test]
fn diagnostic_anchor_prefix_matches_exact_exists_over_all_6_bit_patterns() {
    // Empty fast-path always returns false.
    for start in 0..=4 {
        for end in 0..=4 {
            assert!(!prefix_has_anchor(&[], start, end));
        }
    }

    // Exhaustive over all 2^6 = 64 mark vectors and all (start, end) including OOB.
    const N: usize = 6;
    for mask in 0usize..(1 << N) {
        let mut marks = [false; N];
        let mut prefix = [0usize; N + 1];
        for i in 0..N {
            marks[i] = ((mask >> i) & 1) != 0;
            prefix[i + 1] = usize::from(marks[i]);
        }
        finalize_anchor_prefix(&mut prefix);
        assert_eq!(prefix[0], 0);
        for i in 0..N {
            assert!(prefix[i] <= prefix[i + 1]);
        }
        for start in 0..=(N + 1) {
            for end in 0..=(N + 1) {
                let expected = start < end && end <= N && marks[start..end].iter().any(|&m| m);
                assert_eq!(
                    prefix_has_anchor(&prefix, start, end),
                    expected,
                    "mask={mask:#b}, start={start}, end={end}"
                );
            }
        }
    }
}
