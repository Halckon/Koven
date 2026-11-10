//! Partial dependency to a real Drop; no coverage or absence claim.
use super::parent_child_causal::{SOURCE, verify_parent_child_ssa};
use super::*;
use lang_frontend::ownership_checking::{
    UnitDropTarget, UnitIterationCleanupAction, UnitLoanTarget, check_compilation_unit_ownership,
    legacy_ownership_eq, with_loan_observation,
};

#[test]
fn parent_child_requires_actual_partial_drop_dependency_with_true_off() {
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "parent-child-drop-link.ko", SOURCE);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "parent-child-drop-link.ko", source, &parsed),
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
        let [dependency] = observation.binding_dependency_drops() else {
            panic!("one actual partial parent-child dependency to owner Drop is required")
        };
        assert_eq!(dependency.root(), owner.root());
        assert_eq!(dependency.parent(), parent_commit.binding());
        assert_eq!(dependency.child(), child_commit.binding());
        for (reference, index) in [
            (dependency.source_begin(), 0),
            (dependency.parent_handoff(), 1),
            (dependency.child_commit(), 2),
            (dependency.child_use_return(), 4),
            (dependency.child_retirement(), 6),
            (dependency.parent_retirement(), 8),
            (dependency.cleanup(), 9),
        ] {
            assert_eq!(reference, ids[index]);
            assert_eq!(observation.action(&reference), Some(&actions[index]));
        }
    }
}
