//! CI-only validator corruption; compile locally, never execute locally.
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
use crate::ownership_checking::{
    UnitBorrowCallHandoffFact, UnitLoanAction, UnitLoanActionCoverage,
};

const SOURCE: &str = "fun view(source: String, aux: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner, owner); consume(item) }";

fn recorded(fact: &UnitBorrowCallHandoffFact) -> ActionSource {
    let mut actions = ActionSource::default();
    let source = actions.begin(fact.source_loan().clone());
    let auxiliary = actions.begin(fact.auxiliary_loan_ends()[0].clone());
    let handoff = actions.commit_borrow_handoff(fact.clone());
    actions.register_auxiliary_retirement(handoff, source, vec![auxiliary], true);
    actions
}

#[test]
fn ci_only_auxiliary_links_reject_missing_foreign_reused_and_cross_role_tokens() {
    let fixture = Fixture::new(&[("auxiliary.ko", SOURCE)]);
    let owned = fixture.shared();
    let [fact] = owned.borrow_call_handoffs() else {
        panic!("real handoff")
    };
    let source_unit = fact.source_loan().call().source_unit();
    let file = fixture.parsed[0].source_id();
    let baseline = recorded(fact).finish(source_unit, file);
    let [relation] = baseline[0].auxiliary_retirements() else {
        panic!("valid relation")
    };
    assert_ne!(relation.source_begin(), relation.auxiliary_begin());
    let foreign = recorded(fact);
    let other = recorded(fact).finish(source_unit, file);
    assert!(other[0].action(&relation.auxiliary_begin()).is_none());
    for fault in 0..16 {
        let mut actions = recorded(fact);
        let c = &actions.auxiliary_retirements[0];
        let source = c.source.clone().unwrap();
        let auxiliary = c.auxiliaries[0].clone().unwrap();
        let handoff = c.handoff.clone();
        match fault {
            0 => actions.auxiliary_retirements[0].source = None,
            1 => actions.auxiliary_retirements[0].auxiliaries[0] = None,
            2 => actions.auxiliary_retirements[0].witnessed = false,
            3 => {
                actions.auxiliary_retirements[0].source =
                    foreign.auxiliary_retirements[0].source.clone()
            }
            4 => {
                actions.auxiliary_retirements[0].auxiliaries =
                    foreign.auxiliary_retirements[0].auxiliaries.clone()
            }
            5 => {
                actions.auxiliary_retirements[0].handoff =
                    foreign.auxiliary_retirements[0].handoff.clone()
            }
            6 => actions.auxiliary_retirements[0].source = Some(handoff.clone()),
            7 => actions.auxiliary_retirements[0].auxiliaries = vec![Some(handoff.clone())],
            8 => actions.auxiliary_retirements[0].auxiliaries = vec![Some(source.clone())],
            9 => actions.register_auxiliary_retirement(
                handoff.clone(),
                Some(source.clone()),
                vec![Some(auxiliary.clone())],
                true,
            ),
            10 => actions.register_auxiliary_retirement(
                foreign.auxiliary_retirements[0].handoff.clone(),
                Some(source.clone()),
                vec![],
                false,
            ),
            11 => actions.register_auxiliary_retirement(
                foreign.auxiliary_retirements[0].handoff.clone(),
                None,
                vec![Some(auxiliary.clone())],
                false,
            ),
            12 => actions.register_auxiliary_retirement(
                foreign.auxiliary_retirements[0].handoff.clone(),
                None,
                vec![Some(source.clone())],
                false,
            ),
            13 => actions.register_auxiliary_retirement(
                foreign.auxiliary_retirements[0].handoff.clone(),
                Some(auxiliary.clone()),
                vec![],
                false,
            ),
            14 => {
                // Same root does not permit using the source argument's Begin as the aux fact.
                actions.records[auxiliary.index] = Record::Begin(fact.source_loan().clone());
            }
            _ => {
                // Both Begin identities exist, but their original argument roles are exchanged.
                actions.auxiliary_retirements[0].source = Some(auxiliary.clone());
                actions.auxiliary_retirements[0].auxiliaries = vec![Some(source.clone())];
            }
        }
        let observed = actions.finish(source_unit, file);
        assert_eq!(observed[0].coverage(), UnitLoanActionCoverage::Partial);
        assert!(observed[0].binding_regions().is_empty());
        assert!(
            observed[0].auxiliary_retirements().is_empty(),
            "fault {fault}"
        );
        let handoffs = observed[0]
            .actions()
            .iter()
            .filter_map(|a| match a {
                UnitLoanAction::BorrowHandoffCommitted(raw) => Some(raw),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            handoffs,
            vec![fact],
            "raw retirement projection must survive rejection"
        );
    }
}
