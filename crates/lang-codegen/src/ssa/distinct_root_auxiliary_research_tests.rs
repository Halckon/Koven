//! Independent two-root retirement evidence preserves the existing lowering and same-root boundary.
use super::*;
use lang_frontend::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction, UnitLoanAction, UnitLoanTarget,
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};

#[test]
fn ordinary_distinct_root_auxiliary_research_preserves_actual_source_and_retirement() {
    for (source_index, keep_aux) in [(0, false), (0, true), (1, false), (1, true)] {
        let (parameters, arguments) = if source_index == 0 {
            ("source: String, auxiliary: String", "owner, auxiliary")
        } else {
            ("auxiliary: String, source: String", "auxiliary, owner")
        };
        let later = if keep_aux { "; consume(auxiliary)" } else { "" };
        let text = format!(
            "fun view({parameters}): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun run(): Unit {{val owner = \"kept\"; val auxiliary = \"other\"; borrow val item = view({arguments}); consume(item){later}}}"
        );
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "distinct-root-research.ko", &text);
        let (environment, types) = standard_environments();
        for (layout, input) in [
            (
                "rooted",
                SourceUnitInput::new("root", "distinct-root-research.ko", source, &parsed),
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
            assert_eq!(
                on.ownership().owner_observations(),
                off.ownership().owner_observations()
            );
            assert!(off.ownership().loan_action_observations().is_empty());
            let [handoff] = on.ownership().borrow_call_handoffs() else {
                panic!("one real handoff")
            };
            let [aux] = handoff.auxiliary_loan_ends() else {
                panic!("one actual auxiliary retirement")
            };
            let origin = handoff.source_loan();
            assert_ne!(origin, aux);
            assert_eq!(origin.call(), aux.call());
            assert!(
                matches!((origin.target(),aux.target()),(UnitLoanTarget::Place(a),UnitLoanTarget::Place(b)) if a.is_root()&&b.is_root()&&a.root()!=b.root())
            );
            for catalog in on.ownership().loan_action_observations() {
                assert!(catalog.auxiliary_binding_regions().is_empty());
                assert!(catalog.auxiliary_retirements().is_empty());
            }
            let [catalog] = on.ownership().loan_action_observations() else {
                panic!("one actual catalog");
            };
            assert_eq!(
                catalog.distinct_auxiliary_retirements().len(),
                1,
                "source-index={source_index}/{layout}: distinct-root relation"
            );
            let relation = &catalog.distinct_auxiliary_retirements()[0];
            assert!(catalog.owns_distinct_auxiliary_retirement(relation));
            assert_eq!(relation.source_parameter(), source_index);
            assert_eq!(relation.point(), UnitDropPoint::CallReturn(origin.call()));
            assert_ne!(relation.source_owner(), relation.auxiliary_owner());
            assert_eq!(relation.auxiliary_drop().is_none(), keep_aux);
            if let Some(drop) = relation.auxiliary_drop() {
                let UnitLoanTarget::Place(place) = aux.target() else {
                    panic!("aux root");
                };
                assert!(
                    matches!(catalog.action(&drop), Some(UnitLoanAction::Cleanup { point, action: UnitIterationCleanupAction::Drop(fact) })
                    if *point == relation.point() && fact.point() == relation.point() && fact.target() == UnitDropTarget::Named(place.root()))
                );
            }

            assert_ne!(relation.source_begin(), relation.auxiliary_begin());
            assert_eq!(
                catalog.action(&relation.source_begin()),
                Some(&UnitLoanAction::Begin(origin.clone()))
            );
            assert_eq!(
                catalog.action(&relation.auxiliary_begin()),
                Some(&UnitLoanAction::Begin(aux.clone()))
            );
            assert_eq!(
                catalog.action(&relation.handoff()),
                Some(&UnitLoanAction::BorrowHandoffCommitted(handoff.clone()))
            );
            let position = |wanted| {
                let indices = catalog
                    .actions()
                    .iter()
                    .enumerate()
                    .filter(|(_, actual)| **actual == wanted)
                    .map(|(index, _)| index)
                    .collect::<Vec<_>>();
                assert_eq!(indices.len(), 1);
                indices[0]
            };
            let source_begin = position(UnitLoanAction::Begin(origin.clone()));
            let auxiliary_begin = position(UnitLoanAction::Begin(aux.clone()));
            let committed = position(UnitLoanAction::BorrowHandoffCommitted(handoff.clone()));
            assert_eq!(source_begin < auxiliary_begin, source_index == 0);
            assert!(source_begin < committed && auxiliary_begin < committed);

            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap()
                .id();
            let mut previous = None;
            for (setting, owned) in [("on", &on), ("off", &off)] {
                let ((lowered, verification), receipts) =
                    crate::ssa::refine::aux_binding_receipt::observe(|| {
                        crate::ssa::unit_lower::observe_final_verifier(|| {
                            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                &sources, &inputs, &names, &types, &typed, owned, entry,
                            )
                        })
                    });
                let (program, entry) = lowered.unwrap_or_else(|error| {
                    panic!("source-index={source_index}/{layout}/{setting}: {error:?}")
                });
                assert_eq!(verification.len(), 1);
                assert!(receipts.is_empty());
                verify_program(&program).unwrap();
                check(&program, entry, source_index, keep_aux);
                let output = (
                    crate::ssa::render_program(&program),
                    crate::llvm::render_verified_program(&program).unwrap(),
                );
                if setting == "off" {
                    assert_eq!(previous.as_ref(), Some(&output));
                } else {
                    previous = Some(output);
                }
            }
        }
    }
}
fn check(program: &Program, entry: FunctionId, source_index: usize, keep_aux: bool) {
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
        panic!("one actual borrowed call")
    };
    let Operation::BorrowCall {
        callee,
        arguments,
        source,
    } = &call.operation
    else {
        unreachable!()
    };
    let [EntityId::Loan(a), EntityId::Loan(b)] = arguments.as_slice() else {
        panic!("two loans")
    };
    let auxiliary = if source_index == 0 { *b } else { *a };
    assert_eq!(*source, if source_index == 0 { *a } else { *b });
    assert_ne!(*source, auxiliary);
    let [EntityId::Loan(result)] = call.results.as_slice() else {
        panic!("one result")
    };
    let view = module.function(*callee).unwrap();
    assert_eq!(view.borrow_return, Some(source_index));
    let EntityId::Loan(returned) = view.blocks[0].parameters[source_index] else {
        panic!("source parameter")
    };
    assert_eq!(
        view.blocks[0].terminator.as_ref().unwrap().kind,
        TerminatorKind::BorrowReturn { loan: returned }
    );
    let mut roots = Vec::new();
    for loan in [*source, auxiliary] {
        let (index, begin) = caller
            .instructions
            .iter()
            .enumerate()
            .find(|(_, i)| i.results == [EntityId::Loan(loan)])
            .unwrap();
        assert!(index < *call_index);
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = begin.operation
        else {
            panic!("real shared begin")
        };
        let root = caller
            .instructions
            .iter()
            .find(|i| i.results == [EntityId::Place(place)])
            .unwrap();
        let Operation::RootPlace { owner } = root.operation else {
            panic!("real root")
        };
        roots.push(owner);
    }
    assert_ne!(roots[0], roots[1]);
    let end = |loan| {
        let ends = caller
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| i.operation == Operation::BorrowEnd { loan })
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        assert_eq!(ends.len(), 1);
        ends[0]
    };
    let (aux_end, result_end, source_end) = (end(auxiliary), end(*result), end(*source));
    let uses=caller.instructions.iter().enumerate().filter(|(_,i)|matches!(&i.operation,Operation::DirectCall{arguments,..} if arguments.contains(&EntityId::Loan(*result)))).map(|(i,_)|i).collect::<Vec<_>>();
    let [consumer] = uses.as_slice() else {
        panic!("one actual consumer")
    };
    assert!(
        *call_index < aux_end
            && aux_end < *consumer
            && *consumer < result_end
            && result_end < source_end
    );
    for (root, loan_end) in roots.iter().zip([source_end, aux_end]) {
        let drops = caller
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i.operation,Operation::Drop{owner} if owner==*root))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), 1);
        assert!(loan_end < drops[0]);
        if *root == roots[1] {
            assert!(
                (drops[0] > *consumer) == keep_aux,
                "auxiliary Drop follows its actual last use, not an assumed commit-live obligation"
            );
        } else {
            assert!(*consumer < drops[0]);
        }
    }
}
