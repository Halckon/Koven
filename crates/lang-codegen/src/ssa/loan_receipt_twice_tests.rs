//! Ordered receiver loans and one owner drop under fallback and independently anchored Refine.
use crate::ssa::loan_region_ssa_tests::*;
#[cfg(feature = "loan-observation-test-support")]
#[path = "loan_twice_mandatory_test.rs"]
mod mandatory;
use lang_frontend::ownership_checking::{
    CompilationUnitOwnership, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction,
    UnitLoanAction, UnitLoanActionCoverage, UnitLoanActionId, UnitLoanActionObservation,
    UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
};

use lang_frontend::type_checking::UnitExpressionId;

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

#[test]
fn independently_anchored_distinct_shared_methods_keep_per_call_dispatch_and_owner_release() {
    // Preserve the exact negative's declarations and two calls. The local nullable
    // anchor selects the existing Refine route, not the bounded two-call trigger.
    let source = "class Node { fun touch(): Int = 1; fun other(): Int = 2 }\nfun run(own node: Node): Unit { val anchor: Int? = null; node.touch(); node.other() }";
    verify_shared_receiver_calls(source, true, true);
}

fn verify_two_shared_receiver_calls(source_text: &str, anchored: bool) {
    verify_shared_receiver_calls(source_text, anchored, false);
}

