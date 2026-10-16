//! Inner borrowed provider guard negatives from the ordinary original/Group source.
//! This module is a child of runtime solely to reach its private consumer method.
use super::super::super::cleanup_guard_harness::{self as harness, Fixture};
use super::super::{carrier, provider};
use super::*;
use crate::ssa::model::{BinaryOperator, InstructionId};

fn checkpoint<T>(
    fixture: &Fixture,
    fact: DropFact,
    check: impl FnOnce(&mut ExpressionLowerer<'_>, &mut Plan, &Birth, &mut provider::Provider) -> T,
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
        let proof = harness::outer_context(lowerer, plan.root, plan.outer);
        let birth = carrier::birth(lowerer, &plan, proof).unwrap();
        let mut provider = provider::shape(lowerer, &plan, &birth).unwrap();
        let owner = harness::before_cleanup(
            lowerer,
            provider.exhausted.block,
            plan.root,
            fact.value_origin(),
            Some(proof),
        );
        assert!(provider::backedges(lowerer, &plan, &birth, &provider).is_some());
        assert!(provider::complete(lowerer, &plan, &birth, &provider).is_some());
        assert_eq!(
            lowerer.checked_nullable_borrowed_owner(&plan).unwrap(),
            owner
        );
        check(lowerer, &mut plan, &birth, &mut provider)
    })
}

fn backedge_states(
    lowerer: &ExpressionLowerer<'_>,
    birth: &Birth,
    provider: &provider::Provider,
) -> Vec<(Columns, InstructionId, InstructionId)> {
    let columns = provider
        .header
        .entities
        .iter()
        .map(|&entity| harness::parameter(lowerer.function, entity).1)
        .collect::<Vec<_>>();
    harness::incoming(lowerer.function, provider.header.block)
        .into_iter()
        .filter(|(block, _)| *block != birth.pair.block)
        .map(|(block, edge)| {
            let ops = carrier::instructions(lowerer.function, block).unwrap();
            let end = ops
                .iter()
                .find(|instruction| matches!(instruction.operation, Operation::BorrowEnd { .. }))
                .unwrap();
            let Operation::BorrowEnd { loan } = end.operation else {
                unreachable!()
            };
            let add = ops
                .iter()
                .find(|instruction| {
                    matches!(
                        instruction.operation,
                        Operation::Binary {
                            operator: BinaryOperator::Add,
                            ..
                        }
                    )
                })
                .unwrap();
            let mut entities = columns
                .iter()
                .map(|column| edge.arguments[*column])
                .collect::<Vec<_>>();
            entities.push(EntityId::Loan(loan));
            let state = Columns { block, entities };
            assert!(transport(
                lowerer.function,
                state.clone(),
                &provider.body,
                Some((block, loan))
            ));
            (state, end.id, add.id)
        })
        .collect()
}

fn wrong_same_type(function: &Function, actual: EntityId) -> EntityId {
    let ty = function.entity(actual).unwrap().ty;
    function
        .blocks
        .iter()
        .flat_map(|block| block.parameters.iter())
        .chain(
            function
                .instructions
                .iter()
                .flat_map(|instruction| instruction.results.iter()),
        )
        .copied()
        .find(|&other| other != actual && function.entity(other).unwrap().ty == ty)
        .expect("different real SSA entity with exactly the same type")
}

fn append_effect(function: &mut Function, block: BlockId, operation: Operation, span: Span) {
    let term = function.blocks[block.index()].terminator.take();
    function
        .append_instruction(block, operation, Vec::new(), Origin::Source(span))
        .unwrap();
    function.blocks[block.index()].terminator = term;
}

