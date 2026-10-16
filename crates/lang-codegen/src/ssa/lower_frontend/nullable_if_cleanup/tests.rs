//! Direct private guards; mutations never use the final verifier as an oracle.
use super::super::cleanup_guard_harness::{self as harness, Fixture};
use super::super::non_null_proof::ProofScope;
use super::*;

fn checkpoint<T>(
    fixture: &Fixture,
    fact: DropFact,
    check: impl FnOnce(&mut ExpressionLowerer<'_>, &mut GroupPlan, ValueId) -> T,
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
        harness::outer_context(lowerer, plan.root, plan.while_statement);
        let span = lowerer.expression_span(plan.second_if).unwrap();
        let (block, proof) = lowerer
            .function
            .blocks
            .iter()
            .find_map(|block| {
                let term = block.terminator.as_ref()?;
                if term.origin != Origin::Source(span) {
                    return None;
                }
                let TerminatorKind::NullableBranch {
                    owner,
                    view,
                    when_non_null,
                    when_null,
                } = &term.kind
                else {
                    return None;
                };
                let block = if plan.arm == 0 {
                    when_non_null.target
                } else {
                    when_null.target
                };
                let column = harness::parameter(lowerer.function, EntityId::Value(*owner)).1;
                let EntityId::Value(owner) = lowerer.function.block(block)?.parameters[column]
                else {
                    panic!("owner column")
                };
                let EntityType::Loan { target: inner, .. } =
                    lowerer.function.entity(EntityId::Loan(*view))?.ty
                else {
                    panic!("view type")
                };
                Some((
                    block,
                    (plan.arm == 0).then(|| {
                        NonNullProof::introduced(
                            *view,
                            owner,
                            inner,
                            ProofScope::If(plan.second_if),
                        )
                    }),
                ))
            })
            .unwrap();
        let owner = harness::before_cleanup(
            lowerer,
            block,
            plan.root,
            plan.members[0].value_origin(),
            proof,
        );
        assert_eq!(
            lowerer
                .checked_nullable_if_cleanup_owner(&plan, &plan.members)
                .unwrap(),
            owner,
            "unmodified checkpoint first passes the exact consumer"
        );
        check(lowerer, &mut plan, owner)
    })
}