fn verify_shared_receiver_calls(source_text: &str, anchored: bool, distinct: bool) {
    let text = format!("{source_text}\nfun entry(): Unit {{ run(Node()) }}");
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "two_receiver.ko", &text);
    let (environment, types) = standard_environments();
    let call_offsets = if distinct {
        ["node.touch()", "node.other()"]
            .into_iter()
            .map(|token| {
                let offset = source_text.find(token).unwrap();
                (offset, offset + token.len())
            })
            .collect::<Vec<_>>()
    } else {
        source_text
            .match_indices("node.touch()")
            .map(|(offset, token)| (offset, offset + token.len()))
            .collect::<Vec<_>>()
    };
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
        if distinct {
            use lang_frontend::type_checking::{UnitCallTarget, UnitCallableTarget};

            let plan = crate::ssa::unit_plan::plan_unit_instances(
                &sources, &inputs, &names, &types, &typed, &owned, entry,
            )
            .unwrap();
            let run = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap();
            let caller = plan
                .iter()
                .find(|instance| {
                    instance.key().target() == UnitCallableTarget::Declaration(run.id())
                })
                .unwrap();
            let mut dispatches = Vec::new();
            for &(start, end) in &call_offsets {
                let mut calls = typed.types().calls().iter().filter(|call| {
                    let span = parsed
                        .ast()
                        .expressions()
                        .get(call.expression().expression())
                        .unwrap()
                        .span();
                    span.source_id() == source && (span.start(), span.end()) == (start, end)
                });
                let call = calls.next().expect("exact original typed call");
                assert!(calls.next().is_none());
                let UnitCallTarget::Symbol(target) = call.target() else {
                    panic!("each site selects its actual member symbol");
                };
                let route = plan
                    .callable_plan()
                    .call_site(caller.source_token(), call.expression())
                    .unwrap();
                assert!(route.delegation().is_empty());
                let dispatch = route.dispatch_key().expect("direct selected member");
                assert_eq!(dispatch.target(), UnitCallableTarget::Symbol(target));
                assert!(plan.iter().any(|instance| instance.key() == dispatch));
                dispatches.push(dispatch);
            }
            assert_ne!(dispatches[0], dispatches[1]);
        }
        let (((lowered, routed), receipts), planned) =
            crate::ssa::refine::two_receiver_plan_test_support::observe(|| {
                crate::ssa::refine::receiver_receipt::observe(|| {
                    crate::ssa::refine::observe_routing(|| {
                        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                            &sources, &inputs, &names, &types, &typed, &owned, entry,
                        )
                    })
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
        assert!(
            routed.contains(&run.name),
            "two-call caller route with source {source_text}: {routed:?}"
        );
        let mut raw_intervals = Vec::new();
        let mut receiver_facts = Vec::new();
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
            receiver_facts.push(owned.ownership().receiver_fact(region.call()).unwrap());
            let expected = if anchored {
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
            if !anchored {
                assert!(observation.owns_region(region));
                let ids = region.action_ids().collect::<Vec<_>>();
                assert_eq!(ids.len(), 2 + usize::from(index == 1));
                raw_intervals.push((observation, region.call(), ids));
            }
        }
        if !anchored {
            let [(observation, first, first_ids), (_, second, second_ids)] =
                raw_intervals.as_slice()
            else {
                panic!("exactly two source-qualified caller intervals");
            };
            let args = [
                (*first, first_ids.as_slice()),
                (*second, second_ids.as_slice()),
            ];
            let ownership = owned.ownership();
            let audit = |candidate| audit_raw_intervals(ownership, observation, candidate);
            assert_eq!(audit(args), Ok(()));
            let reversed = [first_ids[1].clone(), first_ids[0].clone()];
            assert_eq!(
                audit([(*first, &reversed), args[1]]),
                Err(IntervalFault::Order)
            );
            assert_eq!(
                audit([(*second, first_ids), (*first, second_ids)]),
                Err(IntervalFault::Receiver)
            );
            let mut early_drop = first_ids.clone();
            early_drop.push(second_ids[2].clone());
            assert_eq!(
                audit([(*first, &early_drop), args[1]]),
                Err(IntervalFault::Shape)
            );
            let displaced_drop = [first_ids[0].clone(), second_ids[2].clone()];
            assert_eq!(
                audit([(*first, &displaced_drop), args[1]]),
                Err(IntervalFault::Order)
            );
            // Reanalysis of the same sources keeps source/item keys, not catalog identity.
            let (_, _, independent) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            let foreign_ids = independent
                .ownership()
                .loan_action_observations()
                .iter()
                .flat_map(|o| o.regions())
                .find(|region| region.call() == *second)
                .unwrap()
                .action_ids()
                .collect::<Vec<_>>();
            assert_eq!(foreign_ids.len(), 3);
            assert_ne!(&foreign_ids, second_ids);
            assert_eq!(
                audit([args[0], (*second, &foreign_ids)]),
                Err(IntervalFault::Catalog)
            );
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
        let mut plan_expectations = Vec::new();
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
            let checked = receiver_facts[at];
            assert_eq!(checked.end_span(), call.origin.span());
            plan_expectations.push((checked.begin_span(), checked.end_span(), *callee));
        }
        assert_ne!(actual_loans[0], actual_loans[1]);
        if distinct {
            assert_ne!(callees[0], callees[1]);
            for (actual, expected) in callees
                .into_iter()
                .zip(["fun touch(): Int = 1", "fun other(): Int = 2"])
            {
                let actual = module.function(actual).unwrap();
                assert_eq!(sources.slice(actual.origin.span()).unwrap(), expected);
            }
        } else {
            assert_eq!(callees[0], callees[1]);
        }
        assert!(ends[0].0 < begins[1].0 && ends[1].0 < drops[0].0);
        assert_eq!(drops[0].1.operation, Operation::Drop { owner: *owner });

        let for_run = planned
            .iter()
            .filter(|plan| plan.function == *callee)
            .collect::<Vec<_>>();
        {
            let [plan] = for_run.as_slice() else {
                panic!("actual planned Refine body for run")
            };
            let drop_span = plan
                .check_two_calls(plan_expectations.as_slice().try_into().unwrap())
                .expect("actual two-lifecycle ROp plan with one terminal drop");
            let UnitReceiverOwnershipTarget::Place(place) = receiver_facts[1].target() else {
                panic!("receiver root");
            };
            let source_drops = owned
                .ownership()
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
                .collect::<Vec<_>>();
            let [source_drop] = source_drops.as_slice() else {
                panic!("one source same-root drop")
            };
            assert_eq!(
                source_drop.point(),
                UnitDropPoint::CallReturn(receiver_facts[1].call())
            );
            assert_eq!(drop_span, source_drop.value_origin());
            assert_eq!(drop_span, drops[0].1.origin.span());
        }

        verify_program(&program).unwrap();
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        #[cfg(feature = "loan-observation-test-support")]
        if !anchored {
            use lang_frontend::ownership_checking::{
                check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
            };
            let off = with_loan_observation(false, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .validate()
                    .unwrap()
            });
            assert!(off.ownership().loan_action_observations().is_empty());
            assert!(legacy_ownership_eq(owned.ownership(), off.ownership()));
            let run_decl = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap();
            let [first, second] = mandatory::ordered_calls(&parsed, run_decl).unwrap();
            for product in [owned.ownership(), off.ownership()] {
                let audit =
                    |calls| mandatory::audit(&parsed, run_decl, typed.types(), product, calls);
                assert_eq!(audit([first, second]), Some(()));
                assert!(audit([second, first]).is_none());
                assert!(audit([first, first]).is_none());
            }
            let entry_decl = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "entry")
                .unwrap()
                .id();
            let ((off_lowered, off_routed), off_receipts) =
                crate::ssa::refine::receiver_receipt::observe(|| {
                    crate::ssa::refine::observe_routing(|| {
                        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                            &sources, &inputs, &names, &types, &typed, &off, entry_decl,
                        )
                    })
                });
            let (off_program, off_entry) = off_lowered.unwrap();
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

#[derive(Debug, PartialEq, Eq)]
enum IntervalFault {
    Shape,
    Catalog,
    Order,
    Receiver,
    Cleanup,
}

// Test-only contract shape: the copied candidate must resolve to actual records,
// never a fabricated frontend product or a new published certificate.
fn audit_raw_intervals(
    owned: &CompilationUnitOwnership,
    observation: &UnitLoanActionObservation,
    [(first, first_ids), (second, second_ids)]: [(UnitExpressionId, &[UnitLoanActionId]); 2],
) -> Result<(), IntervalFault> {
    if first_ids.len() != 2 || second_ids.len() != 3 || first == second {
        return Err(IntervalFault::Shape);
    }
    if first.source_unit() != observation.source() || second.source_unit() != observation.source() {
        return Err(IntervalFault::Catalog);
    }
    let ordered = first_ids.iter().chain(second_ids).collect::<Vec<_>>();
    let actions = ordered
        .iter()
        .map(|id| observation.action(id).ok_or(IntervalFault::Catalog))
        .collect::<Result<Vec<_>, _>>()?;
    let actual_index = (0..observation.actions().len())
        .find(|index| observation.action_id(*index).as_ref() == Some(ordered[0]))
        .ok_or(IntervalFault::Catalog)?;
    if !ordered
        .iter()
        .enumerate()
        .all(|(offset, id)| observation.action_id(actual_index + offset).as_ref() == Some(*id))
    {
        return Err(IntervalFault::Order);
    }
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
        return Err(IntervalFault::Shape);
    };
    for (call, begin, end) in [
        (first, first_begin, first_end),
        (second, second_begin, second_end),
    ] {
        if begin.call() != call
            || owned.receiver_fact(call) != Some(begin)
            || begin.kind() != UnitReceiverOwnershipKind::SharedLoan
            || begin.is_receiver_reservation()
            || begin.activation_point().is_some()
            || end.receiver() != begin
            || end.point() != UnitDropPoint::CallReturn(call)
            || !owned.receiver_call_return_ends().contains(end)
        {
            return Err(IntervalFault::Receiver);
        }
    }
    let UnitReceiverOwnershipTarget::Place(place) = first_begin.target() else {
        return Err(IntervalFault::Receiver);
    };
    if !place.is_root() || first_begin.target() != second_begin.target() {
        return Err(IntervalFault::Receiver);
    }
    let mut drops = owned
        .drops()
        .iter()
        .filter(|row| row.target() == drop.target());
    if *point != UnitDropPoint::CallReturn(second)
        || drop.point() != *point
        || drop.target() != UnitDropTarget::Named(place.root())
        || drops.next() != Some(drop)
        || drops.next().is_some()
    {
        return Err(IntervalFault::Cleanup);
    }
    Ok(())
}

