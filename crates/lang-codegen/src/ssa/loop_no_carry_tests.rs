//! Real loop-local lease lifetime, independent of observer receipts and native true loops.
use super::*;
use lang_frontend::{
    ownership_checking::{
        UnitDropPoint, check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
    },
    parser::{Expression, Statement},
};

#[test]
fn ordinary_loop_local_no_carry_has_body_only_lease_and_zero_iteration_exit() {
    for (count, reversed) in [1, 3, 9]
        .into_iter()
        .flat_map(|count| [false, true].map(move |reversed| (count, reversed)))
    {
        let consumers = vec!["consume(item)"; count].join(";");
        let text = format!(
            "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun run(source:String,flag:Boolean):Unit {{while(flag){{borrow val item=view(source);{consumers}}}}}"
        );
        let text = if reversed {
            text.replace(
                "run(source:String,flag:Boolean)",
                "run(flag:Boolean,source:String)",
            )
        } else {
            text
        };
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "loop-local.ko", &text);
        let (environment, types) = standard_environments();
        let last = parsed
            .ast()
            .statements()
            .iter()
            .filter_map(|(id, node)| match node.payload() {
                Statement::Expression { expression }
                    if matches!(
                        parsed
                            .ast()
                            .expressions()
                            .get(*expression)
                            .unwrap()
                            .payload(),
                        Expression::Call { .. }
                    ) =>
                {
                    Some((node.span().start(), id))
                }
                _ => None,
            })
            .max_by_key(|(start, _)| *start)
            .unwrap()
            .1;
        for input in [
            SourceUnitInput::new("root", "loop-local.ko", source, &parsed),
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
            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap()
                .id();
            let mut baseline = None;
            for owned in [&off, &on] {
                let [binding] = owned.ownership().borrow_results().bindings() else {
                    panic!("one actual fresh binding")
                };
                let [end] = owned.ownership().borrow_results().ends() else {
                    panic!("one actual last-use End")
                };
                assert_eq!(binding.binding(), end.binding());
                let UnitDropPoint::AfterStatement(point) = end.point() else {
                    panic!("not scope cleanup")
                };
                assert_eq!(point.statement(), last);
                let ((result, verified), receipts) =
                    crate::ssa::refine::fresh_loop_receipt::observe(|| {
                        crate::ssa::unit_lower::observe_final_verifier(|| {
                            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                &sources, &inputs, &names, &types, &typed, owned, entry,
                            )
                        })
                    });
                let (program, _) = result.expect("real loop-local body lowering");
                assert_eq!(verified.len(), 1);
                assert_eq!(receipts.len(), 1, "actual fresh-loop receipt missing");
                verify_program(&program).unwrap();
                let run = program
                    .modules
                    .iter()
                    .flat_map(|m| &m.functions)
                    .find(|f| f.name.contains(".run."))
                    .unwrap();
                let receipt = receipts[0].clone();
                receipt
                    .validate(run)
                    .expect("detached actual no-carry proof");
                assert_eq!(receipt.caller, run.id);
                assert!(owned.ownership().owns_no_carry_loop(&receipt.loops[0].fact));
                assert_eq!(receipt.loops[0].fact.binding(), binding.binding());
                assert_eq!(receipt.loops[0].instructions.len(), count + 3);
                let calls = run
                    .instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::BorrowCall { .. }))
                    .collect::<Vec<_>>();
                assert_eq!(calls.len(), 1);
                let acquire = calls[0];
                let Operation::BorrowCall {
                    source: external, ..
                } = acquire.operation
                else {
                    unreachable!()
                };
                assert!(
                    run.instructions.iter().all(|i| !matches!(i.operation,
                        Operation::BorrowEnd { loan } if loan == external
                    )),
                    "the external parameter lease must not end with the fresh result"
                );
                let [EntityId::Loan(result)] = acquire.results.as_slice() else {
                    panic!("borrow result loan")
                };
                let body = run.blocks.iter().find(|b| b.id == acquire.block).unwrap();
                let end_instructions = run
                    .instructions
                    .iter()
                    .filter(|i| matches!(i.operation,Operation::BorrowEnd{loan} if loan==*result))
                    .collect::<Vec<_>>();
                assert_eq!(end_instructions.len(), 1);
                assert_eq!(end_instructions[0].block, body.id);
                let acquired_at = body
                    .instructions
                    .iter()
                    .position(|id| *id == acquire.id)
                    .unwrap();
                let ended_at = body
                    .instructions
                    .iter()
                    .position(|id| *id == end_instructions[0].id)
                    .unwrap();
                assert!(acquired_at < ended_at);
                let consumer_instructions = run
                    .instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
                    .collect::<Vec<_>>();
                assert_eq!(consumer_instructions.len(), count);
                for call in consumer_instructions {
                    let Operation::DirectCall {
                        receiver,
                        arguments,
                        ..
                    } = &call.operation
                    else {
                        unreachable!()
                    };
                    assert!(receiver.is_none());
                    assert_eq!(
                        arguments.as_slice(),
                        &[EntityId::Loan(*result)],
                        "each consumer must use this iteration's fresh result, not the source"
                    );
                    assert_eq!(call.block, body.id);
                    let index = body
                        .instructions
                        .iter()
                        .position(|id| *id == call.id)
                        .unwrap();
                    assert!(acquired_at < index && index < ended_at);
                }
                let TerminatorKind::Branch(backedge) = &body.terminator.as_ref().unwrap().kind
                else {
                    panic!("normal actual backedge")
                };
                assert!(!backedge.arguments.contains(&EntityId::Loan(*result)));
                let header = run.blocks.iter().find(|b| b.id == backedge.target).unwrap();
                let TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } = &header.terminator.as_ref().unwrap().kind
                else {
                    panic!("actual dynamic condition")
                };
                assert_eq!(when_true.target, body.id);
                // Resolve the actual parameter positions along all three edges, rather
                // than assuming header/body slots preserve the source parameter index.
                let external = EntityId::Loan(external);
                let body_source = body
                    .parameters
                    .iter()
                    .position(|entity| *entity == external)
                    .expect("BorrowCall source is a body parameter");
                assert_eq!(
                    body.parameters
                        .iter()
                        .filter(|entity| **entity == external)
                        .count(),
                    1
                );
                let header_source = when_true.arguments[body_source];
                let header_index = header
                    .parameters
                    .iter()
                    .position(|entity| *entity == header_source)
                    .expect("body source comes from a header parameter");
                assert_eq!(
                    backedge.arguments[header_index], external,
                    "backedge preserves the external lease, never the fresh result"
                );
                let entry_block = run.block(run.entry_block().unwrap()).unwrap();
                assert!(run.receiver.is_none());
                let source_parameter = entry_block.parameters[usize::from(reversed)];
                assert!(matches!(source_parameter, EntityId::Loan(_)));
                let TerminatorKind::Branch(initial_edge) =
                    &entry_block.terminator.as_ref().unwrap().kind
                else {
                    panic!("entry transports the external parameter into the loop header")
                };
                assert_eq!(initial_edge.target, header.id);
                assert_eq!(
                    initial_edge.arguments[header_index], source_parameter,
                    "the header source is exactly run's actual declared Borrow String parameter"
                );
                let Operation::BorrowCall { arguments, .. } = &acquire.operation else {
                    unreachable!()
                };
                assert_eq!(arguments.as_slice(), &[external]);
                // Follow every actual false-edge successor. A zero-iteration path cannot visit
                // either a fresh acquisition or the body, not merely omit it from entry.
                let mut pending = vec![when_false.target];
                let mut seen = std::collections::BTreeSet::new();
                let mut exits = 0;
                while let Some(id) = pending.pop() {
                    if !seen.insert(id) {
                        continue;
                    }
                    assert_ne!(id, body.id);
                    let block = run.blocks.iter().find(|b| b.id == id).unwrap();
                    assert!(block.instructions.iter().all(|id| !matches!(
                        run.instruction(*id).unwrap().operation,
                        Operation::BorrowCall { .. }
                            | Operation::BorrowBegin { .. }
                            | Operation::BorrowEnd { .. }
                    )));
                    match &block.terminator.as_ref().unwrap().kind {
                        TerminatorKind::Branch(edge) => pending.push(edge.target),
                        TerminatorKind::Return { values } => {
                            assert!(values.is_empty());
                            exits += 1;
                        }
                        other => panic!("unexpected zero-iteration successor {other:?}"),
                    }
                }
                assert_eq!(exits, 1);
                let rendered = format!(
                    "{verified:?}\n{}",
                    crate::llvm::render_verified_program(&program).unwrap()
                );
                if let Some(previous) = baseline.as_ref() {
                    assert_eq!(&rendered, previous);
                }
                baseline = Some(rendered);
            }
        }
    }
}