#[test]
fn nullable_if_cleanup_rejects_coverage_consumer_and_owner_corruption() {
    let original = crate::ssa::while_flow_tests::proof_cases::retested_after_branch_source();
    for text in [
        original.clone(),
        original.replace("if (current != null)", "if (((current) != null))"),
    ] {
        let fixture = Fixture::analyze(&text);
        // These guards inspect source/facts/SSA and the explicit local state only;
        // no generic instance, callable token, closure, or receiver path is exercised.
        assert_eq!(fixture.program.modules[0].functions.len(), 1);
        assert!(fixture.entry().receiver.is_none());
        assert!(fixture.typed.type_parameters().is_empty());
        assert!(
            fixture
                .parsed
                .ast()
                .expressions()
                .iter()
                .all(|(_, node)| !matches!(node.payload(), Expression::Lambda { .. }))
        );
        assert!(
            fixture
                .entry()
                .instructions
                .iter()
                .all(|instruction| !matches!(instruction.operation, Operation::DirectCall { .. }))
        );
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
                .is_some_and(|plan| plan.members[0] == fact)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            facts.len(),
            2,
            "both actual retest arms publish a two-member group"
        );
        for &fact in &facts {
            let plan = candidate(
                &fixture.parsed,
                &fixture.names,
                &fixture.typed,
                &fixture.owned,
                fact,
            )
            .unwrap();
            let conditions = fixture.owned.cleanup_conditions();
            assert_eq!(
                coverage(&fixture.parsed, conditions, plan.members),
                Some((plan.first_if, plan.second_if, plan.arm))
            );
            assert!(coverage(&fixture.parsed, conditions, [plan.members[0]; 2]).is_none());
            assert!(
                coverage(
                    &fixture.parsed,
                    conditions,
                    [plan.members[1], plan.members[0]]
                )
                .is_none()
            );
            let other = candidate(
                &fixture.parsed,
                &fixture.names,
                &fixture.typed,
                &fixture.owned,
                *facts.iter().find(|&&other| other != fact).unwrap(),
            )
            .unwrap();
            assert_ne!(plan.arm, other.arm);
            assert!(
                coverage(
                    &fixture.parsed,
                    conditions,
                    [plan.members[0], other.members[1]]
                )
                .is_none(),
                "real opposite-arm selector branches cannot complete this cover"
            );
            assert!(
                candidate(
                    &fixture.parsed,
                    &fixture.names,
                    &fixture.typed,
                    &fixture.owned,
                    plan.old_drop
                )
                .is_none()
            );
            for remaining in [
                vec![],
                vec![plan.members[0]],
                vec![plan.members[0]; 2],
                vec![plan.members[1], plan.members[0]],
                vec![plan.members[0], other.members[1]],
            ] {
                checkpoint(&fixture, fact, |lowerer, plan, _| {
                    let before = lowerer.function.clone();
                    harness::unsupported(
                        lowerer.checked_nullable_if_cleanup_owner(plan, &remaining),
                        plan.members[0].value_origin(),
                    );
                    assert_eq!(
                        *lowerer.function, before,
                        "preflight failure has no side effect"
                    );
                });
            }
            for mutation in 0..7 {
                checkpoint(&fixture, fact, |lowerer, plan, _| {
                    let span = plan.members[0].value_origin();
                    match mutation {
                        0 => plan.root = fixture.symbol("initial"),
                        1 => plan.arm = 1 - plan.arm,
                        2 => plan.replacement = plan.second_if,
                        3 => plan.first_if = plan.second_if,
                        4 | 5 => {
                            let block = lowerer.block;
                            let (join, edge) =
                                harness::incoming(lowerer.function, block).pop().unwrap();
                            let EntityId::Value(current) = lowerer.function.block(block).unwrap().parameters.iter().copied().find(|entity| matches!(entity, EntityId::Value(value) if lowerer.bindings[&plan.root] == LoweredValue::Value(*value))).unwrap() else { unreachable!() };
                            let column =
                                harness::parameter(lowerer.function, EntityId::Value(current)).1;
                            let seed = lowerer
                                .function
                                .block(lowerer.function.entry_block().unwrap())
                                .unwrap()
                                .parameters[plan.seed_parameter];
                            let target = harness::edges_mut(
                                &mut lowerer.function.blocks[join.index()]
                                    .terminator
                                    .as_mut()
                                    .unwrap()
                                    .kind,
                            )
                            .into_iter()
                            .find(|target| target.target == block)
                            .unwrap();
                            if mutation == 4 {
                                target.arguments[column] = seed;
                            } else {
                                target.arguments.push(edge.arguments[column]);
                            }
                        }
                        6 => {
                            let (_, edge) = harness::incoming(lowerer.function, lowerer.block)
                                .pop()
                                .unwrap();
                            let unrelated = lowerer
                                .function
                                .blocks
                                .iter()
                                .find(|block| {
                                    block.id != lowerer.block
                                        && block.terminator.as_ref().is_some_and(|term| {
                                            matches!(term.kind, TerminatorKind::Return { .. })
                                        })
                                })
                                .unwrap()
                                .id;
                            lowerer.function.blocks[unrelated.index()]
                                .terminator
                                .as_mut()
                                .unwrap()
                                .kind = TerminatorKind::Branch(edge);
                        }
                        _ => unreachable!(),
                    }
                    assert!(
                        lowerer.nullable_if_cleanup_owner(plan).is_none(),
                        "carrier mutation {mutation}"
                    );
                    harness::unsupported(
                        lowerer.checked_nullable_if_cleanup_owner(plan, &plan.members),
                        span,
                    );
                });
            }
            checkpoint(&fixture, fact, |lowerer, plan, owner| {
                assert_eq!(
                    lowerer
                        .emit_nullable_if_cleanup_group(&plan.members)
                        .unwrap(),
                    Some(2)
                );
                assert!(!lowerer.bindings.contains_key(&plan.root));
                assert!(!lowerer.non_null_bindings.contains_key(&plan.root));
                let block = lowerer.function.block(lowerer.block).unwrap();
                let drop = lowerer
                    .function
                    .instruction(*block.instructions.last().unwrap())
                    .unwrap();
                assert_eq!(drop.operation, Operation::Drop { owner });
                assert_eq!(drop.origin, Origin::Source(plan.members[0].value_origin()));
            });
        }
    }
}