fn assert_real_facts(fixture: &Fixture, plan: &Plan) {
    let iteration = fixture.owned.iteration(plan.statement).unwrap();
    let phis = iteration.closure_phis();
    assert_eq!(phis.len(), 4);
    let table = fixture.owned.cleanup_conditions();
    let span = fixture
        .parsed
        .ast()
        .statements()
        .get(plan.statement)
        .unwrap()
        .span();
    for symbol in [plan.root, plan.source_root] {
        for boundary in [IterationPhiBoundary::Header, IterationPhiBoundary::Exit] {
            let matches = phis
                .iter()
                .filter(|phi| phi.symbol() == symbol && phi.boundary() == boundary)
                .collect::<Vec<_>>();
            assert_eq!(matches.len(), 1);
            let phi = matches[0];
            assert!(
                matches!(table.owner_value(phi.owner()), Some(CleanupOwnerValue::IterationPhi { statement, boundary: actual, symbol: root, origin }) if *statement == plan.statement && *actual == boundary && *root == symbol && *origin == span)
            );
            let slot = phis
                .iter()
                .take_while(|other| other.owner() != phi.owner())
                .filter(|other| other.boundary() == boundary)
                .count();
            assert_eq!(
                table
                    .selector(phi.availability_selector())
                    .unwrap()
                    .source(),
                CleanupSelectorSource::IterationPhi {
                    statement: plan.statement,
                    boundary,
                    slot
                }
            );
        }
    }
    let current_exit = phis
        .iter()
        .find(|phi| phi.symbol() == plan.root && phi.boundary() == IterationPhiBoundary::Exit)
        .unwrap();
    let items_header = phis
        .iter()
        .find(|phi| {
            phi.symbol() == plan.source_root && phi.boundary() == IterationPhiBoundary::Header
        })
        .unwrap();
    assert_eq!(plan.fact.owner(), Some(current_exit.owner()));
    assert_eq!(
        plan.fact.condition(),
        Some(current_exit.availability_condition())
    );
    assert_eq!(plan.source_drop.owner(), Some(items_header.owner()));
    assert_eq!(
        plan.source_drop.condition(),
        Some(items_header.availability_condition())
    );
    assert_eq!(
        plan.source_drop.point(),
        DropPoint::LoopExit(plan.statement)
    );
    assert_ne!(plan.fact.owner(), plan.source_drop.owner());
    for incoming in iteration.closure_phi_incomings() {
        assert_eq!(incoming.bindings().len(), 2);
        for binding in incoming.bindings() {
            let target = phis
                .iter()
                .find(|phi| phi.owner() == binding.target())
                .unwrap();
            assert_eq!(target.boundary(), incoming.boundary());
            assert_eq!(
                binding.availability_selector(),
                target.availability_selector()
            );
        }
    }
    // The frontend owns these fields. No foreign-crate phi/condition mutation or
    // fabricated fact is used; all other genuine DropFacts must fail admission.
    for &fact in fixture
        .owned
        .drops()
        .iter()
        .filter(|fact| **fact != plan.fact)
    {
        assert!(
            candidate(
                &fixture.parsed,
                &fixture.names,
                &fixture.typed,
                &fixture.owned,
                fact
            )
            .is_none()
        );
    }
}

