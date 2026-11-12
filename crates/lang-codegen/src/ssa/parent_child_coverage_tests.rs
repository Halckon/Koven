//! Independent complete parent-child interval, after actual dependency and unchanged lowering.
use super::parent_child_causal::{SOURCE, verify_parent_child_ssa};
use super::*;
use lang_frontend::ownership_checking::{
    UnitDropTarget, UnitIterationCleanupAction, UnitLoanTarget, check_compilation_unit_ownership,
    legacy_ownership_eq, with_loan_observation,
};

#[test]
fn parent_child_requires_independent_complete_region_with_true_off() {
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "parent-child-coverage.ko", SOURCE);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "parent-child-coverage.ko", source, &parsed),
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
        let [region] = observation.child_regions() else {
            panic!("one independently certified parent-child interval is required")
        };
        assert!(observation.owns_child_region(region));
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert_eq!(region.root(), dependency.root());
        assert_eq!(region.parent(), dependency.parent());
        assert_eq!(region.child(), dependency.child());
        assert_eq!(region.action_ids().collect::<Vec<_>>(), ids);
        assert_eq!(
            region.callable(),
            lang_frontend::type_checking::UnitItemId::new(run.source_unit(), run.root())
        );
        assert_eq!(region.source_call(), source_loan.call());
        assert_eq!(region.consumer_call(), consumer.call());
        let lang_frontend::parser::Item::Function {
            form:
                lang_frontend::parser::FunctionForm::Explicit {
                    body: lang_frontend::parser::FunctionBody::Block(body),
                    ..
                },
            ..
        } = parsed.ast().items().get(run.root()).unwrap().payload()
        else {
            panic!("actual ordinary caller block")
        };
        let lang_frontend::parser::Statement::Block { elements } =
            parsed.ast().statements().get(*body).unwrap().payload()
        else {
            panic!("actual root body")
        };
        assert_eq!(elements.len(), 4);
        assert_eq!(
            region.entry_statement(),
            lang_frontend::type_checking::UnitStatementId::new(run.source_unit(), elements[1])
        );
        assert_eq!(
            region.completion_statement(),
            lang_frontend::type_checking::UnitStatementId::new(run.source_unit(), *body)
        );
        assert_eq!(
            *point,
            lang_frontend::ownership_checking::UnitDropPoint::AfterStatement(
                lang_frontend::type_checking::UnitStatementId::new(run.source_unit(), elements[3])
            )
        );
        let facts = on.ownership().borrow_results().bindings();
        assert_eq!(facts.len(), 2);
        let actual_parent = facts
            .iter()
            .find(|fact| fact.binding() == region.parent())
            .unwrap();
        let actual_child = facts
            .iter()
            .find(|fact| fact.binding() == region.child())
            .unwrap();
        assert_eq!(actual_parent.parent(), None);
        let parent_source = actual_parent.source_loan().unwrap();
        assert_eq!(parent_source.call(), source_loan.call());
        assert_eq!(parent_source.argument(), source_loan.argument());
        assert_eq!(actual_child.parent(), Some(region.parent()));
        assert_eq!(actual_child.source_loan(), None);
        assert_eq!(actual_parent.initializer(), region.source_call());
        let lang_frontend::parser::Statement::LocalVariable { declaration } = parsed
            .ast()
            .statements()
            .get(elements[2])
            .unwrap()
            .payload()
        else {
            panic!("actual child declaration")
        };
        let lang_frontend::parser::Item::Variable { initializer, .. } =
            parsed.ast().items().get(*declaration).unwrap().payload()
        else {
            panic!("actual child variable")
        };
        assert!(matches!(
            parsed
                .ast()
                .expressions()
                .get(*initializer)
                .unwrap()
                .payload(),
            lang_frontend::parser::Expression::Name
        ));
        assert_eq!(
            actual_child.initializer(),
            lang_frontend::type_checking::UnitExpressionId::new(run.source_unit(), *initializer)
        );
        let ends = on.ownership().borrow_results().ends();
        assert_eq!(ends.len(), 2);
        assert!(ends.iter().all(|end| end.point() == *point));
        for binding in [region.parent(), region.child()] {
            assert_eq!(
                ends.iter().filter(|end| end.binding() == binding).count(),
                1
            );
        }
    }
}

#[test]
fn extra_plain_statement_keeps_partial_dependency_without_complete_region() {
    const EXTRA: &str = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun ping(): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val parent = view(owner); borrow val child = parent; consume(child); ping() }";
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "extra-child-statement.ko", EXTRA);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "extra-child-statement.ko", source, &parsed),
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
            panic!("one source catalog")
        };
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Partial);
        assert_eq!(observation.actions().len(), 10);
        assert_eq!(observation.binding_dependency_drops().len(), 1);
        assert!(observation.binding_regions().is_empty());
        assert!(observation.child_regions().is_empty());
        let run = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap();
        let mut outputs = Vec::new();
        for owned in [&on, &off] {
            let (lowered, receipts) = crate::ssa::refine::child_receipt::observe(|| {
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
            assert!(receipts.is_empty());
            let (program, _) = lowered.unwrap();
            verify_program(&program).unwrap();
            outputs.push((
                crate::ssa::render_program(&program),
                crate::llvm::render_verified_program(&program).unwrap(),
            ));
        }
        assert_eq!(outputs[0], outputs[1]);
    }
}
