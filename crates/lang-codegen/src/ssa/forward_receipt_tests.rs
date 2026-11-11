//! Missing production receipt, after all original normal lowerings succeed.
use super::forward_causal_links::{SOURCE, assert_original_forward, assert_original_ssa};
use super::*;
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};

#[test]
fn original_live_forward_requires_production_receipt_with_true_off() {
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "forward-receipt.ko", SOURCE);
    let (environment, types) = standard_environments();
    let mut completed = Vec::new();
    for (layout, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "forward-receipt.ko", source, &parsed),
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
        assert_eq!(
            on.ownership().owner_observations(),
            off.ownership().owner_observations()
        );
        let [catalog] = on.ownership().loan_action_observations() else {
            panic!("one catalog")
        };
        let [region] = catalog.forward_regions() else {
            panic!("existing live exit")
        };
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
        let ids = region.action_ids().collect::<Vec<_>>();
        let callable = region.callable();
        let call = region.body();
        let run = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap();
        let mut outputs = Vec::new();
        for (setting, owned) in [("on", &on), ("off", &off)] {
            assert_original_forward(&sources, &parsed, &names, owned.ownership());
            let ((lowered, routes), receipts) =
                super::super::refine::forward_receipt::observe(|| {
                    crate::ssa::refine::observe_routing(|| {
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
            let (program, entry) = lowered.unwrap();
            verify_program(&program).unwrap();
            assert_original_ssa(&program, entry, &routes);
            let llvm = crate::llvm::render_verified_program(&program).unwrap();
            println!(
                "layout={layout}; observer={setting}; original_ssa_verified=true; llvm_bytes={}",
                llvm.len()
            );
            outputs.push((crate::ssa::render_program(&program), llvm));
            completed.push((
                layout,
                setting,
                program,
                ids.clone(),
                callable,
                call,
                receipts,
            ));
        }
        assert_eq!(outputs[0], outputs[1]);
    }
    println!("all_original_prerequisites_complete=2_layouts_4_lowerings");
    for (layout, setting, program, ids, callable, call, receipts) in completed {
        if setting == "off" {
            assert!(receipts.is_empty());
            continue;
        }
        assert_eq!(
            receipts.len(),
            1,
            "{layout}: Complete live exit needs a real production receipt"
        );
        let r = &receipts[0];
        assert_eq!(r.ids, ids);
        assert_eq!(r.callable, callable);
        assert_eq!(r.call, call);
        let caller = program.modules[0]
            .functions
            .iter()
            .find(|f| f.instructions.len() == 8)
            .unwrap();
        let Operation::BorrowCall {
            callee: wrapper_id, ..
        } = caller.instructions[3].operation
        else {
            panic!("original caller wrapper call")
        };
        assert_eq!(r.caller, wrapper_id);
        let expected_wrapper = program.modules[0]
            .functions
            .iter()
            .find(|f| f.name.contains(".wrap."))
            .unwrap();
        assert_eq!(r.caller, expected_wrapper.id());
        let function = program.modules[0].function(r.caller).unwrap();
        assert_eq!(function.instructions.len(), 1);
        assert_eq!(function.blocks.len(), 1);
        let data = function.instruction(r.instruction).unwrap();
        assert_eq!(data.block, r.block);
        assert_eq!(data.results, [EntityId::Loan(r.result)]);
        let Operation::BorrowCall {
            callee,
            arguments,
            source,
        } = &data.operation
        else {
            panic!("actual BorrowCall")
        };
        assert!(
            program.modules[0]
                .function(*callee)
                .unwrap()
                .instructions
                .is_empty()
        );
        assert_eq!(
            program.modules[0].function(*callee).unwrap().blocks[0]
                .terminator
                .as_ref()
                .unwrap()
                .kind,
            TerminatorKind::BorrowReturn {
                loan: match program.modules[0].function(*callee).unwrap().blocks[0].parameters[0] {
                    EntityId::Loan(l) => l,
                    _ => panic!("view parameter"),
                }
            }
        );
        let expected_view = program.modules[0]
            .functions
            .iter()
            .find(|f| f.name.contains(".view."))
            .unwrap();
        assert_eq!(*callee, expected_view.id());
        assert_ne!(*callee, r.caller);
        assert_eq!(*source, r.source);
        assert_eq!(arguments, &[EntityId::Loan(r.source)]);
        assert_eq!(r.source.function(), r.caller);
        assert_eq!(r.result.function(), r.caller);
        assert_ne!(r.source, r.result);
        assert_eq!(function.blocks[0].parameters, [EntityId::Loan(r.source)]);
        assert_eq!(
            function.blocks[0].terminator.as_ref().unwrap().kind,
            TerminatorKind::BorrowReturn { loan: r.result }
        );
    }
}
