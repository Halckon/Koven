//! Actual same-catalog parent/child links, with unchanged on/off lowering products.
use super::*;
use lang_frontend::ownership_checking::{
    UnitDropTarget, UnitIterationCleanupAction, UnitLoanTarget, check_compilation_unit_ownership,
    legacy_ownership_eq, with_loan_observation,
};

const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val parent = view(owner); borrow val child = parent; consume(child) }";

#[test]
fn parent_child_requires_actual_partial_causal_links_with_true_off() {
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "parent-child-links.ko", SOURCE);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "parent-child-links.ko", source, &parsed),
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
            verify_parent_child_ssa(&program, entry, &routes);
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
        assert!(observation.binding_regions().is_empty());
        assert_eq!(observation.regions().len(), 2);
        assert!(observation.regions().iter().all(|region| {
            region.protocol_version() == 3
                && region.coverage() == UnitLoanActionCoverage::Unsupported
        }));
        let actions = observation.actions();
        let [
            UnitLoanAction::Begin(source_loan),
            UnitLoanAction::BorrowHandoffCommitted(parent_commit),
            UnitLoanAction::BorrowBindingChildCommitted(child_commit),
            UnitLoanAction::Begin(consumer),
            UnitLoanAction::BorrowBindingChildUseReturned(used),
            UnitLoanAction::BorrowBindingEnd(child_end),
            UnitLoanAction::BorrowBindingChildRetired(child_retired),
            UnitLoanAction::BorrowBindingEnd(parent_end),
            UnitLoanAction::BorrowBindingSourceRetired(parent_retired),
            UnitLoanAction::Cleanup {
                point,
                action: UnitIterationCleanupAction::Drop(drop),
            },
        ] = actions
        else {
            panic!(
                "ten actual parent-child causal actions are required; got {}",
                actions.len()
            )
        };
        let ids = (0..actions.len())
            .map(|i| observation.action_id(i).unwrap())
            .collect::<Vec<_>>();
        for (reference, index) in [
            (child_commit.source_begin(), 0),
            (child_commit.parent_handoff(), 1),
            (used.source_begin(), 0),
            (used.parent_handoff(), 1),
            (used.commit(), 2),
            (used.begin(), 3),
            (child_retired.source_begin(), 0),
            (child_retired.parent_handoff(), 1),
            (child_retired.commit(), 2),
            (child_retired.use_return(), 4),
            (child_retired.binding_end(), 5),
            (parent_retired.source_begin(), 0),
            (parent_retired.handoff(), 1),
            (parent_retired.binding_end(), 7),
        ] {
            assert_eq!(reference, Some(ids[index].clone()));
            assert_eq!(
                observation.action(&reference.unwrap()),
                Some(&actions[index])
            );
        }
        assert_eq!(parent_retired.use_return(), None);
        assert_eq!(parent_commit.source_loan(), source_loan);
        assert_eq!(child_commit.parent(), parent_commit.binding());
        assert_ne!(child_commit.binding(), child_commit.parent());
        let child_fact = on
            .ownership()
            .borrow_results()
            .bindings()
            .iter()
            .find(|fact| fact.binding() == child_commit.binding())
            .unwrap();
        assert!(child_fact.source_loan().is_none());
        for owned in [&on, &off] {
            let [transport] = owned.ownership().borrow_binding_transports() else {
                panic!("only the parent owns an original source transport")
            };
            assert_eq!(transport.binding(), parent_commit.binding());
        }
        assert_eq!(child_fact.parent(), Some(child_commit.parent()));
        assert_eq!(child_fact.initializer(), child_commit.initializer());
        assert_eq!(used.binding(), child_commit.binding());
        assert_eq!(used.parent(), child_commit.parent());
        assert_eq!(used.loan(), consumer);
        assert_eq!(child_end.binding(), child_commit.binding());
        assert_eq!(child_retired.binding(), child_commit.binding());
        assert_eq!(child_retired.parent(), child_commit.parent());
        assert_eq!(child_retired.point(), child_end.point());
        assert_eq!(parent_end.binding(), parent_commit.binding());
        assert_eq!(parent_retired.transaction().binding(), parent_end.binding());
        assert_eq!(parent_retired.transaction().source_loan(), source_loan);
        assert_eq!(parent_retired.transaction().point(), parent_end.point());
        assert_eq!(child_end.point(), parent_end.point());
        assert_eq!(*point, parent_end.point());
        assert_eq!(drop.point(), *point);
        let UnitLoanTarget::Place(owner) = source_loan.target() else {
            panic!("actual owner root")
        };
        assert!(owner.is_root());
        assert_ne!(owner.root(), child_commit.parent());
        assert_ne!(owner.root(), child_commit.binding());
        assert_eq!(drop.target(), UnitDropTarget::Named(owner.root()));
    }
}

