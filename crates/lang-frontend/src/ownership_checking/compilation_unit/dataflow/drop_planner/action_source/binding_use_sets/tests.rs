//! CI-only fault injection into genuinely captured pending-return collections; compile locally only.
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use std::cell::RefCell;
type Snapshot = (Candidate, Arc<()>, Vec<UnitLoanAction>);
thread_local! {static CAPTURE:RefCell<Option<Vec<Snapshot>>>=const{RefCell::new(None)};}
pub(super) fn record(c: &Candidate, identity: &Arc<()>, actions: &[UnitLoanAction]) {
    CAPTURE.with(|v| {
        if let Some(v) = v.borrow_mut().as_mut() {
            v.push((c.clone(), identity.clone(), actions.to_vec()));
        }
    });
}
fn capture() -> Snapshot {
    capture_word("ABBB")
}
fn capture_word(word: &str) -> Snapshot {
    CAPTURE.with(|v| *v.borrow_mut() = Some(Vec::new()));
    let calls = word
        .chars()
        .map(|c| {
            if c == 'A' {
                "consume(aux);"
            } else {
                "consume(item);"
            }
        })
        .collect::<String>();
    let text = format!(
        "fun view(source: String, aux: String): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun run(): Unit {{val owner=\"one\";val aux=\"two\";borrow val item=view(owner,aux);{calls}}}"
    );
    let fixture = Fixture::new(&[("multi-use.ko", &text)]);
    let shared = fixture.shared();
    assert_eq!(
        shared.loan_action_observations()[0]
            .binding_use_sets()
            .len(),
        1
    );
    let mut captures = CAPTURE.with(|v| v.borrow_mut().take().unwrap());
    assert_eq!(captures.len(), 1);
    captures.pop().unwrap()
}
#[test]
fn ci_only_use_set_rejects_missing_reordered_and_foreign_returns() {
    let (original, identity, actions) = capture();
    assert!(original.clone().finish(&identity, &actions).is_some());
    for fault in 0..15 {
        let mut c = original.clone();
        match fault {
            0 => c.valid = false,
            1 => c.end = None,
            2 => c.source = None,
            3 => c.handoff = None,
            4 => {
                c.returns.remove(1);
            }
            5 => c.returns.swap(0, 1),
            6 => c.returns[1] = c.returns[0].clone(),
            7 => c.returns[0].begin.identity = Arc::new(()),
            8 => c.returns[0].cursor += 1,
            9 => c.expected.consumers.swap(0, 1),
            10 => c.expected.consumers.truncate(1),
            11 => c.expected.source_index = usize::MAX,
            12 => c.start = usize::MAX,
            13 => c.end.as_mut().unwrap().identity = Arc::new(()),
            _ => c.expected.auxiliary_uses.clear(),
        }
        assert!(c.finish(&identity, &actions).is_none(), "fault {fault}");
    }
    for record in [
        actions[original.returns[1].begin.index].clone(),
        actions[original.handoff.as_ref().unwrap().index].clone(),
        actions[original.end.as_ref().unwrap().index].clone(),
    ] {
        let mut corrupted = actions.clone();
        corrupted.push(record);
        assert!(original.clone().finish(&identity, &corrupted).is_none());
    }
    let mut missing = actions.clone();
    missing.remove(original.returns[1].begin.index);
    assert!(original.finish(&identity, &missing).is_none());
}

#[test]
fn ci_only_source_retirement_requires_actual_removed_owner_and_unique_drop_without_erasing_set() {
    let (original, identity, actions) = capture();
    assert!(original.source_retirement(&identity, &actions).is_some());
    for fault in 0..9 {
        let mut c = original.clone();
        match fault {
            0 => c.source_invalid = true,
            1 => c.source_cleanup = None,
            2 => c.source_cleanup.as_mut().unwrap().0.owner = None,
            3 => c.source_cleanup.as_mut().unwrap().0.scope_depth += 1,
            4 => c.source_cleanup.as_mut().unwrap().1.identity = Arc::new(()),
            5 => c.source_cleanup.as_mut().unwrap().1.index = c.end.as_ref().unwrap().index,
            6 => c.expected.source_owner.owner = None,
            7 => c.expected.source_owner.symbol = c.expected.scope.binding,
            _ => c.expected.source_owner.scope_depth = 0,
        }
        assert!(
            c.source_retirement(&identity, &actions).is_none(),
            "fault {fault}"
        );
        assert!(
            c.finish(&identity, &actions).is_some(),
            "new failure must not erase original use set"
        );
    }
    let mut duplicate = actions.clone();
    duplicate.push(actions[original.source_cleanup.as_ref().unwrap().1.index].clone());
    assert!(original.source_retirement(&identity, &duplicate).is_none());
    assert!(original.finish(&identity, &duplicate).is_some());
}

