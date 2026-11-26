//! SPEC-0507 (F6 tenth slice, Kani track): Direct-Rust bounded symbolic
//! verification harnesses for `StrictCallTrialIndex` pure kernels.
//!
//! Evidence level: `validated (bounded Rust, len = 0..=8 arbitrary streams & depths)`.

use crate::parser::MAX_RECURSION_DEPTH;

use super::{
    ListSuccess, ListTrial, SignificantCallTrial, check_recursion_budget, classify_call_trial,
    fill_next_significant, resolve_path_end,
};

#[kani::proof]
#[kani::unwind(10)]
fn verify_raw_to_significant_exact_and_monotone_len_8() {
    let len: usize = kani::any();
    kani::assume(len <= 8);

    let mut is_trivia = [false; 8];
    let mut raw_to_sig = [None; 8];
    let mut sig_to_raw = [0usize; 8];
    let mut sig_count = 0usize;

    let mut raw = 0usize;
    while raw < len {
        let trivia: bool = kani::any();
        is_trivia[raw] = trivia;
        if !trivia {
            raw_to_sig[raw] = Some(sig_count);
            sig_to_raw[sig_count] = raw;
            sig_count += 1;
        }
        raw += 1;
    }

    fill_next_significant(&mut raw_to_sig[..len]);

    // T3: `sig_to_raw` is strictly increasing.
    if sig_count >= 2 {
        let s1: usize = kani::any();
        let s2: usize = kani::any();
        kani::assume(s1 < s2 && s2 < sig_count);
        assert!(sig_to_raw[s1] < sig_to_raw[s2]);
    }

    // T1: Exact next non-trivia lookup for arbitrary `r < len`.
    if len > 0 {
        let r: usize = kani::any();
        kani::assume(r < len);

        let mut first_non_trivia: Option<usize> = None;
        let mut j = r;
        while j < len {
            if !is_trivia[j] {
                first_non_trivia = Some(j);
                break;
            }
            j += 1;
        }

        match (raw_to_sig[r], first_non_trivia) {
            (Some(s), Some(j)) => {
                assert!(s < sig_count);
                assert_eq!(sig_to_raw[s], j);
                assert!(r <= j);
                if !is_trivia[r] {
                    assert_eq!(j, r);
                }
            }
            (None, None) => {}
            _ => panic!("raw_to_significant mismatch with first non-trivia index"),
        }

        // T2: Monotonicity across r1 <= r2 < len.
        let r1: usize = kani::any();
        let r2: usize = kani::any();
        kani::assume(r1 <= r2 && r2 < len);
        if let Some(s2) = raw_to_sig[r2] {
            let Some(s1) = raw_to_sig[r1] else {
                panic!("earlier raw index must also see a significant token");
            };
            assert!(s1 <= s2);
        }
    }
}

#[kani::proof]
#[kani::unwind(10)]
fn verify_trial_path_ends_and_budget_len_8() {
    let len: usize = kani::any();
    kani::assume(len <= 8);

    // 0 = Ident, 1 = Dot, 2 = Other
    let mut kinds = [2u8; 8];
    let mut i = 0usize;
    while i < len {
        let k: u8 = kani::any();
        kani::assume(k <= 2);
        kinds[i] = k;
        i += 1;
    }

    let mut path_ends = [None; 8];
    let mut idx = len;
    while idx > 0 {
        idx -= 1;
        let is_ident = kinds[idx] == 0;
        let has_member = idx + 2 < len && kinds[idx + 1] == 1 && kinds[idx + 2] == 0;
        let member_end = if has_member { path_ends[idx + 2] } else { None };
        path_ends[idx] = resolve_path_end(is_ident, has_member, member_end, idx);
    }

    // T4: Every `path_ends[p] = Some(e)` strictly advances, stays in bounds, and is maximal.
    if len > 0 {
        let p: usize = kani::any();
        kani::assume(p < len);
        match path_ends[p] {
            Some(e) => {
                assert_eq!(kinds[p], 0);
                assert!(p < e && e <= len);
                assert_eq!((e - p) % 2, 1);
                // Cannot extend further with `. Ident`.
                assert!(!(e + 1 < len && kinds[e] == 1 && kinds[e + 1] == 0));
            }
            None => {
                assert_ne!(kinds[p], 0);
            }
        }
    }

    // T5: `classify_call_trial` ordering and `closed_list_significant` preservation.
    let opener: usize = kani::any();
    let closing: usize = kani::any();
    kani::assume(opener < closing && closing < usize::MAX);
    let end = closing + 1;
    let depth: usize = kani::any();
    let is_suffix: bool = kani::any();
    let has_success: bool = kani::any();
    let list = ListTrial {
        success: has_success.then_some(ListSuccess { end, closing }),
        explored_depth: depth,
    };
    match classify_call_trial(list, is_suffix) {
        SignificantCallTrial::Match {
            closing_significant,
            suffix_significant,
            additional_depth,
        } => {
            assert!(has_success && is_suffix);
            assert!(opener < closing_significant);
            assert!(closing_significant < suffix_significant);
            assert_eq!(suffix_significant, closing_significant + 1);
            assert_eq!(additional_depth, depth);
        }
        SignificantCallTrial::NoMatch {
            additional_depth,
            closed_list_significant,
        } => {
            assert!(!(has_success && is_suffix));
            assert_eq!(additional_depth, depth);
            assert_eq!(closed_list_significant, has_success.then_some(closing));
        }
    }

    // T6: `check_recursion_budget` overflow safety & monotonicity.
    let baseline: usize = kani::any();
    let additional: usize = kani::any();
    let ok = check_recursion_budget(baseline, additional, MAX_RECURSION_DEPTH);
    assert_eq!(
        ok,
        baseline
            .checked_add(additional)
            .is_some_and(|total| total <= MAX_RECURSION_DEPTH)
    );
    if !ok && baseline < usize::MAX {
        assert!(!check_recursion_budget(
            baseline + 1,
            additional,
            MAX_RECURSION_DEPTH
        ));
    }
}
