//! Invalid evidence tests are CI-only; the ordinary fixture test injects no faults.
use super::*;
use crate::ownership_checking::{UnitDropPoint, compilation_unit::formed_tests::Fixture};

const SOURCE: &str = "fun view(source: String, aux: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner, owner); consume(item) }";

thread_local! {
    // Opt-in read-only capture of the real validator input, before publication.
    // Final owner observations can be Unsupported and intentionally clear definitions.
    static RECORDED: std::cell::RefCell<Option<Vec<Candidate>>> = const {
        std::cell::RefCell::new(None)
    };
}

pub(super) fn record_candidate(candidate: &Candidate) {
    RECORDED.with_borrow_mut(|recorded| {
        if let Some(recorded) = recorded {
            recorded.push(candidate.clone());
        }
    });
}

fn recorded_with(
    text: &str,
    source_index: usize,
) -> (
    Candidate,
    Arc<()>,
    Vec<UnitLoanAction>,
    Vec<UnitLoanAuxiliaryRetirement>,
) {
    let fixture = Fixture::new(&[("binding-coverage.ko", text)]);
    RECORDED.with_borrow_mut(|recorded| *recorded = Some(Vec::new()));
    let shared = fixture.shared();
    let mut recorded = RECORDED.with_borrow_mut(|recorded| recorded.take().unwrap());
    assert_eq!(recorded.len(), 1, "one actual auxiliary validator input");
    let candidate = recorded.pop().unwrap();
    let observation = &shared.loan_action_observations()[0];
    let region = &observation.auxiliary_binding_regions()[0];
    assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
    assert_eq!(candidate.scope.callable, region.callable());
    assert_eq!(candidate.scope.root, region.root());
    let expected = candidate.expected.as_ref().unwrap();
    assert_eq!(expected.source_index, source_index);
    assert!(
        expected.owner.owner.is_some(),
        "actual entry owner was formed"
    );
    assert!(candidate.removed_owner.as_ref() == Some(&expected.owner));
    (
        candidate,
        observation.identity.clone(),
        observation.actions().to_vec(),
        observation.auxiliary_retirements().to_vec(),
    )
}

fn recorded() -> (
    Candidate,
    Arc<()>,
    Vec<UnitLoanAction>,
    Vec<UnitLoanAuxiliaryRetirement>,
) {
    recorded_with(SOURCE, 0)
}

#[test]
fn ordinary_auxiliary_binding_fixture_preserves_actual_entry_owner() {
    for (text, source_index) in [
        (SOURCE.to_owned(), 0),
        (
            SOURCE.replace(
                "view(source: String, aux: String)",
                "view(aux: String, source: String)",
            ),
            1,
        ),
    ] {
        let (candidate, identity, actions, relations) = recorded_with(&text, source_index);
        assert_eq!(
            candidate.coverage(&identity, &actions, &relations),
            UnitLoanActionCoverage::Complete
        );
    }
}

#[test]
fn ci_only_auxiliary_binding_coverage_rejects_missing_foreign_duplicate_and_wrong_drop_edges() {
    let (original, identity, actions, auxiliaries) = recorded();
    assert_eq!(
        original.coverage(&identity, &actions, &auxiliaries),
        UnitLoanActionCoverage::Complete
    );
    for fault in 0..9 {
        let mut candidate = original.clone();
        match fault {
            0 => candidate.drop_link = None,
            1 => candidate.drop_link.as_mut().unwrap().0.identity = Arc::new(()),
            2 => candidate.drop_link.as_mut().unwrap().1.identity = Arc::new(()),
            3 => candidate.drop_link.as_mut().unwrap().0.index = candidate.start + 4,
            4 => candidate.drop_link.as_mut().unwrap().1.index = candidate.start + 5,
            5 => candidate.ambiguous = true,
            6 => candidate.removed_owner = None,
            7 => candidate.removed_owner.as_mut().unwrap().scope_depth += 1,
            _ => candidate.removed_owner.as_mut().unwrap().symbol = candidate.scope.binding,
        }
        assert_eq!(
            candidate.coverage(&identity, &actions, &auxiliaries),
            UnitLoanActionCoverage::Partial
        );
    }
}

