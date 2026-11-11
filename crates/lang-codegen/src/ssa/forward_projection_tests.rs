//! Ordinary frontend-only neighbors. Block returns are not forced through unsupported lowering.
use super::*;
use lang_frontend::ownership_checking::{
    UnitDropPoint, UnitIterationCleanupAction, check_compilation_unit_ownership,
    legacy_ownership_eq, with_loan_observation,
};

const VIEW: &str = "fun view(source: String): borrow String from source = source\n";

#[test]
fn original_block_and_group_forward_facts_survive_true_off() {
    for (body, point) in [
        ("= ((view((source))))", "((view((source))))"),
        ("{ return view(source) }", "return view(source)"),
        ("{ return ((view((source)))) }", "return ((view((source))))"),
    ] {
        check_frontend_only(
            &format!("{VIEW}fun wrap(source: String): borrow String from source {body}"),
            &[point],
        );
    }
}

#[test]
fn original_branch_returns_keep_both_raw_handoffs_without_a_cause() {
    check_frontend_only(
        &format!(
            "{VIEW}fun wrap(source: String, flag: Boolean): borrow String from source {{ if (flag) {{ return view(source) }}; return ((view(source))) }}"
        ),
        &["return view(source)", "return ((view(source)))"],
    );
}

fn check_frontend_only(text: &str, expected_points: &[&str]) {
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "forward-neighbor.ko", text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "forward-neighbor.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (names, typed, on) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let off = with_loan_observation(false, || {
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .unwrap()
        });
        assert!(legacy_ownership_eq(on.ownership(), off.ownership()));
        assert!(off.ownership().loan_action_observations().is_empty());
        assert_eq!(
            on.ownership().owner_observations(),
            off.ownership().owner_observations()
        );
        let [catalog] = on.ownership().loan_action_observations() else {
            panic!("source catalog")
        };
        assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
        let handoffs = catalog
            .actions()
            .iter()
            .filter_map(|action| {
                if let UnitLoanAction::BorrowForwardHandoff(handoff) = action {
                    Some(handoff)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let facts = on.ownership().borrow_forward_transports();
        assert_eq!(facts.len(), expected_points.len());
        assert_eq!(
            handoffs
                .iter()
                .map(|handoff| handoff.transaction())
                .collect::<Vec<_>>(),
            facts.iter().collect::<Vec<_>>()
        );
        assert!(
            handoffs
                .iter()
                .all(|handoff| handoff.source_begin().is_none() && handoff.callable().is_none())
        );
        for owned in [&on, &off] {
            assert!(
                owned.ownership().drops().is_empty(),
                "borrowed parameters do not own payloads"
            );
            for (fact, expected_point) in owned
                .ownership()
                .borrow_forward_transports()
                .iter()
                .zip(expected_points)
            {
                let loan = fact.source_loan();
                assert!(owned.ownership().loans().contains(loan));
                assert!(
                    !owned
                        .ownership()
                        .direct_call_loan_ends()
                        .iter()
                        .any(|end| end.loan() == loan)
                );
                let UnitDropPoint::ControlTransfer(point) = fact.point() else {
                    panic!("real wrapper transfer")
                };
                assert_eq!(
                    point.source_unit(),
                    names.names().index().source_units()[0].id()
                );
                let span = parsed
                    .ast()
                    .expressions()
                    .get(point.expression())
                    .unwrap()
                    .span();
                assert_eq!(span.source_id(), source);
                assert_eq!(sources.slice(span).unwrap(), *expected_point);
            }
        }
        for fact in facts {
            assert_eq!(catalog.actions().iter().filter(|action| matches!(action, UnitLoanAction::Begin(loan) if loan == fact.source_loan())).count(), 1);
            assert!(!catalog.actions().iter().any(
                |action| matches!(action, UnitLoanAction::Cleanup {
                action: UnitIterationCleanupAction::EndCallLoan(loan), ..
            } if loan == fact.source_loan())
            ));
        }
        if let [first, second] = facts {
            assert_eq!(first.source_loan().target(), second.source_loan().target());
            assert_ne!(first.source_loan().call(), second.source_loan().call());
            assert_ne!(
                first.source_loan().argument(),
                second.source_loan().argument()
            );
            assert_ne!(first.point(), second.point());
        }
    }
}
