//! Same-analysis actual binding links; normal products are never rewritten.
use super::*;
use lang_frontend::ownership_checking::{
    UnitDropTarget, UnitIterationCleanupAction, UnitLoanTarget, check_compilation_unit_ownership,
    legacy_ownership_eq, with_loan_observation,
};
const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner); consume(item) }";
#[test]
fn binding_lifecycle_requires_actual_causal_links_with_true_off() {
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "binding-links.ko", SOURCE);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "binding-links.ko", source, &parsed),
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
        assert!(off.ownership().loan_action_observations().is_empty());
        let run = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap();
        let mut outputs = Vec::new();
        for owned in [&on, &off] {
            let (lowered, routes) = crate::ssa::refine::observe_routing(|| {
                crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                    &sources,
                    &inputs,
                    &names,
                    &types,
                    &typed,
                    owned,
                    run.id(),
                )
            });
            let (program, entry) = lowered.unwrap();
            verify_program(&program).unwrap();
            verify_binding_ssa(&program, entry, &routes);
            outputs.push((
                crate::ssa::render_program(&program),
                crate::llvm::render_verified_program(&program).unwrap(),
            ));
        }
        assert_eq!(outputs[0], outputs[1]);
        let [observation] = on.ownership().loan_action_observations() else {
            panic!("one actual source catalog")
        };
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Partial);
        assert_eq!(observation.regions().len(), 2);
        assert!(observation.regions().iter().all(|r| {
            r.protocol_version() == 3 && r.coverage() == UnitLoanActionCoverage::Unsupported
        }));
        let actions = observation.actions();
        let [
            UnitLoanAction::Begin(source_loan),
            UnitLoanAction::BorrowHandoffCommitted(commit),
            UnitLoanAction::Begin(consumer),
            UnitLoanAction::BorrowBindingUseReturned(used),
            UnitLoanAction::BorrowBindingEnd(end),
            UnitLoanAction::BorrowBindingSourceRetired(retired),
            UnitLoanAction::Cleanup {
                point,
                action: UnitIterationCleanupAction::Drop(drop),
            },
        ] = actions
        else {
            panic!("actual binding use and source-retirement causal records are required")
        };
        let ids = (0..actions.len())
            .map(|i| observation.action_id(i).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(used.source_begin(), Some(ids[0].clone()));
        assert_eq!(used.handoff(), Some(ids[1].clone()));
        assert_eq!(used.begin(), Some(ids[2].clone()));
        assert_eq!(retired.source_begin(), Some(ids[0].clone()));
        assert_eq!(retired.handoff(), Some(ids[1].clone()));
        assert_eq!(retired.use_return(), Some(ids[3].clone()));
        assert_eq!(retired.binding_end(), Some(ids[4].clone()));
        for id in [
            used.source_begin(),
            used.handoff(),
            used.begin(),
            retired.use_return(),
            retired.binding_end(),
        ] {
            assert!(observation.action(&id.unwrap()).is_some());
        }
        assert_eq!(used.binding(), commit.binding());
        assert_eq!(used.loan(), consumer);
        assert_eq!(commit.source_loan(), source_loan);
        assert_eq!(end.binding(), commit.binding());
        assert_eq!(retired.transaction().binding(), end.binding());
        assert_eq!(retired.transaction().source_loan(), source_loan);
        assert_eq!(retired.transaction().point(), end.point());
        assert_eq!(*point, end.point());
        assert_eq!(source_loan.target(), consumer.target());
        let UnitLoanTarget::Place(place) = source_loan.target() else {
            panic!("actual owner root")
        };
        assert_ne!(place.root(), end.binding());
        assert_eq!(drop.target(), UnitDropTarget::Named(place.root()));
    }
}
fn verify_binding_ssa(program: &Program, entry: FunctionId, routes: &[String]) {
    let module = &program.modules[0];
    let caller = module.function(entry).unwrap();
    let [
        literal,
        root,
        begin,
        handoff,
        use_call,
        result_end,
        source_end,
        drop,
    ] = caller.instructions.as_slice()
    else {
        panic!("eight existing operations")
    };
    let [EntityId::Value(owner)] = literal.results.as_slice() else {
        panic!("formed owner")
    };
    let Operation::StringLiteral { bytes, .. } = &literal.operation else {
        panic!("actual literal")
    };
    assert_eq!(bytes, b"kept");
    let [EntityId::Place(place)] = root.results.as_slice() else {
        panic!("place")
    };
    let [EntityId::Loan(source)] = begin.results.as_slice() else {
        panic!("source loan")
    };
    let [EntityId::Loan(result)] = handoff.results.as_slice() else {
        panic!("binding result loan")
    };
    assert_ne!(source, result);
    assert_eq!(root.operation, Operation::RootPlace { owner: *owner });
    assert_eq!(
        begin.operation,
        Operation::BorrowBegin {
            place: *place,
            kind: LoanKind::Shared
        }
    );
    let Operation::BorrowCall {
        callee: view,
        arguments,
        source: actual,
    } = &handoff.operation
    else {
        panic!("actual borrowed return")
    };
    assert_eq!(*actual, *source);
    assert_eq!(arguments, &[EntityId::Loan(*source)]);
    let Operation::DirectCall {
        callee: consume,
        receiver,
        arguments,
    } = &use_call.operation
    else {
        panic!("ordinary consumer")
    };
    assert!(receiver.is_none());
    assert_eq!(arguments, &[EntityId::Loan(*result)]);
    assert!(use_call.results.is_empty());
    assert_eq!(result_end.operation, Operation::BorrowEnd { loan: *result });
    assert_eq!(source_end.operation, Operation::BorrowEnd { loan: *source });
    assert_eq!(drop.operation, Operation::Drop { owner: *owner });
    let view = module.function(*view).unwrap();
    let consume = module.function(*consume).unwrap();
    assert_eq!(routes, &[view.name.clone(), caller.name.clone()]);
    for callee in [view, consume] {
        let [EntityId::Loan(parameter)] = callee.blocks[0].parameters.as_slice() else {
            panic!("callee-local Shared loan")
        };
        assert_ne!(parameter.function(), source.function());
        assert_eq!(
            callee.entity(EntityId::Loan(*parameter)).unwrap().ty,
            caller.entity(EntityId::Loan(*source)).unwrap().ty
        );
    }
}

#[test]
fn binding_metadata_preserves_frontend_merge_and_true_off() {
    let text = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(flag: Boolean): Unit { val owner = \"kept\"; borrow val item = view(owner); if(flag) { consume(item) } else { consume(item) }; consume(item) }";
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "binding-merge.ko", text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "binding-merge.ko", source, &parsed),
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
        assert_eq!(on.ownership().borrow_binding_transports().len(), 1);
        assert_eq!(off.ownership().borrow_binding_transports().len(), 1);
        assert!(off.ownership().loan_action_observations().is_empty());
        let [observation] = on.ownership().loan_action_observations() else {
            panic!("one actual source catalog")
        };
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Partial);
        let retired = observation
            .actions()
            .iter()
            .filter_map(|action| match action {
                UnitLoanAction::BorrowBindingSourceRetired(retired) => Some(retired),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [retired] = retired.as_slice() else {
            panic!("one actual source retirement after merge")
        };
        assert!(
            retired.use_return().is_none(),
            "ambiguous earlier uses cannot be replaced by the final use"
        );
        // This ordinary source proves planner merge/off isolation only. Its if body
        // is outside the existing lowering subset; the straight-line test above
        // separately verifies the unchanged SSA and LLVM lifecycle.
    }
}
