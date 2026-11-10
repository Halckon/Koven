use super::*;
use lang_frontend::ownership_checking::UnitLoanActionId;

#[test]
fn actual_action_source_preserves_acquisition_before_cleanup() {
    use lang_frontend::ownership_checking::{
        UnitIterationCleanupAction, UnitLoanAction, UnitLoanActionCoverage,
    };
    let observed =
        observe("fun inspect(value: String): Int = 1\nfun run(): Unit { inspect(\"temporary\") }");
    for unit in &observed.units {
        assert!(unit.diagnostics().is_empty());
        assert!(unit.deferred().is_empty());
        let observations = unit.loan_action_observations();
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].coverage(), UnitLoanActionCoverage::Partial);
        let actions = observations[0].actions();
        let begins: Vec<_> = actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| {
                if let UnitLoanAction::Begin(loan) = action {
                    Some((index, loan))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            begins.len(),
            1,
            "actual pending acquisition must be present, not inferred from Ends"
        );
        let (begin_index, loan) = begins[0];
        let end_index = actions.iter().position(|action| matches!(action,
            UnitLoanAction::Cleanup { action: UnitIterationCleanupAction::EndCallLoan(ended), .. } if ended == loan
        )).unwrap();
        let drop_index = actions
            .iter()
            .position(|action| {
                matches!(
                    action,
                    UnitLoanAction::Cleanup {
                        action: UnitIterationCleanupAction::Drop(_),
                        ..
                    }
                )
            })
            .unwrap();
        assert!(begin_index < end_index && end_index < drop_index);
        assert_eq!(unit.direct_call_loan_ends()[0].loan(), loan);
    }
}

#[test]
fn empty_observation_is_unsupported_not_complete_absence() {
    use lang_frontend::ownership_checking::UnitLoanActionCoverage;
    let observed = observe("fun run(): Int = 1");
    assert_units_admitted(&observed);
    for (unit, names) in observed.units.iter().zip(&observed.unit_names) {
        let [observation] = unit.loan_action_observations() else {
            panic!("one source")
        };
        assert_eq!(
            observation.source(),
            names.names().index().source_units()[0].id()
        );
        assert!(observation.actions().is_empty());
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Unsupported);
    }
}

#[test]
fn result_handoff_and_receiver_families_stay_partial() {
    use lang_frontend::ownership_checking::UnitLoanActionCoverage;
    for text in [
        "fun select(source: String): borrow String from source = source\nfun run(): Unit { val source = \"kept\"; borrow val item = select(source); println(item) }",
        "class Node { fun touch(text: String): Int = 1 }\nfun run(): Unit { val node = Node(); node.touch(\"text\") }",
    ] {
        let observed = observe(text);
        assert_units_admitted(&observed);
        for unit in &observed.units {
            assert!(
                !unit.borrow_call_handoffs().is_empty()
                    || !unit.receiver_call_return_ends().is_empty()
            );
            assert_eq!(
                unit.loan_action_observations()[0].coverage(),
                UnitLoanActionCoverage::Partial
            );
        }
    }
}