fn verify_parent_child_ssa(program: &Program, entry: FunctionId, routes: &[String]) {
    let module = &program.modules[0];
    let caller = module.function(entry).unwrap();
    let [
        literal,
        root,
        begin,
        handoff,
        reborrow,
        use_call,
        child_end,
        parent_end,
        source_end,
        drop,
    ] = caller.instructions.as_slice()
    else {
        panic!("ten original SSA operations")
    };
    let Operation::StringLiteral { bytes, .. } = &literal.operation else {
        panic!("literal")
    };
    assert_eq!(bytes, b"kept");
    let [EntityId::Value(owner)] = literal.results.as_slice() else {
        panic!("owner")
    };
    let [EntityId::Place(place)] = root.results.as_slice() else {
        panic!("place")
    };
    let [EntityId::Loan(source)] = begin.results.as_slice() else {
        panic!("source loan")
    };
    let [EntityId::Loan(parent)] = handoff.results.as_slice() else {
        panic!("parent result")
    };
    let [EntityId::Loan(child)] = reborrow.results.as_slice() else {
        panic!("child result")
    };
    assert_ne!(source, parent);
    assert_ne!(source, child);
    assert_ne!(parent, child);
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
        panic!("borrowed return")
    };
    assert_eq!(*actual, *source);
    assert_eq!(arguments, &[EntityId::Loan(*source)]);
    assert_eq!(
        reborrow.operation,
        Operation::SharedReborrow { source: *parent }
    );
    let Operation::DirectCall {
        callee: consume,
        receiver,
        arguments,
    } = &use_call.operation
    else {
        panic!("ordinary consumer")
    };
    assert!(receiver.is_none());
    assert_eq!(arguments, &[EntityId::Loan(*child)]);
    assert!(use_call.results.is_empty());
    assert_eq!(child_end.operation, Operation::BorrowEnd { loan: *child });
    assert_eq!(parent_end.operation, Operation::BorrowEnd { loan: *parent });
    assert_eq!(source_end.operation, Operation::BorrowEnd { loan: *source });
    assert_eq!(drop.operation, Operation::Drop { owner: *owner });
    let view = module.function(*view).unwrap();
    let consume = module.function(*consume).unwrap();
    assert_eq!(routes, &[view.name.clone(), caller.name.clone()]);
    for callee in [view, consume] {
        let [EntityId::Loan(parameter)] = callee.blocks[0].parameters.as_slice() else {
            panic!("callee-local loan")
        };
        assert_ne!(parameter.function(), source.function());
        assert_eq!(
            callee.entity(EntityId::Loan(*parameter)).unwrap().ty,
            caller.entity(EntityId::Loan(*source)).unwrap().ty
        );
    }
}

#[test]
fn parent_child_metadata_preserves_frontend_merge_and_true_off() {
    let text = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(own flag: Boolean): Unit { val owner = \"kept\"; borrow val parent = view(owner); borrow val child = parent; if (flag) { consume(child) } else { consume(child) }; consume(child) }";
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "parent-child-merge.ko", text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "parent-child-merge.ko", source, &parsed),
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
        let [observation] = on.ownership().loan_action_observations() else {
            panic!("one actual source catalog")
        };
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Partial);
        assert!(observation.binding_regions().is_empty());
        let retired = observation
            .actions()
            .iter()
            .filter_map(|action| match action {
                UnitLoanAction::BorrowBindingChildRetired(retired) => Some(retired),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [retired] = retired.as_slice() else {
            panic!("one actual child retirement after merge")
        };
        assert!(
            retired.use_return().is_none(),
            "ambiguous branch uses cannot be replaced by the final use"
        );
        for owned in [&on, &off] {
            let [transport] = owned.ownership().borrow_binding_transports() else {
                panic!("one original parent source transport")
            };
            assert_eq!(transport.binding(), retired.parent());
        }
        let child = on
            .ownership()
            .borrow_results()
            .bindings()
            .iter()
            .find(|binding| binding.binding() == retired.binding())
            .unwrap();
        assert_eq!(child.parent(), Some(retired.parent()));
        assert!(child.source_loan().is_none());
        // This fixed source proves frontend merge and observation isolation only.
        // No lowering success or control-flow Complete coverage is claimed here.
    }
}
