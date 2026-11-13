//! Actual same-catalog parent/child links, with unchanged on/off lowering products.
use super::*;
use lang_frontend::ownership_checking::{
    UnitDropTarget, UnitIterationCleanupAction, UnitLoanTarget, check_compilation_unit_ownership,
    legacy_ownership_eq, with_loan_observation,
};

pub(super) const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val parent = view(owner); borrow val child = parent; consume(child) }";

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

pub(super) fn verify_parent_child_ssa(program: &Program, entry: FunctionId, routes: &[String]) {
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
        assert!(observation.binding_dependency_drops().is_empty());
        assert!(observation.child_regions().is_empty());
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

mod binding_coverage {
    //! Ordinary same-analysis complete coverage, without changing actual products.
    use crate::ssa::loan_region_ssa_tests::*;
    use lang_frontend::ownership_checking::{
        check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
    };

    #[test]
    fn single_binding_requires_independent_complete_region() {
        let mut sources = SourceMap::new();
        let (source, parsed) = unit_lower_test_support::parsed(
            &mut sources,
            "binding-coverage.ko",
            crate::ssa::loan_region_ssa_tests::parent_child_coverage::binding_causal_links::SOURCE,
        );
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "binding-coverage.ko", source, &parsed),
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
                crate::ssa::loan_region_ssa_tests::parent_child_coverage::binding_causal_links::verify_binding_ssa(&program, entry, &routes);
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
            assert!(
                observation
                    .regions()
                    .iter()
                    .all(|r| r.protocol_version() == 3
                        && r.coverage() == UnitLoanActionCoverage::Unsupported)
            );
            let actions = observation.actions();
            let [
                UnitLoanAction::Begin(source_loan),
                UnitLoanAction::BorrowHandoffCommitted(commit),
                UnitLoanAction::Begin(_),
                UnitLoanAction::BorrowBindingUseReturned(used),
                UnitLoanAction::BorrowBindingEnd(end),
                UnitLoanAction::BorrowBindingSourceRetired(retired),
                UnitLoanAction::Cleanup { .. },
            ] = actions
            else {
                panic!("actual seven causal records remain intact")
            };
            assert_eq!(used.source_begin(), observation.action_id(0));
            assert_eq!(used.handoff(), observation.action_id(1));
            assert_eq!(used.begin(), observation.action_id(2));
            assert_eq!(retired.use_return(), observation.action_id(3));
            assert_eq!(retired.binding_end(), observation.action_id(4));
            assert_eq!(commit.binding(), end.binding());
            let [region] = observation.binding_regions() else {
                panic!("independent binding coverage region is required")
            };
            assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
            assert!(observation.owns_binding_region(region));
            assert_eq!(region.binding(), end.binding());
            assert_eq!(
                region.callable(),
                lang_frontend::type_checking::UnitItemId::new(run.source_unit(), run.root())
            );
            assert_eq!(region.source_call(), source_loan.call());
            assert_eq!(region.consumer_call(), used.loan().call());
            let lang_frontend::ownership_checking::UnitLoanTarget::Place(place) =
                source_loan.target()
            else {
                panic!("local owner root")
            };
            assert_eq!(region.root(), place.root());
            assert_ne!(region.root(), region.binding());
            let lang_frontend::parser::Item::Function {
                form:
                    lang_frontend::parser::FunctionForm::Explicit {
                        body: lang_frontend::parser::FunctionBody::Block(body),
                        ..
                    },
                ..
            } = parsed.ast().items().get(run.root()).unwrap().payload()
            else {
                panic!("actual caller body")
            };
            let lang_frontend::parser::Statement::Block { elements } =
                parsed.ast().statements().get(*body).unwrap().payload()
            else {
                panic!("actual root block")
            };
            assert_eq!(elements.len(), 3);
            assert_eq!(
                region.entry_statement(),
                lang_frontend::type_checking::UnitStatementId::new(run.source_unit(), elements[1])
            );
            assert_eq!(
                region.completion_statement(),
                lang_frontend::type_checking::UnitStatementId::new(run.source_unit(), *body)
            );
            let lang_frontend::ownership_checking::UnitDropPoint::AfterStatement(retirement_point) =
                retired.transaction().point()
            else {
                panic!("actual immediate statement retirement")
            };
            assert_eq!(
                retirement_point,
                lang_frontend::type_checking::UnitStatementId::new(run.source_unit(), elements[2])
            );
            assert_eq!(
                region.action_ids().collect::<Vec<_>>(),
                (0..actions.len())
                    .map(|i| observation.action_id(i).unwrap())
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn extra_use_and_later_owner_read_do_not_gain_binding_completeness() {
        for suffix in [
            "consume(item); consume(item)",
            "consume(item); consume(owner)",
        ] {
            let text = format!(
                "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; borrow val item = view(owner); {suffix} }}"
            );
            let mut sources = SourceMap::new();
            let (source, parsed) =
                unit_lower_test_support::parsed(&mut sources, "binding-boundary.ko", &text);
            let (environment, types) = standard_environments();
            for input in [
                SourceUnitInput::new("root", "binding-boundary.ko", source, &parsed),
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
                assert!(observation.binding_regions().is_empty());
                let run = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.name() == "run")
                    .unwrap();
                let mut outputs = Vec::new();
                for owned in [&on, &off] {
                    let (program, _) = crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                        &sources,
                        &inputs,
                        &names,
                        &types,
                        &typed,
                        owned,
                        run.id(),
                    )
                    .unwrap();
                    verify_program(&program).unwrap();
                    outputs.push((
                        crate::ssa::render_program(&program),
                        crate::llvm::render_verified_program(&program).unwrap(),
                    ));
                }
                assert_eq!(outputs[0], outputs[1]);
            }
        }
    }
}

