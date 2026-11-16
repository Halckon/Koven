//! Independent complete parent-child interval, after actual dependency and unchanged lowering.
#[cfg(feature = "loan-observation-test-support")]
use super::parent_child_causal::{SOURCE, verify_parent_child_ssa};
#[cfg(feature = "loan-observation-test-support")]
use super::*;
#[cfg(feature = "loan-observation-test-support")]
use lang_frontend::ownership_checking::{
    UnitDropTarget, UnitIterationCleanupAction, UnitLoanTarget, check_compilation_unit_ownership,
    legacy_ownership_eq, with_loan_observation,
};

#[cfg(feature = "loan-observation-test-support")]
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

#[cfg(feature = "loan-observation-test-support")]
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

mod temporary_route {
    //! Ordinary source-only P1 route capability; no native execution or faults.
    use crate::ssa::loan_region_ssa_tests::*;

    #[test]
    fn temporary_borrow_caller_uses_refine_without_migrating_callee() {
        check(false);
    }

    #[test]
    fn temporary_borrow_requires_real_production_receipt() {
        check(true);
    }

    fn check(require_receipt: bool) {
        let text =
            "fun inspect(value: String): Int = 1\nfun run(): Unit { inspect(\"temporary\") }";
        let mut sources = SourceMap::new();
        let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "temporary.ko", text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "temporary.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
        ] {
            let inputs = [input];
            let (names, typed, owned) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            assert!(
                owned
                    .ownership()
                    .loan_action_observations()
                    .iter()
                    .flat_map(|o| o.regions())
                    .any(|r| r.protocol_version() == 1
                        && r.coverage() == UnitLoanActionCoverage::Complete)
            );
            let run = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap();
            let ((lowered, routing), receipts) =
                crate::ssa::refine::temporary_receipt::observe(|| {
                    crate::ssa::refine::observe_routing(|| {
                        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                            &sources,
                            &inputs,
                            &names,
                            &types,
                            &typed,
                            &owned,
                            run.id(),
                        )
                    })
                });
            let (program, entry) = lowered.unwrap();
            verify_program(&program).unwrap();
            crate::llvm::render_verified_program(&program).unwrap();
            let caller = program.modules[0].function(entry).unwrap();
            if require_receipt {
                assert_eq!(
                    receipts.len(),
                    1,
                    "nonempty Complete1 requires production receipt"
                );
            }
            assert_eq!(
                routing.as_slice(),
                std::slice::from_ref(&caller.name),
                "only the temporary Borrow caller should migrate"
            );
        }
    }

    #[test]
    fn non_target_temporary_shapes_preserve_routes_and_no_receipts() {
        for (case, text) in [
            "fun inspect(): Int = 1\nfun run(): Unit { inspect() }",
            "fun inspect(value: String): Boolean = true\nfun run(): Unit { inspect(\"a\") }",
            "fun inspect(value: String): Int = 1\nfun run(flag: Boolean): Unit { inspect(\"a\") }",
        ]
        .into_iter()
        .enumerate()
        {
            let mut sources = SourceMap::new();
            let (source, parsed) =
                unit_lower_test_support::parsed(&mut sources, "temporary-boundary.ko", text);
            let (environment, types) = standard_environments();
            for input in [
                SourceUnitInput::new("root", "temporary-boundary.ko", source, &parsed),
                SourceUnitInput::memory_document(source, &parsed),
            ] {
                let inputs = [input];
                let (names, typed, owned) =
                    unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
                if case == 0 {
                    let regions = owned
                        .ownership()
                        .loan_action_observations()
                        .iter()
                        .flat_map(|o| o.regions())
                        .collect::<Vec<_>>();
                    let [region] = regions.as_slice() else {
                        panic!("one actual P1 boundary region");
                    };
                    assert_eq!(region.protocol_version(), 1);
                    assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
                    assert_eq!(region.action_ids().count(), 0);
                }
                let run = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.name() == "run")
                    .unwrap();
                let ((lowered, routing), receipts) =
                    crate::ssa::refine::temporary_receipt::observe(|| {
                        crate::ssa::refine::observe_routing(|| {
                            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                &sources,
                                &inputs,
                                &names,
                                &types,
                                &typed,
                                &owned,
                                run.id(),
                            )
                        })
                    });
                let (program, entry) = lowered.unwrap();
                verify_program(&program).unwrap();
                crate::llvm::render_verified_program(&program).unwrap();
                let caller = program.modules[0].function(entry).unwrap();
                assert!(
                    !routing.contains(&caller.name),
                    "temporary caller must not gain admission"
                );
                if case == 0 {
                    // SPEC-0454 migrates only the no-argument scalar callee, not this caller.
                    let callees = program.modules[0]
                        .functions
                        .iter()
                        .filter(|function| function.id() != entry)
                        .collect::<Vec<_>>();
                    let [callee] = callees.as_slice() else {
                        panic!("one scalar callee")
                    };
                    assert_eq!(routing.as_slice(), std::slice::from_ref(&callee.name));
                } else {
                    assert!(routing.is_empty());
                }
                assert!(receipts.is_empty());
            }
        }
    }
}

#[cfg(feature = "loan-observation-test-support")]
mod temporary_receipts {
    //! Bounded ordinary temporary literal results; real off products are never edited before lowering.
    use crate::ssa::loan_region_ssa_tests::*;
    use lang_frontend::ownership_checking::{
        check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
    };