#[test]
fn inner_borrowed_provider_rejects_role_and_cleanup_corruption() {
    let sources = crate::ssa::while_flow_tests::proof_cases::borrowed_proof_cases::borrowed_for_inside_while_sources();
    for text in sources {
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
            panic!("exact real final current Exit fact")
        };
        let plan = candidate(
            &fixture.parsed,
            &fixture.names,
            &fixture.typed,
            &fixture.owned,
            *fact,
        )
        .unwrap();
        assert_real_facts(&fixture, &plan);
        for role in 0..5 {
            for edge_index in 0..2 {
                checkpoint(&fixture, *fact, |lowerer, plan, birth, provider| {
                    let states = backedge_states(lowerer, birth, provider);
                    assert_eq!(states.len(), 2, "real continue and fallthrough backedges");
                    let (mut state, _, _) = states[edge_index].clone();
                    let element = provider::loan(state.entities[4]).unwrap();
                    state.entities[role] = wrong_same_type(lowerer.function, state.entities[role]);
                    assert!(
                        !transport(
                            lowerer.function,
                            state.clone(),
                            &provider.body,
                            Some((state.block, element))
                        ),
                        "same-type role swap {role}"
                    );
                    // Mutate the actual transport column too; all identities remain valid.
                    let actual = states[edge_index].0.entities[role];
                    let wrong = state.entities[role];
                    let block = states[edge_index].0.block;
                    if role < 4 {
                        let column =
                            harness::parameter(lowerer.function, provider.header.entities[role]).1;
                        let TerminatorKind::Branch(edge) = &mut lowerer.function.blocks
                            [block.index()]
                        .terminator
                        .as_mut()
                        .unwrap()
                        .kind
                        else {
                            unreachable!()
                        };
                        assert_eq!(edge.arguments[column], actual);
                        edge.arguments[column] = wrong;
                    } else {
                        lowerer.function.instructions[states[edge_index].1.index()].operation =
                            Operation::BorrowEnd {
                                loan: provider::loan(wrong).unwrap(),
                            };
                    }
                    assert!(provider::backedges(lowerer, plan, birth, provider).is_none());
                    harness::unsupported(
                        lowerer.checked_nullable_borrowed_owner(plan),
                        fact.value_origin(),
                    );
                });
            }
            checkpoint(&fixture, *fact, |lowerer, plan, birth, provider| {
                let states = backedge_states(lowerer, birth, provider);
                let (state, _, _) = &states[0];
                let element = provider::loan(state.entities[4]).unwrap();
                let operation = match provider.body.entities[role] {
                    EntityId::Value(owner) => Operation::Drop { owner },
                    EntityId::Loan(loan) => Operation::BorrowEnd { loan },
                    _ => unreachable!(),
                };
                append_effect(
                    lowerer.function,
                    provider.body.block,
                    operation,
                    fact.value_origin(),
                );
                assert!(
                    !transport(
                        lowerer.function,
                        state.clone(),
                        &provider.body,
                        Some((state.block, element))
                    ),
                    "early consumption for role {role}"
                );
                assert!(provider::backedges(lowerer, plan, birth, provider).is_none());
            });
        }
        for mutation in 0..11 {
            checkpoint(&fixture, *fact, |lowerer, plan, birth, provider| {
                let states = backedge_states(lowerer, birth, provider);
                let (state, end, add) = &states[0];
                let block = state.block;
                let element = provider::loan(state.entities[4]).unwrap();
                assert!(!transport(
                    lowerer.function,
                    state.clone(),
                    &provider.body,
                    None
                ));
                assert!(!transport(
                    lowerer.function,
                    state.clone(),
                    &provider.body,
                    Some((provider.body.block, element))
                ));
                assert!(!transport(
                    lowerer.function,
                    state.clone(),
                    &provider.body,
                    Some((block, provider::loan(state.entities[3]).unwrap()))
                ));
                match mutation {
                    0 => lowerer.function.blocks[block.index()]
                        .instructions
                        .retain(|id| id != end),
                    1 => lowerer.function.blocks[block.index()]
                        .instructions
                        .push(*end),
                    2 => {
                        lowerer.function.instructions[end.index()].operation =
                            Operation::BorrowEnd {
                                loan: provider::loan(state.entities[3]).unwrap(),
                            }
                    }
                    3 => {
                        let TerminatorKind::Branch(edge) = &mut lowerer.function.blocks
                            [block.index()]
                        .terminator
                        .as_mut()
                        .unwrap()
                        .kind
                        else {
                            unreachable!()
                        };
                        edge.arguments.push(EntityId::Loan(element));
                    }
                    4 => {
                        let ops = &mut lowerer.function.blocks[block.index()].instructions;
                        let a = ops.iter().position(|id| id == end).unwrap();
                        let b = ops.iter().position(|id| id == add).unwrap();
                        assert!(a < b);
                        ops.swap(a, b);
                    }
                    5 => {
                        lowerer.function.instructions[add.index()].operation = Operation::Binary {
                            operator: BinaryOperator::Add,
                            left: provider.body_cursor,
                            right: provider.length,
                        }
                    }
                    6 => {
                        lowerer.function.instructions[add.index()].operation = Operation::Binary {
                            operator: BinaryOperator::Add,
                            left: provider.cursor,
                            right: provider.step,
                        }
                    }
                    7 => lowerer.function.blocks[birth.pair.block.index()].terminator = None,
                    8 => {
                        let term = lowerer.function.blocks[block.index()]
                            .terminator
                            .clone()
                            .unwrap();
                        let unrelated = lowerer
                            .function
                            .blocks
                            .iter()
                            .find(|candidate| {
                                candidate.id != lowerer.block
                                    && candidate.terminator.as_ref().is_some_and(|term| {
                                        matches!(term.kind, TerminatorKind::Return { .. })
                                    })
                            })
                            .unwrap()
                            .id;
                        lowerer.function.blocks[unrelated.index()].terminator = Some(term);
                    }
                    9 => provider.step = provider.length,
                    10 => {
                        lowerer.function.blocks[block.index()]
                            .terminator
                            .as_mut()
                            .unwrap()
                            .origin = Origin::Source(fact.value_origin())
                    }
                    _ => unreachable!(),
                }
                assert!(
                    provider::backedges(lowerer, plan, birth, provider).is_none(),
                    "backedge corruption {mutation}"
                );
                if mutation != 9 {
                    harness::unsupported(
                        lowerer.checked_nullable_borrowed_owner(plan),
                        fact.value_origin(),
                    );
                }
            });
        }
        for mutation in 0..13 {
            checkpoint(&fixture, *fact, |lowerer, plan, birth, provider| {
                let block = provider.exhausted.block;
                let source = provider::loan(provider.exhausted.entities[3]).unwrap();
                let owner = provider::value(provider.exhausted.entities[2]).unwrap();
                let ops = carrier::instructions(lowerer.function, block).unwrap();
                let end = ops
                    .iter()
                    .find(|op| op.operation == Operation::BorrowEnd { loan: source })
                    .unwrap()
                    .id;
                let drop = ops
                    .iter()
                    .find(|op| op.operation == Operation::Drop { owner })
                    .unwrap()
                    .id;
                match mutation {
                    0 => {
                        let ops = &mut lowerer.function.blocks[block.index()].instructions;
                        let e = ops.iter().position(|id| *id == end).unwrap();
                        let d = ops.iter().position(|id| *id == drop).unwrap();
                        assert!(e < d, "real EndSource precedes items Drop");
                        ops.swap(e, d);
                    }
                    1 => lowerer.function.blocks[block.index()]
                        .instructions
                        .retain(|id| *id != end),
                    2 => lowerer.function.blocks[block.index()]
                        .instructions
                        .retain(|id| *id != drop),
                    3 => {
                        lowerer.function.instructions[end.index()].operation =
                            Operation::BorrowEnd {
                                loan: provider.source,
                            }
                    }
                    4 => {
                        lowerer.function.instructions[drop.index()].operation = Operation::Drop {
                            owner: birth.source_owner,
                        }
                    }
                    5 => {
                        lowerer
                            .bindings
                            .insert(plan.source_root, LoweredValue::Value(owner));
                    }
                    6 => {
                        lowerer.borrow_bindings.insert(
                            plan.element,
                            provider::loan(provider.body.entities[4]).unwrap(),
                        );
                    }
                    7 => append_effect(
                        lowerer.function,
                        block,
                        Operation::ContainerLength {
                            owner: EntityId::Loan(source),
                        },
                        fact.value_origin(),
                    ),
                    8 => lowerer.function.blocks[block.index()]
                        .instructions
                        .push(end),
                    9 => lowerer.function.blocks[block.index()]
                        .instructions
                        .push(drop),
                    10 => {
                        lowerer.function.instructions[end.index()].origin =
                            Origin::Source(fact.value_origin())
                    }
                    11 => {
                        lowerer.function.instructions[drop.index()].origin =
                            Origin::Source(fact.value_origin())
                    }
                    12 => append_effect(
                        lowerer.function,
                        block,
                        Operation::RootPlace { owner },
                        fact.value_origin(),
                    ),
                    _ => unreachable!(),
                }
                assert!(
                    provider::complete(lowerer, plan, birth, provider).is_none(),
                    "completion corruption {mutation}"
                );
                harness::unsupported(
                    lowerer.checked_nullable_borrowed_owner(plan),
                    fact.value_origin(),
                );
            });
        }
        for mutation in 0..6 {
            checkpoint(&fixture, *fact, |lowerer, plan, birth, provider| {
                let proof = lowerer.non_null_bindings[&plan.root];
                match mutation {
                    0 => plan.root = fixture.symbol("initial"),
                    1 => plan.source_root = plan.root,
                    2 => plan.outer = plan.statement,
                    3 => plan.storage = fixture.typed.symbol_type(plan.source_root).unwrap(),
                    4 => plan.statement = plan.outer,
                    5 => plan.source_drop = plan.fact,
                    _ => unreachable!(),
                }
                if mutation < 4 {
                    assert!(carrier::birth(lowerer, plan, proof).is_none());
                }
                if mutation == 4 {
                    assert!(provider::shape(lowerer, plan, birth).is_none());
                }
                if mutation == 5 {
                    assert!(provider::complete(lowerer, plan, birth, provider).is_none());
                }
                harness::unsupported(
                    lowerer.checked_nullable_borrowed_owner(plan),
                    fact.value_origin(),
                );
            });
        }
        checkpoint(&fixture, *fact, |lowerer, plan, _, _| {
            let proof = lowerer.non_null_bindings[&plan.root];
            assert!(lowerer.emit_nullable_borrowed_cleanup(*fact).unwrap());
            assert!(!lowerer.bindings.contains_key(&plan.root));
            assert!(!lowerer.bindings.contains_key(&plan.source_root));
            assert!(!lowerer.borrow_bindings.contains_key(&plan.element));
            assert!(!lowerer.non_null_bindings.contains_key(&plan.root));
            let ops = &lowerer.function.block(lowerer.block).unwrap().instructions;
            assert_eq!(
                lowerer
                    .function
                    .instruction(ops[ops.len() - 2])
                    .unwrap()
                    .operation,
                Operation::BorrowEnd { loan: proof.loan }
            );
            assert_eq!(
                lowerer
                    .function
                    .instruction(ops[ops.len() - 1])
                    .unwrap()
                    .operation,
                Operation::Drop { owner: proof.owner }
            );
        });
    }
}
