//! Actual reservation registration and normal retirement, not an activation certificate.
use super::*;
use lang_frontend::ownership_checking::{
    UnitLoanAction, UnitLoanActionCoverage, UnitReceiverOwnershipKind,
};

const CELL: &str = "class Cell(var value:Int) { fun read():Int=value; inout fun set(own next:Int):Unit {value=next}; inout fun touch(own next:Unit):Unit {} }\n";

#[test]
fn exclusive_normal_retirement_preserves_original_shared_projection() {
    let observed = observe(&format!(
        "{CELL}fun run(inout cell:Cell):Unit {{cell.set(cell.read())}}"
    ));
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let exclusive = unit
            .receiver_facts()
            .iter()
            .find(|fact| fact.kind() == UnitReceiverOwnershipKind::ExclusiveLoan)
            .unwrap();
        assert!(exclusive.is_receiver_reservation());
        assert_eq!(exclusive.activation_point(), Some(exclusive.call()));
        let catalog = &unit.loan_action_observations()[0];
        assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
        let actions = catalog.actions();
        let ends = actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| match action {
                UnitLoanAction::ReceiverEnd(end) if end.receiver() == exclusive => Some(index),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ends.len(),
            1,
            "actual Exclusive pending retirement must not disappear"
        );
        let shared = &unit.receiver_call_return_ends()[0];
        assert_eq!(
            unit.receiver_call_return_ends().len(),
            1,
            "old projection remains Shared-only"
        );
        assert_eq!(
            shared.receiver().kind(),
            UnitReceiverOwnershipKind::SharedLoan
        );
        let nested_end = actions
            .iter()
            .position(|action| matches!(action, UnitLoanAction::ReceiverEnd(end) if end == shared))
            .unwrap();
        assert!(
            nested_end < ends[0],
            "nested shared call returns before outer exclusive retirement"
        );
        assert!(
            unit.drops().is_empty(),
            "Inout parent and copyable locals do not create an owner drop"
        );
    }
}

