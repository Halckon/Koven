//! A borrowed receiver caller forwards a loan, while its outer caller owns cleanup.
use crate::ssa::loan_region_ssa_tests::*;

pub(super) const SOURCE: &str =
    "class Node { fun touch(): Int = 1 }\nfun run(node: Node): Unit { node.touch() }";

#[test]
fn borrowed_receiver_refines_and_forwards_entry_loan_without_owning_release() {
    verify_borrowed_receiver(SOURCE);
}

#[test]
fn borrowed_receiver_refine_anchor_forwards_entry_loan_without_owning_release() {
    let anchored = SOURCE.replace(
        "{ node.touch() }",
        "{ val anchor: Int? = null; node.touch() }",
    );
    assert_ne!(anchored, SOURCE);
    verify_borrowed_receiver(&anchored);
}

fn verify_borrowed_receiver(source_text: &str) {
    let text = format!("{source_text}\nfun entry(): Unit {{ val node = Node(); run(node) }}");
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "borrowed_receiver.ko", &text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "borrowed_receiver.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name() == "entry")
            .unwrap()
            .id();
        let ((lowered, routed), receipts) = crate::ssa::refine::receiver_receipt::observe(|| {
            crate::ssa::refine::observe_routing(|| {
                crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                    &sources, &inputs, &names, &types, &typed, &owned, entry,
                )
            })
        });
        let (program, entry) = lowered.unwrap();
        let [module] = program.modules.as_slice() else {
            panic!("one module");
        };
        let outer = module.function(entry).unwrap();
        let [block] = outer.blocks.as_slice() else {
            panic!("straight-line outer caller");
        };
        let unique = |predicate: fn(&Operation) -> bool| {
            let selected = block
                .instructions
                .iter()
                .enumerate()
                .map(|(index, id)| (index, outer.instruction(*id).unwrap()))
                .filter(|(_, instruction)| predicate(&instruction.operation))
                .collect::<Vec<_>>();
            assert_eq!(selected.len(), 1, "{:?}", outer.instructions);
            selected[0]
        };
        let (allocate_at, allocate) = unique(|op| matches!(op, Operation::HeapAllocate { .. }));
        let [EntityId::Value(owner)] = allocate.results.as_slice() else {
            panic!("one allocated owner");
        };
        let (root_at, root) = unique(|op| matches!(op, Operation::RootPlace { .. }));
        assert_eq!(root.operation, Operation::RootPlace { owner: *owner });
        let [EntityId::Place(place)] = root.results.as_slice() else {
            panic!("place rooted in this owner");
        };
        let (begin_at, begin) = unique(|op| matches!(op, Operation::BorrowBegin { .. }));
        assert_eq!(
            begin.operation,
            Operation::BorrowBegin {
                place: *place,
                kind: LoanKind::Shared
            }
        );
        let [EntityId::Loan(loan)] = begin.results.as_slice() else {
            panic!("fresh outer shared loan");
        };
        let (call_at, call) = unique(|op| matches!(op, Operation::DirectCall { .. }));
        let Operation::DirectCall {
            callee,
            receiver: None,
            arguments,
        } = &call.operation
        else {
            panic!("plain call to run");
        };
        assert_eq!(arguments, &[EntityId::Loan(*loan)]);
        assert_eq!(sources.slice(call.origin.span()).unwrap(), "run(node)");
        let (end_at, end) = unique(|op| matches!(op, Operation::BorrowEnd { .. }));
        assert_eq!(end.operation, Operation::BorrowEnd { loan: *loan });
        let (drop_at, drop) = unique(|op| matches!(op, Operation::Drop { .. }));
        assert_eq!(drop.operation, Operation::Drop { owner: *owner });
        assert!(allocate_at < root_at && root_at < begin_at && begin_at < call_at);
        assert!(call_at < end_at && end_at < drop_at);

        let run = module.function(*callee).unwrap();
        assert!(
            routed.contains(&run.name),
            "borrowed caller route with source {source_text}: {routed:?}"
        );
        assert!(receipts.iter().all(|receipt| receipt.caller != *callee));
        assert_eq!(
            sources.slice(run.origin.span()).unwrap(),
            source_text.split_once('\n').unwrap().1
        );
        let [run_block] = run.blocks.as_slice() else {
            panic!("straight-line borrowed caller");
        };
        let [EntityId::Loan(imported)] = run_block.parameters.as_slice() else {
            panic!("imported shared loan, not an owned argument");
        };
        assert_ne!(loan.function(), imported.function());
        assert_eq!(
            outer.entity(EntityId::Loan(*loan)).unwrap().ty,
            run.entity(EntityId::Loan(*imported)).unwrap().ty
        );
        if source_text == SOURCE {
            assert_eq!(run.instructions.len(), 1);
        }
        assert!(run.instructions.iter().all(|instruction| !matches!(
            instruction.operation,
            Operation::BorrowBegin { .. } | Operation::BorrowEnd { .. } | Operation::Drop { .. }
        )));
        let forwards = run
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
            .collect::<Vec<_>>();
        let [forward] = forwards.as_slice() else {
            panic!("exactly one forwarding call");
        };
        let Operation::DirectCall {
            callee: method,
            receiver,
            arguments,
        } = &forward.operation
        else {
            panic!("forwarding member call");
        };
        assert_eq!(*receiver, Some(EntityId::Loan(*imported)));
        assert!(arguments.is_empty());
        assert_eq!(
            sources.slice(forward.origin.span()).unwrap(),
            "node.touch()"
        );
        let method = module.function(*method).unwrap();
        assert!(
            method
                .instructions
                .iter()
                .all(|instruction| !matches!(instruction.operation, Operation::Drop { .. }))
        );

        verify_program(&program).unwrap();
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        #[cfg(feature = "loan-observation-test-support")]
        {
            use lang_frontend::ownership_checking::{
                check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
            };
            assert!(!owned.ownership().loan_action_observations().is_empty());
            let off = with_loan_observation(false, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .validate()
                    .unwrap()
            });
            assert!(legacy_ownership_eq(owned.ownership(), off.ownership()));
            assert!(off.ownership().loan_action_observations().is_empty());
            let entry_declaration = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|declaration| declaration.name() == "entry")
                .unwrap()
                .id();
            let ((off_lowered, off_routed), off_receipts) =
                crate::ssa::refine::receiver_receipt::observe(|| {
                    crate::ssa::refine::observe_routing(|| {
                        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                            &sources,
                            &inputs,
                            &names,
                            &types,
                            &typed,
                            &off,
                            entry_declaration,
                        )
                    })
                });
            let (off_program, off_entry) = off_lowered.unwrap();
            assert_eq!(
                outer.name,
                off_program.modules[0].function(off_entry).unwrap().name
            );
            assert_eq!(routed, off_routed);
            assert!(off_receipts.is_empty());
            verify_program(&off_program).unwrap();
            assert_eq!(
                crate::ssa::render_program(&program),
                crate::ssa::render_program(&off_program)
            );
            assert_eq!(
                llvm,
                crate::llvm::render_verified_program_with_entry(&off_program, off_entry).unwrap()
            );
        }

        let output =
            crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(&llvm, &[0]);
        crate::native_tests::boxed_enum_tests::assert_success(&output, b"");
    }
}