#[test]
fn same_target_arguments_keep_distinct_actual_acquisitions() {
    use lang_frontend::ownership_checking::UnitLoanAction;
    let observed = observe(
        "fun inspect(a: String, b: String): Int = 1\nfun run(): Unit { val source = \"kept\"; inspect(b = source, a = source) }",
    );
    assert_units_admitted(&observed);
    let mut previous_id = None;
    for unit in &observed.units {
        let observation = &unit.loan_action_observations()[0];
        let begins: Vec<_> = observation
            .actions()
            .iter()
            .filter_map(|action| {
                if let UnitLoanAction::Begin(loan) = action {
                    Some(loan)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(begins.len(), 2);
        assert_eq!(begins[0].target(), begins[1].target());
        assert_ne!(begins[0].argument(), begins[1].argument());
        assert_eq!(begins[0].call(), begins[1].call());
        assert_eq!(begins[0].call().source_unit(), observation.source());
        let id = observation.action_id(0).unwrap();
        assert_eq!(observation.action(&id), observation.actions().first());
        if let Some(previous) = previous_id {
            assert_ne!(previous, id);
            assert!(observation.action(&previous).is_none());
        }
        previous_id = Some(id);
    }
}

#[test]
fn reanalysis_cannot_resolve_previous_catalog_record() {
    let text = "fun inspect(value: String): Int = 1\nfun run(): Unit { inspect(\"temporary\") }";
    let first = observe(text);
    let second = observe(text);
    for (left, right) in first.units.iter().zip(&second.units) {
        let left = &left.loan_action_observations()[0];
        let right = &right.loan_action_observations()[0];
        assert_eq!(left.source(), right.source()); // Same index is not same catalog.
        let id: UnitLoanActionId = left.action_id(0).unwrap();
        assert_eq!(&left.clone(), left); // Cloning preserves the catalog and full snapshot.
        assert_eq!(left.clone().action(&id), left.action(&id));
        assert!(left.action_id(left.actions().len()).is_none());
        assert!(right.action(&id).is_none());
        assert_ne!(id, right.action_id(0).unwrap());
    }
}

#[test]
fn nested_calls_keep_actual_acquisition_order() {
    use lang_frontend::ownership_checking::UnitLoanAction;
    let observed = observe(
        "fun number(value: String): Int = 1\nfun inspect(a: String, own n: Int): Int = n\nfun run(): Unit { inspect(\"outer\", number(\"inner\")) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let arguments: Vec<_> = unit.loan_action_observations()[0]
            .actions()
            .iter()
            .filter_map(|action| {
                if let UnitLoanAction::Begin(loan) = action {
                    let node = observed
                        .parsed
                        .ast()
                        .expressions()
                        .get(loan.argument().expression())
                        .unwrap();
                    Some(observed.sources.slice(node.span()).unwrap())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(arguments, ["\"outer\"", "\"inner\""]);
    }
}

#[test]
fn later_operand_return_does_not_invent_suffix_acquisitions() {
    use lang_frontend::ownership_checking::{UnitIterationCleanupAction, UnitLoanAction};
    let observed = observe(
        "fun inspect(a: String, own n: Int, b: String): Unit {}\nfun run(): Unit { inspect(\"prefix\", return, \"unreached\") }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let actions = unit.loan_action_observations()[0].actions();
        let begins: Vec<_> = actions
            .iter()
            .filter_map(|action| {
                if let UnitLoanAction::Begin(loan) = action {
                    Some(loan)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(begins.len(), 1);
        let node = observed
            .parsed
            .ast()
            .expressions()
            .get(begins[0].argument().expression())
            .unwrap();
        assert_eq!(observed.sources.slice(node.span()).unwrap(), "\"prefix\"");
        assert!(actions.iter().any(|action| matches!(action,
            UnitLoanAction::Cleanup { point: UnitDropPoint::ControlTransfer(_), action: UnitIterationCleanupAction::EndCallLoan(loan) } if loan == begins[0]
        )));
    }
}

#[test]
fn region_complete_normal_return_requires_actual_closed_actions() {
    use lang_frontend::ownership_checking::UnitLoanActionCoverage;
    let observed =
        observe("fun inspect(value: String): Int = 1\nfun run(): Unit { inspect(\"temporary\") }");
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let observation = &unit.loan_action_observations()[0];
        let [region] = observation.regions() else {
            panic!("one entered call")
        };
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert!(region.callable().is_some());
        assert_eq!(region.action_ids().count(), 3);
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Partial);
    }
}

#[test]
fn region_zero_arguments_requires_entered_normal_edge() {
    use lang_frontend::ownership_checking::UnitLoanActionCoverage;
    let observed = observe("fun number(): Int = 1\nfun run(): Int = number()");
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let observation = &unit.loan_action_observations()[0];
        let [region] = observation.regions() else {
            panic!("actual entered zero-argument call")
        };
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert_eq!(region.action_ids().len(), 0);
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Unsupported);
        assert_eq!(region.protocol_version(), 1);
        assert_eq!(region.families().len(), 2);
    }
    let absent = observe("fun run(): Int = 1");
    for unit in &absent.units {
        assert!(unit.loan_action_observations()[0].regions().is_empty());
    }
}

#[test]
fn region_two_named_literals_keep_reverse_end_and_drop_order() {
    use lang_frontend::ownership_checking::{UnitDirectCallLoanRegion, UnitLoanActionCoverage};
    let observed = observe(
        "fun inspect(a: String, b: String): Int = 1\nfun run(): Unit { inspect(b = \"first\", a = \"second\") }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let observation = &unit.loan_action_observations()[0];
        let region: &UnitDirectCallLoanRegion = &observation.regions()[0];
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert_eq!(region.action_ids().len(), 6);
        for id in region.action_ids() {
            assert!(observation.action(&id).is_some());
        }
    }
}

#[test]
fn region_ineligible_families_and_contexts_never_become_complete() {
    use lang_frontend::ownership_checking::UnitLoanActionCoverage;
    for text in [
        "fun inspect(value: String?): Int = 1\nfun run(): Unit { inspect(\"conversion\") }",
        "fun inspect(value: String): Int = 1\nfun run(): Unit { val named = \"kept\"; inspect(named) }",
        "fun inspect(value: String): Int = 1\nfun run(flag: Boolean): Unit { if (flag) { inspect(\"branch\") } }",
        "fun inspect(value: String): Int = 1\nfun run(flag: Boolean): Unit { while (flag) { inspect(\"loop\"); break } }",
        "fun inspect(value: String): Int = 1\nfun outer(a: String, own n: Int): Unit {}\nfun run(): Unit { outer(\"outer\", inspect(\"nested\")) }",
        "fun inspect(value: String): Int = 1\nfun run(): Unit { inspect(\"interpolated ${1}\") }",
        "class Node { fun inspect(value: String): Int = 1 }\nfun run(): Unit { Node().inspect(\"receiver\") }",
    ] {
        let observed = observe(text);
        assert_units_admitted(&observed);
        for unit in &observed.units {
            assert!(!unit.loan_action_observations()[0].regions().is_empty());
            assert!(
                unit.loan_action_observations()[0]
                    .regions()
                    .iter()
                    .all(|region| region.coverage() != UnitLoanActionCoverage::Complete),
                "{text}"
            );
        }
    }
}

#[test]
fn region_catalog_cannot_cross_reanalysis() {
    let text = "fun inspect(value: String): Int = 1\nfun run(): Unit { inspect(\"temporary\") }";
    let first = observe(text);
    let second = observe(text);
    for (left, right) in first.units.iter().zip(&second.units) {
        let left = &left.loan_action_observations()[0];
        let right = &right.loan_action_observations()[0];
        for id in left.regions()[0].action_ids() {
            assert!(left.clone().action(&id).is_some());
            assert!(right.action(&id).is_none());
        }
    }
}

#[test]
fn region_sequential_calls_use_full_record_cursors() {
    use lang_frontend::ownership_checking::UnitLoanActionCoverage;
    let observed = observe(
        "fun inspect(value: String): Int = 1\nfun run(): Unit { inspect(\"first\"); inspect(\"second\") }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let observation = &unit.loan_action_observations()[0];
        assert_eq!(observation.regions().len(), 2);
        for region in observation.regions() {
            assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
            assert_eq!(region.action_ids().len(), 3);
            for id in region.action_ids() {
                assert!(observation.action(&id).is_some());
            }
        }
        assert_ne!(
            observation.regions()[0].action_ids().next(),
            observation.regions()[1].action_ids().next()
        );
    }
}

#[test]
fn region_zero_certificate_catalog_is_checked_without_actions() {
    use lang_frontend::ownership_checking::UnitLoanActionCoverage;
    let text = "fun number(): Int = 1\nfun run(): Int = number()";
    let first = observe(text);
    let second = observe(text);
    for (left, right) in first.units.iter().zip(&second.units) {
        let left = &left.loan_action_observations()[0];
        let right = &right.loan_action_observations()[0];
        let region = &left.regions()[0];
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert_eq!(region.action_ids().len(), 0);
        assert!(left.owns_region(region));
        assert!(left.clone().owns_region(&region.clone()));
        assert!(!right.owns_region(region));
    }
}
