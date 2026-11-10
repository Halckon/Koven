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

#[test]
fn receiver_region_owned_parameter_requires_actual_begin_end_and_drop() {
    use lang_frontend::ownership_checking::{UnitLoanActionCoverage, UnitReceiverOwnershipTarget};
    let observed = observe(
        "class Node { fun touch(): Int = 1 }\nfun run(own node: Node): Unit { node.touch() }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let [end] = unit.receiver_call_return_ends() else {
            panic!("existing real receiver retirement")
        };
        let UnitReceiverOwnershipTarget::Place(place) = end.receiver().target() else {
            panic!("owned parameter place")
        };
        assert!(place.is_root());
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| {
                drop.point() == end.point() && drop.target() == UnitDropTarget::Named(place.root())
            })
            .collect();
        assert_eq!(
            drops.len(),
            1,
            "existing actual root Drop, not manufactured from receiver End"
        );
        let observation = &unit.loan_action_observations()[0];
        let region = observation
            .regions()
            .iter()
            .find(|region| region.call() == end.receiver().call())
            .unwrap();
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert_eq!(region.protocol_version(), 2);
        assert_eq!(region.action_ids().len(), 3);
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Partial);
    }
}

#[test]
fn receiver_region_live_root_is_partial_until_actual_last_use_drop() {
    use lang_frontend::ownership_checking::UnitLoanActionCoverage::{Complete, Partial};
    let observed = observe(
        "class Node { fun touch(): Int = 1 }\nfun run(own node: Node): Unit { node.touch(); node.touch() }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let observation = &unit.loan_action_observations()[0];
        let regions = observation.regions();
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].coverage(), Partial);
        assert_eq!(regions[0].action_ids().len(), 2);
        assert_eq!(regions[1].coverage(), Complete);
        assert_eq!(regions[1].action_ids().len(), 3);
        assert_eq!(unit.receiver_call_return_ends().len(), 2);
        assert_eq!(observation.coverage(), Partial);
    }
}

#[test]
fn receiver_region_two_owned_roots_keep_distinct_formation_origins() {
    use lang_frontend::ownership_checking::{
        UnitLoanAction, UnitLoanActionCoverage, UnitLoanRegionFamily,
    };
    let observed = observe(
        "class Node { fun touch(): Int = 1 }\nfun run(own first: Node, own second: Node): Unit { first.touch(); second.touch() }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let observation = &unit.loan_action_observations()[0];
        assert_eq!(observation.regions().len(), 2);
        let mut origins = Vec::new();
        for region in observation.regions() {
            assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
            assert_eq!(
                region.families(),
                &[
                    UnitLoanRegionFamily::SharedReceiver,
                    UnitLoanRegionFamily::NamedSourceDrop
                ]
            );
            let actions: Vec<_> = region
                .action_ids()
                .map(|id| observation.action(&id).unwrap())
                .collect();
            let [
                UnitLoanAction::ReceiverBegin(begin),
                UnitLoanAction::ReceiverEnd(end),
                UnitLoanAction::Cleanup {
                    action:
                        lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(drop),
                    ..
                },
            ] = actions.as_slice()
            else {
                panic!("entire interval");
            };
            assert_eq!(begin, end.receiver());
            assert_ne!(drop.value_origin(), begin.begin_span());
            origins.push(drop.value_origin());
        }
        assert_ne!(origins[0], origins[1]);
    }
}

#[test]
fn receiver_region_unadmitted_roots_and_reservations_are_not_complete() {
    use lang_frontend::ownership_checking::UnitLoanActionCoverage;
    // Ordinary finite source programs only, no corrupted products or Abort calls.
    for source in [
        "class Node { fun touch(): Int = 1 }\nfun run(node: Node): Unit { node.touch() }",
        "class Node { fun touch(): Int = 1 }\nfun run(inout node: Node): Unit { node.touch() }",
        "class Node { fun touch(): Int = 1 }\nfun run(own node: Node): Unit { ((node)).touch() }",
        "class Node { fun touch(): Int = 1 }\nfun run(): Unit { Node().touch() }",
        "class Node { inout fun touch(): Int = 1 }\nfun run(inout node: Node): Unit { node.touch() }",
        "class Node { fun touch(): Int = 1; deinit() {} }\nfun run(own node: Node): Unit { node.touch() }",
        "class Node { fun touch(): Int = 1 }\nclass Holder(val node: Node)\nfun run(own holder: Holder): Unit { holder.node.touch() }",
        "class Node { fun touch(): Int = 1; fun outer(): Unit { this.touch() } }",
    ] {
        let observed = observe(source);
        assert_units_admitted(&observed);
        for unit in &observed.units {
            let observation = &unit.loan_action_observations()[0];
            assert!(!observation.regions().is_empty());
            assert!(
                observation
                    .regions()
                    .iter()
                    .all(|region| region.coverage() != UnitLoanActionCoverage::Complete),
                "{source}"
            );
            assert_ne!(observation.coverage(), UnitLoanActionCoverage::Complete);
        }
    }
}

#[test]
fn receiver_borrow_result_keeps_existing_rejection_and_no_certificate() {
    let observed = observe(
        "class Node { fun view(): borrow Node from this = this }\nfun run(own node: Node): Unit { node.view() }",
    );
    for unit in &observed.units {
        assert_eq!(unit.diagnostics().len(), 2);
        for (diagnostic, (start, end, message)) in unit.diagnostics().iter().zip([
            (
                25,
                31,
                "borrow result source requires an unsupported receiver or inout continuation",
            ),
            (
                88,
                99,
                "caller borrow result continuation is not yet proven",
            ),
        ]) {
            assert_eq!(diagnostic.code().to_string(), "L0164");
            assert_eq!(diagnostic.message(), message);
            let span = diagnostic.primary_span();
            assert_eq!(
                (span.source_id(), span.start(), span.end()),
                (observed.parsed.source_id(), start, end)
            );
        }
        assert!(unit.loan_action_observations().is_empty());
        assert!(unit.receiver_call_return_ends().is_empty());
    }
}

#[test]
fn receiver_region_record_and_certificate_identity_cannot_cross_reanalysis() {
    use lang_frontend::ownership_checking::UnitLoanActionCoverage;
    let source =
        "class Node { fun touch(): Int = 1 }\nfun run(own node: Node): Unit { node.touch() }";
    let left = observe(source);
    let right = observe(source);
    assert_units_admitted(&left);
    assert_units_admitted(&right);
    for (left, right) in left.units.iter().zip(&right.units) {
        let left = &left.loan_action_observations()[0];
        let right = &right.loan_action_observations()[0];
        let region = &left.regions()[0];
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert!(left.owns_region(region));
        assert!(!right.owns_region(region));
        for id in region.action_ids() {
            assert!(right.action(&id).is_none());
        }
    }
}
