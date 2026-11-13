//! CI-only corruption of the actual double-resumption producer. Never execute locally.
use super::*;

fn capture_cascade() -> Snapshot {
    capture_text(
        "fun left(s: String): borrow String from s = s\nfun middle(s: String): borrow String from s = s\nfun right(s: String): borrow String from s = s\nfun wrap(s: String, a: Boolean, b: Boolean): borrow String from s { if (a) { return left(s) } else { if (b) { return middle(s) } }\nreturn right(s) }",
    )
}
fn complete_state(source: &ActionSource) -> NestedForwardState {
    let CascadeProgress::OuterResumed(state) = source.nested_regions[0].cascade else {
        panic!("actual two-resumption producer")
    };
    state
}
#[test]
fn ci_only_cascade_closure_rejects_missing_reordered_and_foreign_evidence() {
    for fault in 0..12 {
        let (mut source, mut paths) = capture_cascade();
        let baseline = source.finish_nested_regions(&paths);
        assert_eq!(baseline[0].coverage(), UnitLoanActionCoverage::Complete);
        assert!(baseline[0].cascade_continuation().is_some());
        assert!(baseline[0].continuation().is_none());
        let state = complete_state(&source);
        let c = &mut source.nested_regions[0];
        match fault {
            0 => c.cascade = CascadeProgress::Empty,
            1 => c.cascade = CascadeProgress::InnerResumed(state),
            2 => c.cascade = CascadeProgress::OuterPending(state),
            3 => c.inner_closed = true,
            4 => c.stage = 19,
            5 => c.entries.swap(0, 2),
            6 => {
                let mut wrong = state;
                wrong.path.swap(0, 1);
                c.cascade = CascadeProgress::OuterResumed(wrong);
            }
            7 => paths[2].identity = Arc::new(()),
            8 => source.nested_entries[c.entries[2].unwrap()].parent = None,
            9 => source.nested_entries[c.entries[0].unwrap()].cursor += 1,
            10 => source.nested_region_join(0, 1, true, true, true),
            _ => c.closed = None,
        }
        let result = source.finish_nested_regions(&paths);
        assert_eq!(
            result[0].coverage(),
            UnitLoanActionCoverage::Partial,
            "fault {fault}"
        );
        assert!(result[0].paths().is_empty());
        assert!(result[0].cascade_continuation().is_none());
    }
}
#[test]
fn ci_only_cascade_outer_token_requires_exact_inner_resumed_state() {
    for fault in 0..7 {
        let (mut source, _) = capture_cascade();
        let state = complete_state(&source);
        source.records.truncate(source.nested_regions[0].start + 4);
        let c = &mut source.nested_regions[0];
        c.stage = 14;
        c.closed = None;
        c.cascade = CascadeProgress::InnerResumed(state);
        let mut successor = state;
        match fault {
            0 => c.cascade = CascadeProgress::Empty,
            1 => c.cascade = CascadeProgress::InnerPending(state),
            2 => successor.path[1] = c.entries[1],
            3 => successor.coverage = Some(1),
            _ => {}
        }
        let token = source.nested_cascade_continuation(
            0,
            0,
            Some(successor),
            fault != 4,
            fault == 5,
            fault != 6,
        );
        assert!(token.is_none(), "fault {fault}");
        // Failed evidence cannot recover by subsequently supplying a valid successor.
        assert!(
            source
                .nested_cascade_continuation(0, 0, Some(state), true, false, true)
                .is_none()
        );
    }
}
#[test]
fn ci_only_cascade_tokens_reject_cross_level_foreign_replay_and_pollution() {
    for split in [1, 0] {
        for fault in 0..7 {
            let (mut source, _) = capture_cascade();
            let state = complete_state(&source);
            source.records.truncate(source.nested_regions[0].start + 4);
            let c = &mut source.nested_regions[0];
            c.stage = if split == 1 { 13 } else { 15 };
            c.closed = None;
            c.cascade = if split == 1 {
                CascadeProgress::InnerPending(state)
            } else {
                CascadeProgress::OuterPending(state)
            };
            let mut token = ContinuationToken {
                identity: source.identity.clone(),
                candidate: 0,
                cursor: source.records.len(),
                state,
                entry: c.entries[if split == 1 { 2 } else { 0 }].unwrap(),
                cascade_split: Some(split),
            };
            match fault {
                0 => token.identity = Arc::new(()),
                1 => token.cascade_split = Some(1 - split),
                2 => token.cursor += 1,
                3 => token.entry = c.entries[3].unwrap(),
                4 => token.cascade_split = None,
                _ => {}
            }
            let result = source.resume_nested_continuation(token.clone(), fault != 5);
            assert_eq!(result.is_some(), fault == 6, "split {split}, fault {fault}");
            assert!(source.resume_nested_continuation(token, true).is_none());
        }
    }
}