mod binding_receipts {
    //! Ordinary same-analysis complete coverage, without changing actual products.
    use crate::ssa::loan_region_ssa_tests::*;
    use lang_frontend::ownership_checking::{
        check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
    };

    #[test]
    fn complete_binding_requires_real_production_receipt() {
        let mut sources = SourceMap::new();
        let (source, parsed) = unit_lower_test_support::parsed(
            &mut sources,
            "binding-coverage.ko",
            crate::ssa::loan_region_ssa_tests::parent_child_coverage::binding_causal_links::SOURCE,
        );
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "binding-coverage.ko", source, &parsed),
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
            let mut observed = Vec::new();
            let mut actual = Vec::new();
            for owned in [&on, &off] {
                let ((lowered, routes), receipts) =
                    crate::ssa::refine::binding_receipt::observe(|| {
                        crate::ssa::refine::observe_routing(|| {
                            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                &sources,
                                &inputs,
                                &names,
                                &types,
                                &typed,
                                owned,
                                run.id(),
                            )
                        })
                    });
                let (program, entry) = lowered.unwrap();
                verify_program(&program).unwrap();
                crate::ssa::loan_region_ssa_tests::parent_child_coverage::binding_causal_links::verify_binding_ssa(&program, entry, &routes);
                let caller = program.modules[0].function(entry).unwrap();
                actual.push((
                    entry,
                    caller
                        .instructions
                        .iter()
                        .skip(1)
                        .map(|i| i.id)
                        .collect::<Vec<_>>(),
                    caller
                        .instructions
                        .iter()
                        .take(4)
                        .map(|i| i.results[0])
                        .collect::<Vec<_>>(),
                ));
                observed.push(receipts);
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
            assert!(
                observation
                    .regions()
                    .iter()
                    .all(|r| r.protocol_version() == 3
                        && r.coverage() == UnitLoanActionCoverage::Unsupported)
            );
            let actions = observation.actions();
            let [
                UnitLoanAction::Begin(source_loan),
                UnitLoanAction::BorrowHandoffCommitted(commit),
                UnitLoanAction::Begin(_),
                UnitLoanAction::BorrowBindingUseReturned(used),
                UnitLoanAction::BorrowBindingEnd(end),
                UnitLoanAction::BorrowBindingSourceRetired(retired),
                UnitLoanAction::Cleanup { .. },
            ] = actions
            else {
                panic!("actual seven causal records remain intact")
            };
            assert_eq!(used.source_begin(), observation.action_id(0));
            assert_eq!(used.handoff(), observation.action_id(1));
            assert_eq!(used.begin(), observation.action_id(2));
            assert_eq!(retired.use_return(), observation.action_id(3));
            assert_eq!(retired.binding_end(), observation.action_id(4));
            assert_eq!(commit.binding(), end.binding());
            let [region] = observation.binding_regions() else {
                panic!("independent binding coverage region is required")
            };
            assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
            assert!(observation.owns_binding_region(region));
            assert_eq!(region.binding(), end.binding());
            assert_eq!(
                region.callable(),
                lang_frontend::type_checking::UnitItemId::new(run.source_unit(), run.root())
            );
            assert_eq!(region.source_call(), source_loan.call());
            assert_eq!(region.consumer_call(), used.loan().call());
            let lang_frontend::ownership_checking::UnitLoanTarget::Place(place) =
                source_loan.target()
            else {
                panic!("local owner root")
            };
            assert_eq!(region.root(), place.root());
            assert_ne!(region.root(), region.binding());
            let lang_frontend::parser::Item::Function {
                form:
                    lang_frontend::parser::FunctionForm::Explicit {
                        body: lang_frontend::parser::FunctionBody::Block(body),
                        ..
                    },
                ..
            } = parsed.ast().items().get(run.root()).unwrap().payload()
            else {
                panic!("actual caller body")
            };
            let lang_frontend::parser::Statement::Block { elements } =
                parsed.ast().statements().get(*body).unwrap().payload()
            else {
                panic!("actual root block")
            };
            assert_eq!(elements.len(), 3);
            assert_eq!(
                region.entry_statement(),
                lang_frontend::type_checking::UnitStatementId::new(run.source_unit(), elements[1])
            );
            assert_eq!(
                region.completion_statement(),
                lang_frontend::type_checking::UnitStatementId::new(run.source_unit(), *body)
            );
            let lang_frontend::ownership_checking::UnitDropPoint::AfterStatement(retirement_point) =
                retired.transaction().point()
            else {
                panic!("actual immediate statement retirement")
            };
            assert_eq!(
                retirement_point,
                lang_frontend::type_checking::UnitStatementId::new(run.source_unit(), elements[2])
            );
            assert_eq!(
                region.action_ids().collect::<Vec<_>>(),
                (0..actions.len())
                    .map(|i| observation.action_id(i).unwrap())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                observed[0].len(),
                1,
                "Complete binding must have a real production receipt"
            );
            assert!(observed[1].is_empty());
            let receipt = &observed[0][0];
            assert_eq!(receipt.ids, region.action_ids().collect::<Vec<_>>());
            assert_eq!(receipt.callable, region.callable());
            assert_eq!(receipt.binding, region.binding());
            assert_eq!(receipt.root, region.root());
            assert_eq!(receipt.source_call, region.source_call());
            assert_eq!(receipt.consumer_call, region.consumer_call());
            assert_eq!(receipt.entry_statement, region.entry_statement());
            assert_eq!(receipt.completion_statement, region.completion_statement());
            assert_eq!(receipt.caller, actual[0].0);
            assert_eq!(receipt.instructions, actual[0].1);
            assert_eq!(receipt.entities, actual[0].2);
        }
    }

    #[test]
    fn extra_use_and_later_owner_read_keep_old_path_without_binding_receipt() {
        for suffix in [
            "consume(item); consume(item)",
            "consume(item); consume(owner)",
        ] {
            let text = format!(
                "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; borrow val item = view(owner); {suffix} }}"
            );
            let mut sources = SourceMap::new();
            let (source, parsed) =
                unit_lower_test_support::parsed(&mut sources, "binding-boundary.ko", &text);
            let (environment, types) = standard_environments();
            for input in [
                SourceUnitInput::new("root", "binding-boundary.ko", source, &parsed),
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
                assert!(observation.binding_regions().is_empty());
                let run = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.name() == "run")
                    .unwrap();
                let mut outputs = Vec::new();
                for owned in [&on, &off] {
                    let (lowered, receipts) = crate::ssa::refine::binding_receipt::observe(|| {
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
    }
}

mod parent_child_lifecycle_probe {
    //! One ordinary original-source probe; no fact mutation or forced routing.
    use crate::ssa::loan_region_ssa_tests::*;

    #[test]
    fn original_parent_child_lifecycle_records_real_dependency_and_ssa() {
        let text = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val parent = view(owner); borrow val child = parent; consume(child) }";
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "parent-child-lifecycle.ko", text);
        let (environment, types) = standard_environments();
        let inputs = [SourceUnitInput::memory_document(source, &parsed)];
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        println!(
            "actions={:#?}",
            owned.ownership().loan_action_observations()
        );
        println!("handoffs={:#?}", owned.ownership().borrow_call_handoffs());
        println!(
            "transports={:#?}",
            owned.ownership().borrow_binding_transports()
        );
        println!(
            "bindings={:#?}",
            owned.ownership().borrow_results().bindings()
        );
        println!(
            "binding_ends={:#?}",
            owned.ownership().borrow_results().ends()
        );
        let run = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap();
        let (lowered, routes) = crate::ssa::refine::observe_routing(|| {
            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                &owned,
                run.id(),
            )
        });
        let (program, entry) = lowered.expect("original ordinary parent-child source lowers");
        verify_program(&program).unwrap();
        crate::llvm::render_verified_program(&program).unwrap();
        println!("routes={routes:?}; entry={entry:?}");
        println!("{}", crate::ssa::render_program(&program));

        let caller = program.modules[0].function(entry).unwrap();
        let calls: Vec<_> = caller
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(index, instruction)| {
                if let Operation::BorrowCall { source, .. } = &instruction.operation {
                    Some((index, *source, instruction))
                } else {
                    None
                }
            })
            .collect();
        let [(handoff_index, source, handoff)] = calls.as_slice() else {
            panic!("one actual borrowed return")
        };
        let [EntityId::Loan(parent)] = handoff.results.as_slice() else {
            panic!("actual parent result")
        };
        let children: Vec<_> = caller
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(index, instruction)| {
                if let Operation::SharedReborrow { source } = &instruction.operation {
                    Some((index, *source, instruction))
                } else {
                    None
                }
            })
            .collect();
        let [(child_index, actual_parent, reborrow)] = children.as_slice() else {
            panic!("one actual child reborrow")
        };
        assert_eq!(actual_parent, parent);
        let [EntityId::Loan(child)] = reborrow.results.as_slice() else {
            panic!("actual child result")
        };
        assert_ne!(source, parent);
        assert_ne!(parent, child);
        assert_ne!(source, child);
        let consumers: Vec<_> = caller
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(index, instruction)| {
                if let Operation::DirectCall {
                    receiver,
                    arguments,
                    ..
                } = &instruction.operation
                {
                    Some((index, receiver, arguments, instruction))
                } else {
                    None
                }
            })
            .collect();
        let [(consumer_index, receiver, arguments, consumer)] = consumers.as_slice() else {
            panic!("one ordinary consumer")
        };
        assert!(receiver.is_none());
        assert_eq!(arguments.as_slice(), &[EntityId::Loan(*child)]);
        assert!(consumer.results.is_empty());
        let ends: Vec<_> = caller
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(index, instruction)| {
                if let Operation::BorrowEnd { loan } = &instruction.operation {
                    Some((index, *loan))
                } else {
                    None
                }
            })
            .collect();
        let [
            (child_end, ended_child),
            (parent_end, ended_parent),
            (source_end, ended_source),
        ] = ends.as_slice()
        else {
            panic!("exactly child, parent and source retirements")
        };
        assert_eq!(
            (ended_child, ended_parent, ended_source),
            (child, parent, source)
        );
        assert!(handoff_index < child_index && child_index < consumer_index);
        assert!(consumer_index < child_end && child_end < parent_end && parent_end < source_end);
        let roots: Vec<_> = caller
            .instructions
            .iter()
            .filter_map(|instruction| {
                if let Operation::RootPlace { owner } = &instruction.operation {
                    Some(*owner)
                } else {
                    None
                }
            })
            .collect();
        let [owner] = roots.as_slice() else {
            panic!("one original owner place")
        };
        let drops: Vec<_> = caller
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(index, instruction)| {
                if let Operation::Drop { owner } = &instruction.operation {
                    Some((index, *owner))
                } else {
                    None
                }
            })
            .collect();
        let [(drop_index, dropped)] = drops.as_slice() else {
            panic!("one original owner drop")
        };
        assert_eq!(dropped, owner);
        assert!(source_end < drop_index);
    }
}
