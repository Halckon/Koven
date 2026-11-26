use crate::parser::MAX_RECURSION_DEPTH;

use super::{
    ListSuccess, ListTrial, SignificantCallTrial, check_recursion_budget, classify_call_trial,
    fill_next_significant, resolve_path_end,
};

#[test]
fn fill_next_significant_matches_first_non_trivia_over_all_6_bit_patterns() {
    const N: usize = 6;
    for mask in 0usize..(1 << N) {
        let mut is_trivia = [false; N];
        let mut raw_to_sig = [None; N];
        let mut sig_to_raw = Vec::new();
        for i in 0..N {
            is_trivia[i] = ((mask >> i) & 1) != 0;
            if !is_trivia[i] {
                raw_to_sig[i] = Some(sig_to_raw.len());
                sig_to_raw.push(i);
            }
        }
        fill_next_significant(&mut raw_to_sig);
        for (r, slot) in raw_to_sig.iter().enumerate() {
            let expected_raw = (r..N).find(|&j| !is_trivia[j]);
            assert_eq!(slot.map(|s| sig_to_raw[s]), expected_raw);
        }
    }
}

#[test]
fn resolve_path_end_matches_maximal_qualified_chain_over_all_3_pow_6_patterns() {
    const N: usize = 6;
    for mut code in 0usize..729 {
        let mut kinds = [0u8; N];
        for slot in &mut kinds {
            *slot = (code % 3) as u8;
            code /= 3;
        }
        let mut path_ends = [None; N];
        for idx in (0..N).rev() {
            let is_ident = kinds[idx] == 0;
            let has_member = idx + 2 < N && kinds[idx + 1] == 1 && kinds[idx + 2] == 0;
            let member_end = if has_member { path_ends[idx + 2] } else { None };
            path_ends[idx] = resolve_path_end(is_ident, has_member, member_end, idx);
        }
        for p in 0..N {
            if kinds[p] != 0 {
                assert_eq!(path_ends[p], None);
            } else {
                let mut e = p + 1;
                while e + 1 < N && kinds[e] == 1 && kinds[e + 1] == 0 {
                    e += 2;
                }
                assert_eq!(path_ends[p], Some(e));
            }
        }
    }
}

#[test]
fn recursion_budget_and_call_trial_classification_respect_bounds() {
    assert!(check_recursion_budget(0, 0, MAX_RECURSION_DEPTH));
    assert!(check_recursion_budget(
        MAX_RECURSION_DEPTH,
        0,
        MAX_RECURSION_DEPTH
    ));
    assert!(!check_recursion_budget(
        MAX_RECURSION_DEPTH,
        1,
        MAX_RECURSION_DEPTH
    ));
    assert!(!check_recursion_budget(usize::MAX, 1, MAX_RECURSION_DEPTH));
    assert!(!check_recursion_budget(1, usize::MAX, MAX_RECURSION_DEPTH));

    let closed = ListTrial {
        success: Some(ListSuccess { end: 5, closing: 4 }),
        explored_depth: 3,
    };
    assert!(matches!(
        classify_call_trial(closed, true),
        SignificantCallTrial::Match {
            closing_significant: 4,
            suffix_significant: 5,
            additional_depth: 3,
        }
    ));
    assert!(matches!(
        classify_call_trial(closed, false),
        SignificantCallTrial::NoMatch {
            additional_depth: 3,
            closed_list_significant: Some(4),
        }
    ));
    let failed = ListTrial {
        success: None,
        explored_depth: 2,
    };
    assert!(matches!(
        classify_call_trial(failed, true),
        SignificantCallTrial::NoMatch {
            additional_depth: 2,
            closed_list_significant: None,
        }
    ));
}