#[test]
fn exclusive_reserve_precedes_nested_shared_call_without_certifying_activation() {
    let observed = observe(&format!(
        "{CELL}fun run(inout cell:Cell):Unit {{cell.set(cell.read()); ((cell)).set(2)}}"
    ));
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let catalog = &unit.loan_action_observations()[0];
        let actions = catalog.actions();
        let reserves = actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| match action {
                UnitLoanAction::ReceiverReserve(receiver) => Some((index, receiver)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(reserves.len(), 2);
        assert_ne!(reserves[0].1.call(), reserves[1].1.call());
        assert_eq!(reserves[0].1.target(), reserves[1].1.target());
        let shared_begin = actions
            .iter()
            .position(|action| matches!(action, UnitLoanAction::ReceiverBegin(_)))
            .unwrap();
        assert!(reserves[0].0 < shared_begin && shared_begin < reserves[1].0);
        for (index, reserve) in reserves {
            let ends = actions
                .iter()
                .enumerate()
                .filter_map(|(end_index, action)| match action {
                    UnitLoanAction::ReceiverEnd(end) if end.receiver() == reserve => {
                        Some(end_index)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(ends.len(), 1);
            assert!(index < ends[0]);
        }
        assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
        assert!(
            catalog
                .regions()
                .iter()
                .all(|region| region.coverage() != UnitLoanActionCoverage::Complete)
        );
        assert!(unit.drops().is_empty());
    }
}

#[test]
fn early_return_reservation_has_no_activation_or_normal_end() {
    use lang_frontend::ownership_checking::UnitIterationCleanupAction;
    let observed = observe(&format!(
        "{CELL}fun run(inout cell:Cell):Unit {{cell.touch(return)}}"
    ));
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let actions = unit.loan_action_observations()[0].actions();
        let reserves = actions
            .iter()
            .filter_map(|action| match action {
                UnitLoanAction::ReceiverReserve(receiver) => Some(receiver),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [reserve] = reserves.as_slice() else {
            panic!("one evaluated receiver reservation");
        };
        assert_eq!(reserve.activation_point(), None);
        assert!(!actions.iter().any(|action| matches!(action, UnitLoanAction::ReceiverEnd(end) if end.receiver() == *reserve)));
        assert_eq!(actions.iter().filter(|action| matches!(action,
            UnitLoanAction::Cleanup { action: UnitIterationCleanupAction::EndReceiverLoan(receiver), .. } if receiver == *reserve
        )).count(), 1, "original prefix cleanup remains sole retirement");
        assert!(unit.receiver_call_return_ends().is_empty());
    }
}

#[test]
fn owned_root_normal_retirement_precedes_original_drop() {
    assert_owned_receiver_retirement("fun run():Unit {var cell=Cell(1);cell.set(2)}");
}

#[test]
fn owned_parameter_normal_retirement_precedes_original_drop() {
    assert_owned_receiver_retirement("fun run(own cell:Cell):Unit {cell.set(2)}");
}

fn assert_owned_receiver_retirement(body: &str) {
    let observed = observe(&format!("{CELL}{body}"));
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let actions = unit.loan_action_observations()[0].actions();
        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(action, UnitLoanAction::ReceiverReserve(_)))
                .count(),
            1
        );
        let reserve = actions
            .iter()
            .find_map(|action| match action {
                UnitLoanAction::ReceiverReserve(receiver) => Some(receiver),
                _ => None,
            })
            .unwrap();
        let ends = actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| match action {
                UnitLoanAction::ReceiverEnd(end) if end.receiver() == reserve => Some(index),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ends.len(),
            1,
            "actual owned-root pending retirement must not disappear"
        );
        let drop_index = actions
            .iter()
            .position(|action| {
                matches!(
                    action,
                    UnitLoanAction::Cleanup {
                        action: lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(
                            _
                        ),
                        ..
                    }
                )
            })
            .unwrap();
        assert!(
            ends[0] < drop_index,
            "actual receiver retirement precedes original owner drop"
        );
        assert_eq!(unit.drops().len(), 1);
        assert_eq!(actions.iter().filter(|action| matches!(action,
            UnitLoanAction::Cleanup { action: lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(drop), .. } if drop == &unit.drops()[0]
        )).count(), 1, "original owner Drop has one action-source record");
        let UnitLoanAction::Cleanup {
            action: lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(drop),
            ..
        } = &actions[drop_index]
        else {
            unreachable!()
        };
        assert_eq!(
            drop,
            &unit.drops()[0],
            "preserve the complete original Drop fact"
        );
        assert!(unit.receiver_call_return_ends().is_empty());
        assert_eq!(
            unit.loan_action_observations()[0].coverage(),
            UnitLoanActionCoverage::Partial
        );
    }
}

#[test]
fn invalid_or_deferred_neighbor_revokes_exclusive_actions() {
    for (bad, deferred) in [
        (
            "fun take(own text:String):Unit {}\nfun bad():Unit {val text=\"x\";take(text);take(text)}",
            false,
        ),
        (
            "fun bad():Unit {var callback:()->Int={1};val old=replace(&callback,{2})}",
            true,
        ),
    ] {
        let observed = observe(&format!(
            "{CELL}fun run(inout cell:Cell):Unit {{cell.set(2)}}\nfun owned():Unit {{var cell=Cell(1);cell.set(2)}}\n{bad}"
        ));
        for unit in &observed.units {
            if deferred {
                assert!(!unit.deferred().is_empty());
            } else {
                assert!(!unit.diagnostics().is_empty());
            }
            assert!(unit.loan_action_observations().is_empty());
            if !deferred {
                assert!(unit.receiver_facts().is_empty());
            } // Deferred retains old diagnostic facts, never the new action catalog.
            assert!(unit.receiver_call_return_ends().is_empty());
        }
    }
}

#[cfg(feature = "loan-observation-test-support")]
#[test]
fn observation_switch_preserves_old_receiver_products() {
    use lang_frontend::ownership_checking::{legacy_ownership_eq, with_loan_observation};
    let text = format!(
        "{CELL}fun run(inout cell:Cell):Unit {{cell.set(cell.read());cell.read()}}\nfun owned():Unit {{var cell=Cell(1);cell.set(cell.read());((cell)).set(2)}}\nfun parameter(own cell:Cell):Unit {{cell.set(2)}}"
    );
    let baseline = observe(&text);
    for index in 0..2 {
        let (env, types) = standard_environments();
        let input = if index == 0 {
            SourceUnitInput::new(
                "root",
                "call_ends.ko",
                baseline.parsed.source_id(),
                &baseline.parsed,
            )
        } else {
            SourceUnitInput::memory_document(baseline.parsed.source_id(), &baseline.parsed)
        };
        let inputs = [input];
        let index = index_compilation_unit(&baseline.sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&baseline.sources, &inputs, &index, &env)
            .unwrap()
            .validate()
            .unwrap();
        let raw = check_compilation_unit_types(&baseline.sources, &inputs, &names, &types).unwrap();
        complete_compilation_unit_types(&baseline.sources, &inputs, &names, &types, &raw).unwrap();
        let typed = raw.validate().unwrap();
        let analyze = |enabled| {
            with_loan_observation(enabled, || {
                check_compilation_unit_ownership(&baseline.sources, &inputs, &names, &types, &typed)
                    .unwrap()
            })
        };
        let on = analyze(true);
        let off = analyze(false);
        assert!(on.diagnostics().is_empty() && on.deferred().is_empty());
        assert!(off.diagnostics().is_empty() && off.deferred().is_empty());
        assert!(legacy_ownership_eq(&on, &off));
        assert_eq!(
            on.receiver_call_return_ends(),
            off.receiver_call_return_ends()
        );
        assert!(off.loan_action_observations().is_empty());
    }
}

#[test]
fn projected_inout_receiver_stays_outside_normal_return_bridge() {
    let observed = observe(&format!(
        "{CELL}class Holder(var cell:Cell)\nfun run(inout holder:Holder):Unit {{holder.cell.set(2)}}"
    ));
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let actions = unit.loan_action_observations()[0].actions();
        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(action, UnitLoanAction::ReceiverReserve(_)))
                .count(),
            1
        );
        assert!(!actions.iter().any(|action| matches!(action, UnitLoanAction::ReceiverEnd(end) if end.receiver().kind() == UnitReceiverOwnershipKind::ExclusiveLoan)));
    }
}