    #[test]
    fn temporary_int_unit_and_empty_string_preserve_real_observer_off_products() {
        for (text, result_count, expected_bytes) in [
            (
                "fun inspect(value: String): Int = 1\nfun run(): Unit { inspect(\"temporary\") }",
                1,
                b"temporary".as_slice(),
            ),
            (
                "fun inspect(value: String): Unit {}\nfun run(): Unit { inspect(\"temporary\") }",
                0,
                b"temporary".as_slice(),
            ),
            (
                "fun inspect(value: String): Int = 1\nfun run(): Unit { inspect(\"\") }",
                1,
                b"".as_slice(),
            ),
        ] {
            let mut sources = SourceMap::new();
            let (source, parsed) =
                unit_lower_test_support::parsed(&mut sources, "temporary-receipt.ko", text);
            let (environment, types) = standard_environments();
            for input in [
                SourceUnitInput::new("root", "temporary-receipt.ko", source, &parsed),
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
                let regions = on
                    .ownership()
                    .loan_action_observations()
                    .iter()
                    .flat_map(|o| o.regions())
                    .filter(|r| {
                        r.protocol_version() == 1
                            && r.coverage() == UnitLoanActionCoverage::Complete
                    })
                    .collect::<Vec<_>>();
                let [region] = regions.as_slice() else {
                    panic!("one nonempty Complete1");
                };
                let run = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.name() == "run")
                    .unwrap();
                let mut outputs = Vec::new();
                for (owned, count) in [(&on, 1), (&off, 0)] {
                    let (((lowered, routing), p4), receipts) =
                        crate::ssa::refine::temporary_receipt::observe(|| {
                            crate::ssa::refine::loan_receipt::observe(|| {
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
                            })
                        });
                    let (program, entry) = lowered.unwrap();
                    assert_eq!(receipts.len(), count);
                    assert!(p4.is_empty());
                    verify_program(&program).unwrap();
                    let module = &program.modules[0];
                    let caller = module.function(entry).unwrap();
                    assert_eq!(routing.as_slice(), std::slice::from_ref(&caller.name));
                    let [literal, root, begin, call, end, drop] = caller.instructions.as_slice()
                    else {
                        panic!("six actual operations");
                    };
                    let [EntityId::Value(owner)] = literal.results.as_slice() else {
                        panic!("actual literal owner");
                    };
                    let Operation::StringLiteral { string, bytes } = &literal.operation else {
                        panic!("actual literal formation");
                    };
                    assert_eq!(bytes, expected_bytes);
                    assert_eq!(module.types[string.index()], SsaTypeKind::StringOwner);
                    assert_eq!(
                        caller.entity(EntityId::Value(*owner)).unwrap().ty,
                        EntityType::Value(*string)
                    );
                    let [EntityId::Place(place)] = root.results.as_slice() else {
                        panic!("root place");
                    };
                    let [EntityId::Loan(loan)] = begin.results.as_slice() else {
                        panic!("temporary argument loan");
                    };
                    assert_eq!(root.operation, Operation::RootPlace { owner: *owner });
                    assert_eq!(
                        begin.operation,
                        Operation::BorrowBegin {
                            place: *place,
                            kind: LoanKind::Shared
                        }
                    );
                    let Operation::DirectCall {
                        callee,
                        receiver,
                        arguments,
                    } = &call.operation
                    else {
                        panic!("actual ordinary call");
                    };
                    assert!(receiver.is_none());
                    assert_eq!(arguments, &[EntityId::Loan(*loan)]);
                    assert_eq!(call.results.len(), result_count);
                    if let [value @ EntityId::Value(_)] = call.results.as_slice() {
                        let EntityType::Value(integer) = caller.entity(*value).unwrap().ty else {
                            panic!("owned Int call result");
                        };
                        assert_eq!(
                            module.types[integer.index()],
                            SsaTypeKind::Integer {
                                bits: 32,
                                signed: true
                            }
                        );
                    }
                    assert_eq!(
                        caller.entity(EntityId::Loan(*loan)).unwrap().ty,
                        EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: *string
                        }
                    );
                    assert_eq!(end.operation, Operation::BorrowEnd { loan: *loan });
                    assert_eq!(drop.operation, Operation::Drop { owner: *owner });
                    let method = module.function(*callee).unwrap();
                    let [EntityId::Loan(parameter)] = method.blocks[0].parameters.as_slice() else {
                        panic!("callee-local loan");
                    };
                    assert_ne!(parameter.function(), loan.function());
                    assert_eq!(
                        method.entity(EntityId::Loan(*parameter)).unwrap().ty,
                        EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: *string
                        }
                    );
                    assert_eq!(method.return_types.len(), result_count);
                    assert_eq!(method.instructions.len(), result_count);
                    if let [receipt] = receipts.as_slice() {
                        assert_eq!(receipt.ids, region.action_ids().collect::<Vec<_>>());
                        assert_eq!(Some(receipt.callable), region.callable());
                        assert_eq!(receipt.call, region.call());
                        assert_eq!(receipt.caller, entry);
                        assert_eq!(
                            receipt.instructions,
                            caller.instructions.iter().map(|i| i.id).collect::<Vec<_>>()
                        );
                        assert_eq!(
                            receipt.entities,
                            [
                                EntityId::Value(*owner),
                                EntityId::Place(*place),
                                EntityId::Loan(*loan)
                            ]
                        );
                    }
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

#[cfg(feature = "loan-observation-test-support")]
pub(in crate::ssa) mod binding_causal_links {
    //! Same-analysis actual binding links; normal products are never rewritten.
    use crate::ssa::loan_region_ssa_tests::*;
    use lang_frontend::ownership_checking::{
        UnitDropTarget, UnitIterationCleanupAction, UnitLoanTarget,
        check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
    };
    pub(in crate::ssa) const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner); consume(item) }";
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
    pub(in crate::ssa) fn verify_binding_ssa(
        program: &Program,
        entry: FunctionId,
        routes: &[String],
    ) {
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
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "binding-merge.ko", text);
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
}