#[test]
fn ci_only_complete_multi_region_requires_actual_auxiliary_progress_exit_and_two_drop_causes() {
    for word in ["ABBB", "BABA", "BBBA"] {
        let (original, identity, actions) = capture_word(word);
        let (source_owner, source_drop) = original.source_retirement(&identity, &actions).unwrap();
        let retirement = crate::ownership_checking::UnitLoanBindingUseSetRetirement {
            use_set: original.clone().finish(&identity, &actions).unwrap(),
            source_owner,
            source_drop,
        };
        assert!(
            original
                .complete_region(&identity, &actions, &retirement)
                .is_some()
        );
        for fault in 0..25 {
            let mut c = original.clone();
            match fault {
                0 => c.multi.invalid = true,
                1 => c.multi.exit = None,
                2 => c.multi.exit = Some(usize::MAX),
                3 => c.multi.claimed = false,
                4 => c.multi.removed = None,
                5 => c.multi.removed.as_mut().unwrap().owner = None,
                6 => c.multi.drop = None,
                7 => c.multi.drop.as_mut().unwrap().identity = Arc::new(()),
                8 => c.multi.drop.as_mut().unwrap().index += 1,
                9 => c.multi.uses.clear(),
                10 => c.multi.uses[0].actual.begin.identity = Arc::new(()),
                11 => c.multi.uses[0].actual.end.identity = Arc::new(()),
                12 => c.multi.uses[0].actual.end.index += 1,
                13 => c.multi.uses[0].returned_binding_uses += 1,
                14 => c.multi.uses[0].actual.owner = retirement.source_owner().clone(),
                15 => c.multi.owner_events.clear(),
                16 => c.multi.owner_events.swap(0, 1),
                17 => c.expected.ordered_uses.reverse(),
                18 => c.expected.auxiliary_statements.clear(),
                19 => c.expected.auxiliary_owner.owner = None,
                20 => c.expected.auxiliary_owner = c.expected.source_owner.clone(),
                21 => c.expected.completion = c.expected.scope.entry,
                22 => c.multi.uses.push(c.multi.uses[0].clone()),
                23 => {
                    c.multi.uses[0].source_state =
                        crate::ownership_checking::UnitMultiBindingSourceState::Active {
                            owner: c.expected.auxiliary_owner.owner.clone().unwrap(),
                            source_begin: retirement.use_set().source_begin(),
                            handoff: retirement.use_set().handoff(),
                        }
                }
                _ => c.multi.exit = Some(original.multi.exit.unwrap() - 1),
            }
            assert!(
                c.complete_region(&identity, &actions, &retirement)
                    .is_none(),
                "{word} fault {fault}"
            );
            assert!(
                c.source_retirement(&identity, &actions).is_some(),
                "new region failure must preserve partial retirement"
            );
            assert!(
                c.finish(&identity, &actions).is_some(),
                "new region failure must preserve partial use set"
            );
        }
        let mut foreign = retirement.clone();
        foreign.use_set.identity = Arc::new(());
        assert!(
            original
                .complete_region(&identity, &actions, &foreign)
                .is_none()
        );
        for record in [
            actions[original.multi.uses[0].begin().index].clone(),
            actions[original.multi.drop.as_ref().unwrap().index].clone(),
            actions[original.multi.uses[0].end().index].clone(),
            actions[original.returns[0].cursor].clone(),
        ] {
            let mut duplicate = actions.clone();
            duplicate.push(record);
            assert!(
                original
                    .complete_region(&identity, &duplicate, &retirement)
                    .is_none()
            );
        }
    }
}