#[test]
fn ci_only_auxiliary_binding_coverage_rejects_incomplete_boundaries_and_unfiltered_extra_actions() {
    let (original, identity, original_actions, auxiliaries) = recorded();
    for fault in 0..6 {
        let mut candidate = original.clone();
        let mut actions = original_actions.clone();
        match fault {
            0 => candidate.end = None,
            1 => candidate.exit_empty = false,
            2 => candidate.start += 1,
            3 => {
                let extra = actions[candidate.start + 2].clone();
                actions.insert(candidate.start + 2, extra);
                *candidate.end.as_mut().unwrap() += 1;
            }
            4 => {
                let UnitLoanAction::Cleanup { point, .. } = &mut actions[candidate.start + 7]
                else {
                    unreachable!()
                };
                *point = UnitDropPoint::AfterStatement(candidate.scope.completion);
            }
            _ => candidate.scope.root = candidate.scope.binding,
        }
        assert_eq!(
            candidate.coverage(&identity, &actions, &auxiliaries),
            UnitLoanActionCoverage::Partial
        );
    }
}

#[test]
fn ci_only_auxiliary_binding_coverage_rejects_missing_foreign_duplicate_or_mismatched_relation() {
    let (candidate, identity, actions, original) = recorded();
    for fault in 0..6 {
        let mut relations = original.clone();
        match fault {
            0 => relations.clear(),
            1 => relations[0].identity = Arc::new(()),
            2 => relations.push(relations[0].clone()),
            3 => relations[0].source_begin += 1,
            4 => relations[0].auxiliary_begin += 1,
            _ => relations[0].handoff += 1,
        }
        assert_eq!(
            candidate.coverage(&identity, &actions, &relations),
            UnitLoanActionCoverage::Partial
        );
    }
}

#[test]
fn ci_only_auxiliary_binding_coverage_rejects_removed_and_reordered_records() {
    let (candidate, identity, original, auxiliaries) = recorded();
    for position in 0..8 {
        let mut removed = original.clone();
        removed.remove(candidate.start + position);
        assert_eq!(
            candidate.coverage(&identity, &removed, &auxiliaries),
            UnitLoanActionCoverage::Partial
        );
    }
    let mut swapped = original.clone();
    swapped.swap(candidate.start, candidate.start + 1);
    assert_eq!(
        candidate.coverage(&identity, &swapped, &auxiliaries),
        UnitLoanActionCoverage::Partial
    );
    let mut missing = candidate;
    missing.expected = None;
    assert_eq!(
        missing.coverage(&identity, &original, &auxiliaries),
        UnitLoanActionCoverage::Unsupported
    );
}

#[test]
fn ci_only_auxiliary_binding_coverage_rejects_absent_expected_owner_identity() {
    let (mut candidate, identity, actions, auxiliaries) = recorded();
    assert_eq!(
        candidate.coverage(&identity, &actions, &auxiliaries),
        UnitLoanActionCoverage::Complete
    );
    candidate.expected.as_mut().unwrap().owner.owner = None;
    // Matching missing snapshots must not stand in for an actual formed owner.
    candidate.removed_owner.as_mut().unwrap().owner = None;
    assert_eq!(
        candidate.coverage(&identity, &actions, &auxiliaries),
        UnitLoanActionCoverage::Partial
    );
}

#[test]
fn ci_only_source_second_rejects_wrong_chronology_and_source_edges() {
    let text = SOURCE.replace(
        "view(source: String, aux: String)",
        "view(aux: String, source: String)",
    );
    let (candidate, identity, actions, relations) = recorded_with(&text, 1);
    assert_eq!(
        candidate.coverage(&identity, &actions, &relations),
        UnitLoanActionCoverage::Complete
    );
    for fault in 0..6 {
        let mut candidate = candidate.clone();
        let mut actions = actions.clone();
        let mut relations = relations.clone();
        match fault {
            0 => candidate.expected.as_mut().unwrap().source_index = 0,
            1 => candidate.expected.as_mut().unwrap().source_index = 2,
            2 => actions.swap(candidate.start, candidate.start + 1),
            3 => relations[0].source_begin = candidate.start,
            4 => relations[0].auxiliary_begin = candidate.start + 1,
            _ => {
                let expected = candidate.expected.as_mut().unwrap();
                std::mem::swap(&mut expected.source, &mut expected.auxiliary);
            }
        }
        assert_eq!(
            candidate.coverage(&identity, &actions, &relations),
            UnitLoanActionCoverage::Partial,
            "chronology fault {fault}"
        );
    }
}