#[test]
fn resource_receiver_pair_and_imported_forwarding_preserve_existing_native_route() {
    const RESOURCE: &str = r#"class Node() {
    fun touch(): Int = 1
    deinit() { println("release") }
}
fun runPair(own node: Node): Unit { node.touch(); node.touch() }
fun runBorrowed(node: Node): Unit { node.touch() }
fun entry(): Unit {
    runPair(Node())
    val allocated = Node()
    runBorrowed(allocated)
}
"#;
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "resource_pair.ko", RESOURCE);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "resource_pair.ko", source, &parsed),
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
            .find(|d| d.name() == "entry")
            .unwrap()
            .id();
        let facts = owned.ownership();
        assert_eq!(facts.receiver_facts().len(), 3);
        for receiver in facts.receiver_facts() {
            assert_eq!(
                typed.types().is_resource_type(receiver.receiver_type()),
                Some(true)
            );
            assert_eq!(receiver.kind(), UnitReceiverOwnershipKind::SharedLoan);
        }
        let ((lowered, routes), receipts) = crate::ssa::refine::receiver_receipt::observe(|| {
            crate::ssa::refine::observe_routing(|| {
                crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                    &sources, &inputs, &names, &types, &typed, &owned, entry,
                )
            })
        });
        let (program, entry) = lowered.unwrap();
        let module = &program.modules[0];
        for name in ["runPair", "runBorrowed"] {
            let run = module
                .functions
                .iter()
                .find(|function| {
                    let source = sources.slice(function.origin.span()).unwrap();
                    source.starts_with(&format!("fun {name}("))
                })
                .expect("resource caller in emitted program");
            // The identical Resource source already selected Refine on the
            // pre-candidate main baseline; the bounded non-Resource triggers
            // should not change this routed behavior or add a receipt.
            assert!(
                routes.contains(&run.name),
                "{name}: existing Resource route"
            );
            assert!(receipts.iter().all(|receipt| receipt.caller != run.id()));
        }
        verify_program(&program).unwrap();
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let output =
            crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(&llvm, &[0, 1]);
        crate::native_tests::boxed_enum_tests::assert_success(&output, b"release\nrelease\n");
    }
}

