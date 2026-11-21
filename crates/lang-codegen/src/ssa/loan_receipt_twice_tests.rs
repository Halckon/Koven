//! Ordered receiver loans and one owner drop under fallback and independently anchored Refine.
use crate::ssa::loan_region_ssa_tests::*;
use lang_frontend::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction, UnitLoanAction,
    UnitLoanActionCoverage, UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
};

pub(super) const SOURCE: &str = "class Node { fun touch(): Int = 1 }\nfun run(own node: Node): Unit { node.touch(); node.touch() }";

#[test]
fn two_shared_receiver_calls_release_one_owner_after_both_loans_end() {
    verify_two_shared_receiver_calls(SOURCE, false);
}

#[test]
fn two_shared_receiver_calls_refine_anchor_preserves_order_and_single_owner_release() {
    let anchored = SOURCE.replace(
        "{ node.touch(); node.touch() }",
        "{ val anchor: Int? = null; node.touch(); node.touch() }",
    );
    assert_ne!(anchored, SOURCE);
    verify_two_shared_receiver_calls(&anchored, true);
}

fn verify_two_shared_receiver_calls(source_text: &str, expect_refine: bool) {
    let text = format!("{source_text}\nfun entry(): Unit {{ run(Node()) }}");
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "two_receiver.ko", &text);
    let (environment, types) = standard_environments();
    let call_offsets = source_text
        .match_indices("node.touch()")
        .map(|(offset, token)| (offset, offset + token.len()))
        .collect::<Vec<_>>();
    for input in [
        SourceUnitInput::new("root", "two_receiver.ko", source, &parsed),
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
        let allocations = outer
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::HeapAllocate { .. }))
            .collect::<Vec<_>>();
        let [allocation] = allocations.as_slice() else {
            panic!("one allocation in the outer caller");
        };
        let [EntityId::Value(allocated_owner)] = allocation.results.as_slice() else {
            panic!("owner constructed by Node()");
        };
        let calls = outer
            .instructions
            .iter()
            .filter_map(|instruction| match &instruction.operation {
                Operation::DirectCall {
                    callee,
                    receiver: None,
                    arguments,
                } => Some((*callee, arguments)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(callee, arguments)] = calls.as_slice() else {
            panic!("entry transfers to run exactly once");
        };
        assert_eq!(*arguments, &[EntityId::Value(*allocated_owner)]);
        assert!(
            outer
                .instructions
                .iter()
                .all(|i| !matches!(i.operation, Operation::Drop { .. }))
        );
        let run = module.function(*callee).unwrap();
        assert_eq!(
            sources.slice(run.origin.span()).unwrap(),
            source_text.split_once('\n').unwrap().1
        );
        assert_eq!(
            routed.contains(&run.name),
            expect_refine,
            "two-call caller route with source {source_text}: {routed:?}"
        );
        let mut raw_intervals = Vec::new();
        for (index, call_span) in call_offsets.iter().copied().enumerate() {
            let mut regions = owned
                .ownership()
                .loan_action_observations()
                .iter()
                .flat_map(|obs| obs.regions().iter().map(move |region| (obs, region)))
                .filter(|(obs, region)| {
                    obs.source() == region.call().source_unit()
                        && parsed
                            .ast()
                            .expressions()
                            .get(region.call().expression())
                            .is_ok_and(|node| {
                                let span = node.span();
                                span.source_id() == source
                                    && (span.start(), span.end()) == call_span
                            })
                });
            let (observation, region) = regions
                .next()
                .expect("a region for this exact receiver call");
            assert!(regions.next().is_none());
            assert_eq!(region.protocol_version(), 2);
            let expected = if expect_refine {
                // The declaration anchor is outside the flat expression-only observation gate.
                UnitLoanActionCoverage::Unsupported
            } else if index == 0 {
                // The first call retires its loan, but does not drop the owner.
                UnitLoanActionCoverage::Partial
            } else {
                // The final call has its own local certificate, not one for both calls.
                UnitLoanActionCoverage::Complete
            };
            assert_eq!(region.coverage(), expected, "receiver call {index}");
            if !expect_refine {
                assert!(observation.owns_region(region));
                let ids = region.action_ids().collect::<Vec<_>>();
                assert_eq!(ids.len(), 2 + usize::from(index == 1));
                raw_intervals.push((observation, region.call(), ids));
            }
        }
        if !expect_refine {
            let [(observation, first, first_ids), (_, second, second_ids)] =
                raw_intervals.as_slice()
            else {
                panic!("exactly two source-qualified caller intervals");
            };
            assert_ne!(first, second);
            let ordered = first_ids.iter().chain(second_ids).collect::<Vec<_>>();
            let actual_index = (0..observation.actions().len())
                .find(|index| observation.action_id(*index).as_ref() == Some(ordered[0]))
                .unwrap();
            assert!(ordered.iter().enumerate().all(|(offset, id)| {
                observation.action_id(actual_index + offset).as_ref() == Some(*id)
            }));
            let actions = ordered
                .iter()
                .map(|id| observation.action(id).unwrap())
                .collect::<Vec<_>>();
            let [
                UnitLoanAction::ReceiverBegin(first_begin),
                UnitLoanAction::ReceiverEnd(first_end),
                UnitLoanAction::ReceiverBegin(second_begin),
                UnitLoanAction::ReceiverEnd(second_end),
                UnitLoanAction::Cleanup {
                    point,
                    action: UnitIterationCleanupAction::Drop(drop),
                },
            ] = actions.as_slice()
            else {
                panic!("first pair, second pair, then one terminal cleanup");
            };
            for (call, begin, end) in [
                (first, first_begin, first_end),
                (second, second_begin, second_end),
            ] {
                assert_eq!(begin.call(), *call);
                assert_eq!(owned.ownership().receiver_fact(*call), Some(begin));
                assert_eq!(begin.kind(), UnitReceiverOwnershipKind::SharedLoan);
                assert_eq!(end.receiver(), begin);
                assert_eq!(end.point(), UnitDropPoint::CallReturn(*call));
                assert!(owned.ownership().receiver_call_return_ends().contains(end));
            }
            let UnitReceiverOwnershipTarget::Place(place) = first_begin.target() else {
                panic!("stable source root");
            };
            assert!(place.is_root());
            assert_eq!(first_begin.target(), second_begin.target());
            assert_eq!(*point, UnitDropPoint::CallReturn(*second));
            assert_eq!(drop.point(), *point);
            assert_eq!(drop.target(), UnitDropTarget::Named(place.root()));
            let drops = owned
                .ownership()
                .drops()
                .iter()
                .filter(|row| row.target() == drop.target())
                .collect::<Vec<_>>();
            let [actual_drop] = drops.as_slice() else {
                panic!("exactly one same-root drop in the ownership product");
            };
            assert_eq!(*actual_drop, drop);
        }
        assert!(receipts.iter().all(|receipt| receipt.caller != *callee));
        let [block] = run.blocks.as_slice() else {
            panic!("one caller block");
        };
        let [EntityId::Value(owner)] = block.parameters.as_slice() else {
            panic!("run owns one transferred argument");
        };
        let operations = block
            .instructions
            .iter()
            .map(|id| run.instruction(*id).unwrap())
            .collect::<Vec<_>>();
        let indexed = |predicate: fn(&Operation) -> bool| {
            operations
                .iter()
                .enumerate()
                .filter(|(_, i)| predicate(&i.operation))
                .map(|(at, i)| (at, *i))
                .collect::<Vec<_>>()
        };
        let all_roots = indexed(|op| matches!(op, Operation::RootPlace { .. }));
        let begins = indexed(|op| matches!(op, Operation::BorrowBegin { .. }));
        let calls = indexed(|op| matches!(op, Operation::DirectCall { .. }));
        let ends = indexed(|op| matches!(op, Operation::BorrowEnd { .. }));
        let drops = indexed(|op| matches!(op, Operation::Drop { .. }));
        assert_eq!(
            (begins.len(), calls.len(), ends.len(), drops.len()),
            (2, 2, 2, 1)
        );
        let mut actual_loans = Vec::new();
        let mut callees = Vec::new();
        for (at, call_span) in call_offsets.iter().copied().enumerate() {
            let (begin_at, begin) = begins[at];
            let (call_at, call) = calls[at];
            let (end_at, end) = ends[at];
            let Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } = begin.operation
            else {
                panic!("only a shared receiver is admitted");
            };
            let [EntityId::Loan(loan)] = begin.results.as_slice() else {
                panic!("one loan for this invocation");
            };
            assert!(all_roots.iter().any(|(root_at, root)| {
                *root_at < begin_at
                    && root.operation == Operation::RootPlace { owner: *owner }
                    && root.results == [EntityId::Place(place)]
            }));
            let Operation::DirectCall {
                callee,
                receiver,
                arguments,
            } = &call.operation
            else {
                panic!("direct call");
            };
            assert_eq!(*receiver, Some(EntityId::Loan(*loan)));
            assert!(arguments.is_empty());
            assert_eq!(end.operation, Operation::BorrowEnd { loan: *loan });
            assert!(begin_at < call_at && call_at < end_at);
            assert_span(call.origin.span(), source, call_span);
            assert_span(end.origin.span(), source, call_span);
            actual_loans.push(*loan);
            callees.push(*callee);
        }
        assert_ne!(actual_loans[0], actual_loans[1]);
        assert_eq!(callees[0], callees[1]);
        assert!(ends[0].0 < begins[1].0 && ends[1].0 < drops[0].0);
        assert_eq!(drops[0].1.operation, Operation::Drop { owner: *owner });

        verify_program(&program).unwrap();
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let output =
            crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(&llvm, &[0]);
        crate::native_tests::boxed_enum_tests::assert_success(&output, b"");
    }
}
