//! Ordinary unchanged-source legality; no synthetic facts or native execution.
use super::*;
use lang_frontend::ownership_checking::{
    UnitLoanTarget, check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};

const SOURCE: &str = "fun view(source: String, aux: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner, owner); consume(item) }";

#[test]
fn ordinary_auxiliary_retirement_preserves_live_source_with_true_off() {
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "aux-handoff.ko", SOURCE);
    let (environment, types) = standard_environments();
    let mut completed = Vec::new();
    for (layout, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "aux-handoff.ko", source, &parsed),
        ),
        ("memory", SourceUnitInput::memory_document(source, &parsed)),
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
        let [catalog] = on.ownership().loan_action_observations() else {
            panic!("one source catalog")
        };
        assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
        assert!(
            catalog.binding_regions().is_empty(),
            "old no-aux binding gate"
        );
        let [handoff] = on.ownership().borrow_call_handoffs() else {
            panic!("one real handoff")
        };
        let source_loan = handoff.source_loan();
        let [auxiliary] = handoff.auxiliary_loan_ends() else {
            panic!("one retired auxiliary token")
        };
        assert_ne!(source_loan, auxiliary);
        assert_ne!(source_loan.argument(), auxiliary.argument());
        assert_eq!(source_loan.call(), auxiliary.call());
        assert_eq!(source_loan.target(), auxiliary.target());
        assert!(matches!(source_loan.target(), UnitLoanTarget::Place(place) if place.is_root()));
        assert!(!handoff.auxiliary_loan_ends().contains(source_loan));
        for loan in [source_loan, auxiliary] {
            assert_eq!(
                catalog
                    .actions()
                    .iter()
                    .filter(|action| {
                        matches!(action, UnitLoanAction::Begin(actual) if actual == loan)
                    })
                    .count(),
                1
            );
        }
        let run = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap();
        for (setting, owned) in [("on", &on), ("off", &off)] {
            let ((lowered, receipts), verification) =
                crate::ssa::unit_lower::observe_final_verifier(|| {
                    crate::ssa::refine::binding_receipt::observe(|| {
                        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                            &sources,
                            &inputs,
                            &names,
                            &types,
                            &typed,
                            owned,
                            run.id(),
                        )
                    })
                });
            println!(
                "layout={layout}; observer={setting}; lowering_ok={}; original_verification={verification:?}",
                lowered.is_ok()
            );
            completed.push((layout, setting, lowered, receipts, verification));
        }
    }
    // Collect all four real lowerings before checking returned products.
    let mut previous = None;
    for (layout, setting, lowered, receipts, verification) in completed {
        let (program, entry) = lowered.unwrap_or_else(|e| panic!("{layout}/{setting}: {e:?}"));
        assert_eq!(
            verification.len(),
            1,
            "{layout}/{setting}: original verifier once"
        );
        assert!(
            receipts.is_empty(),
            "no old binding receipt for auxiliary shape"
        );
        verify_program(&program).unwrap();
        check_actual_auxiliary(&program, entry);
        let output = (
            crate::ssa::render_program(&program),
            crate::llvm::render_verified_program(&program).unwrap(),
        );
        if setting == "on" {
            previous = Some(output);
        } else {
            assert_eq!(previous.as_ref(), Some(&output));
        }
    }
}

fn check_actual_auxiliary(program: &Program, entry: FunctionId) {
    let module = &program.modules[0];
    let caller = module.function(entry).unwrap();
    assert_eq!(caller.blocks.len(), 1);
    let calls = caller
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| matches!(i.operation, Operation::BorrowCall { .. }))
        .collect::<Vec<_>>();
    let [(call_index, call)] = calls.as_slice() else {
        panic!("one borrowed call")
    };
    let Operation::BorrowCall {
        callee,
        arguments,
        source,
    } = &call.operation
    else {
        unreachable!()
    };
    let [EntityId::Loan(actual_source), EntityId::Loan(auxiliary)] = arguments.as_slice() else {
        panic!("two actual loan operands")
    };
    assert_eq!(source, actual_source);
    assert_ne!(source, auxiliary);
    let [EntityId::Loan(result)] = call.results.as_slice() else {
        panic!("borrowed result")
    };
    assert_ne!(result, source);
    assert_ne!(result, auxiliary);
    let view = module.function(*callee).unwrap();
    assert!(view.name.contains(".view."));
    let [EntityId::Loan(view_source), EntityId::Loan(view_aux)] =
        view.blocks[0].parameters.as_slice()
    else {
        panic!("two callee parameters")
    };
    assert_ne!(view_source, view_aux);
    assert!(matches!(&view.blocks[0].terminator.as_ref().unwrap().kind,
        TerminatorKind::BorrowReturn { loan } if loan == view_source));
    let mut owners = Vec::new();
    for loan in [source, auxiliary] {
        let begins = caller
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| i.results == [EntityId::Loan(*loan)])
            .collect::<Vec<_>>();
        let [(index, begin)] = begins.as_slice() else {
            panic!("unique actual begin")
        };
        assert!(index < call_index);
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = &begin.operation
        else {
            panic!("shared root begin")
        };
        let roots = caller
            .instructions
            .iter()
            .filter(|i| i.results == [EntityId::Place(*place)])
            .collect::<Vec<_>>();
        let [root] = roots.as_slice() else {
            panic!("actual root place")
        };
        let Operation::RootPlace { owner } = &root.operation else {
            panic!("owner root")
        };
        owners.push(*owner);
    }
    assert_eq!(
        owners[0], owners[1],
        "same owner, distinct loan capabilities"
    );
    let ends = |loan: LoanId| {
        caller
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(index, i)| {
                (i.operation == Operation::BorrowEnd { loan }).then_some(index)
            })
            .collect::<Vec<_>>()
    };
    let aux_ends = ends(*auxiliary);
    let source_ends = ends(*source);
    let result_ends = ends(*result);
    assert_eq!(aux_ends.len(), 1);
    assert_eq!(source_ends.len(), 1);
    assert_eq!(result_ends.len(), 1);
    let uses = caller
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, i)| match &i.operation {
            Operation::DirectCall {
                callee,
                receiver: None,
                arguments,
            } if arguments == &[EntityId::Loan(*result)] => Some((index, *callee)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(use_index, consume)] = uses.as_slice() else {
        panic!("one consumer")
    };
    assert!(
        module
            .function(*consume)
            .unwrap()
            .name
            .contains(".consume.")
    );
    assert!(*call_index < aux_ends[0] && aux_ends[0] < *use_index);
    assert!(*use_index < result_ends[0] && result_ends[0] < source_ends[0]);
    let drops = caller
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, i)| {
            matches!(i.operation, Operation::Drop { owner } if owner == owners[0]).then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1);
    assert!(source_ends[0] < drops[0]);
    assert_eq!(drops[0], caller.instructions.len() - 1, "owner drops last");
}
