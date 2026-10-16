//! Real numeric-loop SSA, with direct private transport/exhaustion guard oracles.
use super::super::cleanup_guard_harness::{self as harness, Fixture};
use super::*;

fn checkpoint<T>(
    fixture: &Fixture,
    fact: DropFact,
    check: impl FnOnce(&mut ExpressionLowerer<'_>, &mut Plan, Pair, Pair) -> T,
) -> T {
    let mut plan = candidate(
        &fixture.parsed,
        &fixture.names,
        &fixture.typed,
        &fixture.owned,
        fact,
    )
    .unwrap();
    let mut function = fixture.entry().clone();
    fixture.with_lowerer(&mut function, |lowerer| {
        let birth = harness::outer_context(lowerer, plan.root, plan.outer);
        let anchor = lowerer.numeric_while_birth(plan.root, birth).unwrap();
        let TerminatorKind::Conditional { when_true, .. } = &lowerer
            .function
            .block(anchor.block)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind
        else {
            panic!("numeric empty-range split")
        };
        let block = when_true.target;
        let owner =
            harness::before_cleanup(lowerer, block, plan.root, fact.value_origin(), Some(birth));
        let proof = lowerer.non_null_bindings[&plan.root];
        let current = Pair {
            block,
            owner,
            loan: proof.loan,
        };
        assert!(transport(
            lowerer.function,
            current,
            anchor,
            proof.inner,
            lowerer.ssa_types
        ));
        assert!(numeric_exit(lowerer, &plan, current, anchor));
        assert_eq!(
            lowerer.checked_nullable_numeric_owner(&plan).unwrap(),
            owner
        );
        check(lowerer, &mut plan, current, anchor)
    })
}

fn incoming_edge<'a>(lowerer: &'a mut ExpressionLowerer<'_>, current: Pair) -> &'a mut Edge {
    let (block, _) = harness::incoming(lowerer.function, current.block)
        .into_iter()
        .find(|(block, _)| *block != lowerer.loops[0].header)
        .unwrap();
    harness::edges_mut(
        &mut lowerer.function.blocks[block.index()]
            .terminator
            .as_mut()
            .unwrap()
            .kind,
    )
    .into_iter()
    .find(|edge| edge.target == current.block)
    .unwrap()
}

#[test]
fn nullable_numeric_cleanup_rejects_columns_ungrounded_cycles_and_false_exhaustion() {
    let original = crate::ssa::while_flow_tests::proof_cases::numeric_nested_source();
    for text in [
        original.clone(),
        original.replace("in 0..<2", "in ((0..<2))"),
    ] {
        let fixture = Fixture::analyze(&text);
        assert_eq!(fixture.program.modules[0].functions.len(), 1);
        assert!(fixture.entry().receiver.is_none());
        assert!(fixture.typed.type_parameters().is_empty());
        assert_eq!(
            fixture.entry().return_types,
            [fixture.type_ids[&fixture.typed.types().builtin(BuiltinType::Int).unwrap()]]
        );
        let facts = fixture
            .owned
            .drops()
            .iter()
            .copied()
            .filter(|&fact| {
                candidate(
                    &fixture.parsed,
                    &fixture.names,
                    &fixture.typed,
                    &fixture.owned,
                    fact,
                )
                .is_some()
            })
            .collect::<Vec<_>>();
        let [fact] = facts.as_slice() else {
            panic!("one real numeric Exit owner fact")
        };
        for &other in fixture
            .owned
            .drops()
            .iter()
            .filter(|other| **other != *fact)
        {
            assert!(
                candidate(
                    &fixture.parsed,
                    &fixture.names,
                    &fixture.typed,
                    &fixture.owned,
                    other
                )
                .is_none()
            );
        }
        for mutation in 0..8 {
            checkpoint(&fixture, *fact, |lowerer, plan, mut current, anchor| {
                let proof = lowerer.non_null_bindings[&plan.root];
                let owner_column =
                    harness::parameter(lowerer.function, EntityId::Value(current.owner)).1;
                let loan_column =
                    harness::parameter(lowerer.function, EntityId::Loan(current.loan)).1;
                match mutation {
                    0 => current.owner = anchor.owner,
                    1 => current.loan = anchor.loan,
                    2 | 3 => {
                        let edge = incoming_edge(lowerer, current);
                        edge.arguments.push(
                            edge.arguments[if mutation == 2 {
                                owner_column
                            } else {
                                loan_column
                            }],
                        );
                    }
                    4 | 5 => {
                        incoming_edge(lowerer, current)
                            .arguments
                            .remove(if mutation == 4 {
                                owner_column
                            } else {
                                loan_column
                            });
                    }
                    6 => lowerer.function.blocks[anchor.block.index()].terminator = None,
                    7 => {
                        // Retain an actual loop SCC but delete its only real entry.
                        lowerer.function.blocks[anchor.block.index()]
                            .terminator
                            .as_mut()
                            .unwrap()
                            .kind = TerminatorKind::Abort;
                    }
                    _ => unreachable!(),
                }
                assert!(
                    !transport(
                        lowerer.function,
                        current,
                        anchor,
                        proof.inner,
                        lowerer.ssa_types
                    ),
                    "transport mutation {mutation}"
                );
                if mutation < 2 {
                    let proof = lowerer.non_null_bindings.get_mut(&plan.root).unwrap();
                    proof.owner = current.owner;
                    proof.loan = current.loan;
                    lowerer
                        .bindings
                        .insert(plan.root, LoweredValue::Value(current.owner));
                }
                harness::unsupported(
                    lowerer.checked_nullable_numeric_owner(plan),
                    fact.value_origin(),
                );
            });
        }
        checkpoint(&fixture, *fact, |lowerer, plan, current, anchor| {
            // One good predecessor is insufficient: a different visited SCC has
            // no path to the birth even though the current node itself does.
            let first = lowerer
                .function
                .block(anchor.block)
                .unwrap()
                .terminator
                .as_ref()
                .unwrap()
                .clone();
            let TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } = first.kind
            else {
                unreachable!()
            };
            let header = when_false.target;
            for block in &mut lowerer.function.blocks {
                if let Some(term) = &mut block.terminator {
                    for edge in harness::edges_mut(&mut term.kind) {
                        if edge.target == header {
                            edge.target = current.block;
                        }
                    }
                }
            }
            lowerer.function.blocks[anchor.block.index()]
                .terminator
                .as_mut()
                .unwrap()
                .kind = TerminatorKind::Branch(when_true);
            let arguments = lowerer.function.block(header).unwrap().parameters.clone();
            lowerer.function.blocks[header.index()]
                .terminator
                .as_mut()
                .unwrap()
                .kind = TerminatorKind::Conditional {
                condition,
                when_true: Edge {
                    target: current.block,
                    arguments: arguments.clone(),
                },
                when_false: Edge {
                    target: header,
                    arguments,
                },
            };
            let proof = lowerer.non_null_bindings[&plan.root];
            assert!(!transport(
                lowerer.function,
                current,
                anchor,
                proof.inner,
                lowerer.ssa_types
            ));
        });
        for mutation in 0..5 {
            checkpoint(&fixture, *fact, |lowerer, plan, current, anchor| {
                match mutation {
                    0 => plan.statement = plan.outer,
                    1 => plan.outer = plan.statement,
                    2 => plan.root = fixture.symbol("initial"),
                    3 => {
                        lowerer.function.blocks[current.block.index()].origin = Origin::Source(
                            lowerer
                                .parsed
                                .ast()
                                .statements()
                                .get(plan.outer)
                                .unwrap()
                                .span(),
                        )
                    }
                    4 => {
                        let block = harness::incoming(lowerer.function, current.block)
                            .into_iter()
                            .find(|(block, _)| *block != anchor.block)
                            .unwrap()
                            .0;
                        lowerer.function.blocks[block.index()]
                            .terminator
                            .as_mut()
                            .unwrap()
                            .origin = Origin::Source(fact.value_origin());
                    }
                    _ => unreachable!(),
                }
                if mutation != 1 {
                    assert!(!numeric_exit(lowerer, plan, current, anchor));
                }
                harness::unsupported(
                    lowerer.checked_nullable_numeric_owner(plan),
                    fact.value_origin(),
                );
            });
        }
        checkpoint(&fixture, *fact, |lowerer, plan, current, anchor| {
            assert!(
                !numeric_exit(lowerer, plan, anchor, anchor),
                "a real same-typed birth pair is not exhaustion"
            );
            assert!(lowerer.emit_nullable_numeric_cleanup(*fact).unwrap());
            assert!(!lowerer.bindings.contains_key(&plan.root));
            assert!(!lowerer.non_null_bindings.contains_key(&plan.root));
            let ops = &lowerer.function.block(lowerer.block).unwrap().instructions;
            assert_eq!(
                lowerer
                    .function
                    .instruction(ops[ops.len() - 2])
                    .unwrap()
                    .operation,
                Operation::BorrowEnd { loan: current.loan }
            );
            assert_eq!(
                lowerer
                    .function
                    .instruction(ops[ops.len() - 1])
                    .unwrap()
                    .operation,
                Operation::Drop {
                    owner: current.owner
                }
            );
        });
    }
}