#[test]
fn owned_nested_shared_and_sequential_reservations_keep_actual_identity() {
    let observed = observe(&format!(
        "{CELL}fun run():Unit {{var cell=Cell(1);cell.set(cell.read());((cell)).set(2)}}"
    ));
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let catalog = &unit.loan_action_observations()[0];
        let actions = catalog.actions();
        let reserves = actions
            .iter()
            .enumerate()
            .filter_map(|(i, action)| match action {
                UnitLoanAction::ReceiverReserve(receiver) => Some((i, receiver)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(reserves.len(), 2);
        assert_ne!(reserves[0].1.call(), reserves[1].1.call());
        assert_eq!(reserves[0].1.target(), reserves[1].1.target());
        assert_eq!(unit.receiver_call_return_ends().len(), 1);
        let shared_end = actions
            .iter()
            .position(|action| {
                matches!(action,
                    UnitLoanAction::ReceiverEnd(end) if end == &unit.receiver_call_return_ends()[0]
                )
            })
            .unwrap();
        let mut end_indices = Vec::new();
        for (begin, receiver) in &reserves {
            let ends = actions
                .iter()
                .enumerate()
                .filter_map(|(i, action)| match action {
                    UnitLoanAction::ReceiverEnd(end) if end.receiver() == *receiver => Some(i),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(ends.len(), 1);
            assert!(*begin < ends[0]);
            end_indices.push(ends[0]);
        }
        assert!(reserves[0].0 < shared_end && shared_end < end_indices[0]);
        assert!(end_indices[0] < reserves[1].0);
        assert_eq!(unit.drops().len(), 1);
        assert_eq!(actions.iter().filter(|action| matches!(action,
            UnitLoanAction::Cleanup { action: lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(drop), .. } if drop == &unit.drops()[0]
        )).count(), 1, "original owner Drop has one action-source record");
        let drop_index = actions.iter().position(|action| matches!(action,
            UnitLoanAction::Cleanup { action: lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(drop), .. } if drop == &unit.drops()[0]
        )).unwrap();
        assert!(end_indices[1] < drop_index);
        assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
        assert!(
            catalog
                .regions()
                .iter()
                .all(|r| r.coverage() != UnitLoanActionCoverage::Complete)
        );
    }
}