#[test]
fn two_receiver_route_work_scales_by_indexed_facts_and_callers() {
    use crate::ssa::refine::two_receiver_route_metrics;
    use std::fmt::Write;
    fn count(n: usize) -> two_receiver_route_metrics::Counts {
        let mut text = String::from("class Node() { fun touch(): Int = 1 }\n");
        for i in 0..n {
            writeln!(
                text,
                "fun run{i}(own node: Node): Unit {{ node.touch(); node.touch() }}"
            )
            .unwrap();
        }
        if n == 0 {
            text.push_str("fun neighbor(own node: Node): Unit { node.touch() }\n");
        }
        text.push_str("fun entry(): Unit {\n");
        if n == 0 {
            text.push_str("neighbor(Node())\n");
        }
        for i in 0..n {
            writeln!(text, "run{i}(Node())").unwrap();
        }
        text.push_str("}\n");
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "route_scaling.ko", &text);
        let inputs = [SourceUnitInput::memory_document(source, &parsed)];
        let (environment, types) = standard_environments();
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "entry")
            .unwrap()
            .id();
        let ((lowered, routes), counts) = two_receiver_route_metrics::observe(|| {
            crate::ssa::refine::observe_routing(|| {
                crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                    &sources, &inputs, &names, &types, &typed, &owned, entry,
                )
            })
        });
        assert_eq!(
            routes
                .iter()
                .filter(|route| route.starts_with("koven.run"))
                .count(),
            n,
            "{routes:?}"
        );
        verify_program(&lowered.unwrap().0).unwrap();
        counts
    }
    let n = 8;
    let first = count(n);
    let second = count(2 * n);
    eprintln!("N={n}: {first:?}; 2N: {second:?}");
    // A single-call neighbor may use the shared receiver index, but never the paired indices.
    assert_eq!(
        two_receiver_route_metrics::Counts {
            receiver_rows: 0,
            receiver_probes: 0,
            ..count(0)
        },
        two_receiver_route_metrics::Counts::default()
    );
    for (callers, counts) in [(n, first), (2 * n, second)] {
        assert_eq!(
            counts,
            two_receiver_route_metrics::Counts {
                bindings: callers,
                ends: 2 * callers,
                drops: callers,
                probes: 4 * callers,
                receiver_rows: 2 * callers,
                receiver_probes: 2 * callers,
            }
        );
    }
}