#[test]
fn ordinary_fresh_loop_receipt_non_candidates_preserve_boundaries() {
    for (label, parameters, body, expected) in [
        (
            "own-flag",
            "source:String,own flag:Boolean",
            "while(flag){borrow val item=view(source);consume(item)}",
            1,
        ),
        (
            "siblings",
            "source:String,flag:Boolean",
            "while(flag){borrow val item=view(source);consume(item)};while(flag){borrow val other=view(source);consume(other)}",
            2,
        ),
        (
            "unused-flag",
            "source:String,flag:Boolean,unused:Boolean",
            "while(flag){borrow val item=view(source);consume(item)}",
            1,
        ),
        (
            "before-after",
            "source:String,flag:Boolean",
            "consume(source);while(flag){borrow val item=view(source);consume(item)};consume(source)",
            1,
        ),
    ] {
        let text = format!(
            "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun run({parameters}):Unit {{{body}}}"
        );
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "fresh-neighbor.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "fresh-neighbor.ko", source, &parsed),
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
            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap()
                .id();
            let mut baseline = None;
            for owned in [&off, &on] {
                assert_eq!(owned.ownership().no_carry_loops().len(), expected);
                let ((result, verified), receipts) =
                    crate::ssa::refine::fresh_loop_receipt::observe(|| {
                        crate::ssa::unit_lower::observe_final_verifier(|| {
                            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                &sources, &inputs, &names, &types, &typed, owned, entry,
                            )
                        })
                    });
                let (program, _) = result.unwrap_or_else(|error| panic!("{label}: {error:?}"));
                assert_eq!(verified.len(), 1);
                assert_eq!(receipts.len(), usize::from(label == "siblings"));
                verify_program(&program).unwrap();
                let rendered = crate::llvm::render_verified_program(&program).unwrap();
                if let Some(previous) = baseline.as_ref() {
                    assert_eq!(&rendered, previous);
                }
                baseline = Some(rendered);
            }
        }
    }
}
