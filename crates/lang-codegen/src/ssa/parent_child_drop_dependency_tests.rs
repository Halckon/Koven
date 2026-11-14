//! Partial dependency to a real Drop; no coverage or absence claim.
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

#[cfg(feature = "loan-observation-test-support")]
#[test]
fn later_parent_or_owner_use_preserves_old_products_without_drop_dependency() {
    let cases = [
        (
            "later-parent.ko",
            "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val parent = view(owner); borrow val child = parent; consume(child); consume(parent) }",
        ),
        (
            "later-owner.ko",
            "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val parent = view(owner); borrow val child = parent; consume(child); consume(owner) }",
        ),
    ];
    for (path, text) in cases {
        let mut sources = SourceMap::new();
        let (source, parsed) = unit_lower_test_support::parsed(&mut sources, path, text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", path, source, &parsed),
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
                .find(|declaration| declaration.name() == "run")
                .unwrap();
            let mut outputs = Vec::new();
            for owned in [&on, &off] {
                let ((lowered, routes), receipts) =
                    crate::ssa::refine::child_receipt::observe(|| {
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
                assert!(receipts.is_empty());
                let (program, _) =
                    lowered.expect("ordinary later use retains its existing lowering");
                verify_program(&program).unwrap();
                outputs.push((
                    routes,
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
            assert!(observation.binding_dependency_drops().is_empty());
            assert!(observation.child_regions().is_empty());
        }
    }
}

#[cfg(feature = "loan-observation-test-support")]
pub(in crate::ssa) mod products {
    //! Real reanalysis and actual producer identities, with no synthetic inputs.
    // The two-literal source belongs to this product family; `loan_region_ssa_tests::SOURCE`
    // is the borrow-return fixture and must not be substituted here.
    use crate::ssa::loan_region_ssa_tests::multi_temporary::SOURCE;
    use crate::ssa::loan_region_ssa_tests::*;
    use lang_frontend::ownership_checking::{
        check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
    };
    #[test]
    fn two_literal_int_unit_same_bytes_and_empty_preserve_true_off() {
        check_cases(
            &[
                (SOURCE, [b"left".as_slice(), b"right".as_slice()], 1),
                (
                    "fun inspect(a: String, b: String): Unit {}\nfun run(): Unit { inspect(\"same\", \"same\") }",
                    [b"same".as_slice(), b"same".as_slice()],
                    0,
                ),
                (
                    "fun inspect(a: String, b: String): Int = 1\nfun run(): Unit { inspect(\"\", \"\") }",
                    [b"".as_slice(), b"".as_slice()],
                    1,
                ),
            ],
            [0, 1],
        );
    }

    pub(in crate::ssa) fn check_cases<const N: usize>(
        cases: &[(&str, [&[u8]; N], usize)],
        parameter_sources: [usize; N],
    ) {
        for &(text, bytes, result_count) in cases {
            let mut sources = SourceMap::new();
            let (source, parsed) =
                unit_lower_test_support::parsed(&mut sources, "two-off.ko", text);
            let (environment, types) = standard_environments();
            for input in [
                SourceUnitInput::new("root", "two-off.ko", source, &parsed),
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
                let observation = &on.ownership().loan_action_observations()[0];
                let [region] = observation.regions() else {
                    panic!("one real region")
                };
                assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
                assert_eq!(region.protocol_version(), 1);
                let mapped = typed.types().call(region.call()).unwrap();
                let expected_mapping = (0..N)
                    .map(|source| {
                        (
                            source,
                            parameter_sources.iter().position(|i| *i == source).unwrap(),
                        )
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    mapped
                        .arguments()
                        .iter()
                        .map(|a| (a.argument_index(), a.parameter_index()))
                        .collect::<Vec<_>>(),
                    expected_mapping
                );

                let ids = region.action_ids().collect::<Vec<_>>();
                assert_eq!(ids.len(), 3 * N);
                let actions = ids
                    .iter()
                    .map(|id| observation.action(id).unwrap())
                    .collect::<Vec<_>>();
                let begins = actions[..N]
                    .iter()
                    .map(|action| match action {
                        UnitLoanAction::Begin(begin) => begin,
                        _ => panic!("source-order Begin"),
                    })
                    .collect::<Vec<_>>();
                for (i, begin) in begins.iter().enumerate() {
                    assert!(begins[..i].iter().all(|previous| {
                        previous.argument() != begin.argument()
                            && previous.target() != begin.target()
                    }));
                    let parameter = expected_mapping[i].1;
                    assert_eq!(
                        sources.slice(begin.parameter_span().unwrap()).unwrap(),
                        format!(
                            "{}: String",
                            char::from(b'a' + u8::try_from(parameter).unwrap())
                        )
                    );
                    let contract = on
                        .ownership()
                        .call_argument_contracts()
                        .iter()
                        .find(|c| c.call() == region.call() && c.argument() == begin.argument())
                        .unwrap();
                    assert_eq!(contract.parameter_index(), parameter);
                    assert_eq!(contract.parameter_span(), begin.parameter_span());

                    let UnitLoanAction::Cleanup {
                        action:
                            lang_frontend::ownership_checking::UnitIterationCleanupAction::EndCallLoan(
                                end,
                            ),
                        ..
                    } = actions[2 * N - 1 - i]
                    else {
                        panic!("exact reverse End")
                    };
                    assert_eq!(*begin, end);
                    let UnitLoanAction::Cleanup {
                        action:
                            lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(drop),
                        ..
                    } = actions[3 * N - 1 - i]
                    else {
                        panic!("exact reverse Drop")
                    };
                    assert_eq!(
                        drop.target(),
                        lang_frontend::ownership_checking::UnitDropTarget::Temporary(
                            begin.argument()
                        )
                    );
                }
                let run = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.name() == "run")
                    .unwrap();
                let mut outputs = Vec::new();
                for (owned, count) in [(&on, 1), (&off, 0)] {
                    let ((lowered, routes), receipts) =
                        crate::ssa::refine::temporary_receipt::observe(|| {
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
                    let module = &program.modules[0];
                    let caller = module.function(entry).unwrap();
                    assert_eq!(routes.as_slice(), std::slice::from_ref(&caller.name));
                    assert_eq!(receipts.len(), count);
                    assert_eq!(caller.instructions.len(), 5 * N + 1);
                    let mut entities = Vec::new();
                    let mut loans = Vec::new();
                    let mut owners = Vec::new();
                    for (i, expected) in bytes.iter().enumerate() {
                        let literal = &caller.instructions[3 * i];
                        let root = &caller.instructions[3 * i + 1];
                        let begin = &caller.instructions[3 * i + 2];
                        let Operation::StringLiteral { string, bytes } = &literal.operation else {
                            panic!("actual literal")
                        };
                        assert_eq!(bytes, expected);
                        assert_eq!(module.types[string.index()], SsaTypeKind::StringOwner);
                        let [EntityId::Value(owner)] = literal.results.as_slice() else {
                            panic!("owner")
                        };
                        let [EntityId::Place(place)] = root.results.as_slice() else {
                            panic!("place")
                        };
                        let [EntityId::Loan(loan)] = begin.results.as_slice() else {
                            panic!("loan")
                        };
                        assert_eq!(root.operation, Operation::RootPlace { owner: *owner });
                        assert_eq!(
                            begin.operation,
                            Operation::BorrowBegin {
                                place: *place,
                                kind: LoanKind::Shared
                            }
                        );
                        assert_eq!(
                            caller.entity(EntityId::Loan(*loan)).unwrap().ty,
                            EntityType::Loan {
                                kind: LoanKind::Shared,
                                target: *string
                            }
                        );
                        entities.extend([
                            EntityId::Value(*owner),
                            EntityId::Place(*place),
                            EntityId::Loan(*loan),
                        ]);
                        loans.push(*loan);
                        owners.push(*owner);
                    }
                    assert!(
                        entities.iter().all(|e| entities
                            .iter()
                            .filter(|other| *other == e)
                            .count()
                            == 1)
                    );
                    let call = &caller.instructions[3 * N];
                    let Operation::DirectCall {
                        callee,
                        receiver,
                        arguments,
                    } = &call.operation
                    else {
                        panic!("one actual call")
                    };
                    assert!(receiver.is_none());
                    assert_eq!(
                        *arguments,
                        parameter_sources
                            .iter()
                            .map(|i| EntityId::Loan(loans[*i]))
                            .collect::<Vec<_>>()
                    );
                    assert_eq!(call.results.len(), result_count);
                    if let [e] = call.results.as_slice() {
                        let EntityType::Value(ty) = caller.entity(*e).unwrap().ty else {
                            panic!("Int")
                        };
                        assert_eq!(
                            module.types[ty.index()],
                            SsaTypeKind::Integer {
                                bits: 32,
                                signed: true
                            }
                        );
                    }
                    let method = module.function(*callee).unwrap();
                    assert_eq!(method.return_types.len(), result_count);
                    assert_eq!(method.instructions.len(), result_count);
                    assert_eq!(method.blocks[0].parameters.len(), N);
                    for (i, param) in method.blocks[0].parameters.iter().enumerate() {
                        let EntityId::Loan(param) = param else {
                            panic!("callee loan")
                        };
                        assert_ne!(param.function(), loans[i].function());
                        assert_eq!(
                            method.entity(EntityId::Loan(*param)).unwrap().ty,
                            caller.entity(EntityId::Loan(loans[i])).unwrap().ty
                        );
                    }
                    for i in 0..N {
                        assert_eq!(
                            caller.instructions[3 * N + 1 + i].operation,
                            Operation::BorrowEnd {
                                loan: loans[N - 1 - i]
                            }
                        );
                        assert_eq!(
                            caller.instructions[4 * N + 1 + i].operation,
                            Operation::Drop {
                                owner: owners[N - 1 - i]
                            }
                        );
                    }
                    if let [r] = receipts.as_slice() {
                        assert_eq!(r.ids, ids);
                        assert_eq!(r.caller, entry);
                        assert_eq!(r.call, region.call());
                        assert_eq!(Some(r.callable), region.callable());
                        assert_eq!(r.entities, entities);
                        assert_eq!(
                            r.instructions,
                            caller.instructions.iter().map(|i| i.id).collect::<Vec<_>>()
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
    #[test]
    fn reordered_named_literals_use_refine_with_production_receipt() {
        let text = "fun inspect(a: String, b: String): Int = 1\nfun run(): Unit { inspect(b = \"first\", a = \"second\") }";
        let mut sources = SourceMap::new();
        let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "reordered.ko", text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "reordered.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
        ] {
            let inputs = [input];
            let (names, typed, owned) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            let regions = owned
                .ownership()
                .loan_action_observations()
                .iter()
                .flat_map(|o| o.regions())
                .collect::<Vec<_>>();
            let [region] = regions.as_slice() else {
                panic!("one real region")
            };
            assert_eq!(region.protocol_version(), 1);
            assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
            assert_eq!(region.action_ids().count(), 6);
            let run = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap();
            let ((lowered, routes), receipts) =
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
            assert_eq!(routes.as_slice(), std::slice::from_ref(&caller.name));
            assert_eq!(receipts.len(), 1);
        }
    }
}

pub(in crate::ssa) mod range_view_source {
    //! View-source descriptors inherit the collection root, independently of parent metadata.
    use crate::ssa::model::{Definition, EntityId, EntityType, Operation};
    use crate::ssa::range_lowering_tests::*;
    use crate::ssa::verify::verify_program;
    use crate::test_support::diagnostics::assert_diagnostic;

    #[test]
    fn range_view_source_take_single_and_unit_end_parent_before_child_use() {
        let consumer = "fun metadata(source:View<String>):borrow View<String> from source=source\nfun read(view:View<String>):Unit{for(item in view){println(item)}}\nfun consume(own source:List<String>):Unit{}\nfun main():Unit{val source=listOf(\"first\".clone(),\"second\".clone());borrow val parent=take(source,2);borrow val child=take(parent,1);borrow val alias=metadata(child);read(alias);consume(source)}";
        for program in [
            memory_unit_llvm(consumer),
            unit(&format!(
                "package app\nimport koven.algorithms.take\n{consumer}"
            )),
        ] {
            let caller = program.modules[0]
                .functions
                .iter()
                .find(|function| {
                    function
                        .instructions
                        .iter()
                        .filter(|instruction| {
                            matches!(instruction.operation, Operation::RangeCall { .. })
                        })
                        .count()
                        == 2
                })
                .unwrap();
            let calls = caller
                .instructions
                .iter()
                .filter(|instruction| matches!(instruction.operation, Operation::RangeCall { .. }))
                .collect::<Vec<_>>();
            let EntityId::Value(parent) = calls[0].results[0] else {
                panic!("parent descriptor");
            };
            let parent_end = caller.instructions.iter().find(|instruction| {
                matches!(instruction.operation, Operation::RangeEnd { view, .. } if view == parent)
            }).unwrap();
            let metadata_call = caller
                .instructions
                .iter()
                .find(|instruction| matches!(instruction.operation, Operation::BorrowCall { .. }))
                .unwrap();
            assert!(calls[1].id.index() < parent_end.id.index());
            assert!(parent_end.id.index() < metadata_call.id.index());
            crate::llvm::render_verified_program(&program).unwrap();
        }
    }

    #[test]
    fn range_view_source_native_single_and_unit_preserve_collection_and_element_identities() {
        use crate::native_tests::boxed_enum_tests::{
            assert_success, run_counted_allocations_in_order,
        };
        for (declaration, element, values, read, drops) in [
            (
                "",
                "String",
                "\"first\".clone(),\"second\".clone()",
                "println(item)",
                "",
            ),
            (
                "class Item(val text:String){}",
                "Item",
                "Item(\"first\"),Item(\"second\")",
                "println(item.text)",
                "",
            ),
            (
                "class Item(val text:String){deinit(){println(this.text)}}",
                "Item",
                "Item(\"first\"),Item(\"second\")",
                "println(item.text)",
                "second\nfirst\n",
            ),
        ] {
            let consumer = format!(
                "{declaration}\nfun metadata(source:View<{element}>):borrow View<{element}> from source=source\nfun read(view:View<{element}>):Unit{{for(item in view){{{read}}}}}\nfun consume(own source:List<{element}>):Unit{{println(\"consume\")}}\nfun main():Unit{{val source=listOf({values});borrow val parent=take(source,2);borrow val child=take(parent,2);borrow val grandchild=take(child,1);borrow val alias=metadata(grandchild);read(alias);println(\"after\");consume(source);println(\"done\")}}"
            );
            for (program, entry) in [
                memory_unit_with_entry(&consumer),
                unit_with_entry(&format!(
                    "package app\nimport koven.algorithms.take\n{consumer}"
                )),
            ] {
                let llvm =
                    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
                // Only the two elements and List allocate; metadata creation and forwarding allocate nothing.
                let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2]);
                assert_success(
                    &run,
                    format!("first\nafter\nconsume\n{drops}done\n").as_bytes(),
                );
            }
        }
    }

    #[test]
    fn range_view_source_offsets_and_forwarded_return_preserve_absolute_root_coordinates() {
        use crate::native_tests::boxed_enum_tests::{
            assert_success, run_counted_allocations_in_order,
        };
        let trusted = "fun <T> window(source:List<T>):View<T> from source=rangeView(source,1,2)\nfun <T> forward(source:View<T>,count:Int):View<T> from source=take(source,count)";
        for count in [0, 1, 2147483647] {
            let consumer = format!(
                "class Item(val text:String){{deinit(){{println(this.text)}}}}\nfun read(view:View<Item>):Unit{{for(item in view){{println(item.text)}}}}\nfun main():Unit{{val source=listOf(Item(\"first\"),Item(\"second\"));borrow val parent=window(source);borrow val child=forward(parent,{count});read(child);println(\"after\")}}"
            );
            for (program, entry) in [
                memory_unit_with_entry(&format!("{trusted}\n{consumer}")),
                unit_with_provider_entry(
                    &format!(
                        "package app\nimport koven.algorithms.window\nimport koven.algorithms.forward\n{consumer}"
                    ),
                    trusted,
                ),
            ] {
                let llvm =
                    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
                let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2]);
                let visits = if count == 0 { "" } else { "second\n" };
                assert_success(&run, format!("{visits}after\nsecond\nfirst\n").as_bytes());
            }
        }
    }

    #[test]
    fn range_view_source_parent_child_and_elements_coexist_until_their_own_last_uses() {
        let consumer = "class Item(val text:String){deinit(){println(this.text)}}\nfun read(view:View<Item>):Unit{for(item in view){println(item.text)}}\nfun consume(own source:List<Item>):Unit{}\nfun main():Unit{val source=listOf(Item(\"first\"),Item(\"second\"));{borrow val parent=take(source,2);borrow val child=take(parent,1);for(item in parent){read(child);println(item.text)};read(child)};consume(source)}";
        use crate::native_tests::boxed_enum_tests::{
            assert_success, run_counted_allocations_in_order,
        };
        for (program, entry) in [
            memory_unit_with_entry(consumer),
            unit_with_entry(&format!(
                "package app\nimport koven.algorithms.take\n{consumer}"
            )),
        ] {
            let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2]);
            assert_success(&run, b"first\nfirst\nfirst\nsecond\nfirst\nsecond\nfirst\n");
        }
    }

    #[test]
    fn range_view_source_nested_temporary_chain_is_consumed_synchronously() {
        let consumer = "fun source():List<String> =listOf(\"a\".clone())\nfun read(view:View<String>):Unit{for(item in view){println(item)}}\nfun main():Unit{read(take(take(source(),1),1));for(item in take(take(source(),1),1)){println(item)}}";
        memory_unit(consumer);
        unit(&format!(
            "package app\nimport koven.algorithms.take\n{consumer}"
        ));
    }

    #[test]
    fn range_view_source_verifier_rejects_premature_collection_release_after_parent_end() {
        let consumer = "fun read(view:View<String>):Unit{}\nfun main():Unit{val source=listOf(\"a\".clone());borrow val parent=take(source,1);borrow val child=take(parent,1);read(child)}";
        for mut program in [
            memory_unit(consumer),
            unit(&format!(
                "package app\nimport koven.algorithms.take\n{consumer}"
            )),
        ] {
            let caller = program.modules[0]
                .functions
                .iter_mut()
                .find(|function| {
                    function
                        .instructions
                        .iter()
                        .filter(|instruction| {
                            matches!(instruction.operation, Operation::RangeCall { .. })
                        })
                        .count()
                        == 2
                })
                .unwrap();
            let (parent, source) = caller
                .instructions
                .iter()
                .find_map(|instruction| {
                    match (&instruction.operation, instruction.results.as_slice()) {
                        (
                            Operation::RangeCall { source, .. },
                            [EntityId::Value(view), EntityId::Loan(_)],
                        ) => Some((*view, *source)),
                        _ => None,
                    }
                })
                .unwrap();
            let Definition::InstructionResult { instruction, .. } =
                caller.entity(EntityId::Loan(source)).unwrap().definition
            else {
                panic!("source loan");
            };
            let Operation::BorrowBegin { place, .. } =
                caller.instruction(instruction).unwrap().operation
            else {
                panic!("source place");
            };
            let Definition::InstructionResult { instruction, .. } =
                caller.entity(EntityId::Place(place)).unwrap().definition
            else {
                panic!("root place");
            };
            let Operation::RootPlace { owner } = caller.instruction(instruction).unwrap().operation
            else {
                panic!("collection owner");
            };
            caller.instructions.iter_mut().find(|instruction| matches!(instruction.operation, Operation::RangeEnd { view, .. } if view == parent)).unwrap().operation = Operation::Drop { owner };
            let errors = verify_program(&program)
                .expect_err("the child's independent root loan must still protect the collection");
            assert_diagnostic!(errors.errors.iter().any(|error| matches!(error.kind, crate::ssa::verify::VerifyErrorKind::OwnerLoanConflict { value } if value == owner)), "{errors:?}");
            // The rejected Program is never emitted or executed.
        }
    }

    #[test]
    fn range_view_source_verifier_rejects_a_forwarded_sibling_root() {
        let trusted = "fun choose(source:View<String>,sibling:View<String>):View<String> from source=take(source,1)";
        let consumer = "fun read(view:View<String>):Unit{}\nfun main():Unit{val left=listOf(\"a\");val right=listOf(\"b\");borrow val parent=take(left,1);borrow val sibling=take(right,1);borrow val child=choose(parent,sibling);read(child)}";
        for mut program in [
            memory_unit(&format!("{trusted}\n{consumer}")),
            unit_with_provider_entry(&format!("package app\nimport koven.algorithms.take\nimport koven.algorithms.choose\n{consumer}"), trusted).0,
        ] {
            let function = program.modules[0].functions.iter_mut().find(|function| {
                function.carrier_return == Some(0) && function.blocks[0].parameters.len() == 2 && function.blocks[0].parameters.iter().all(|parameter| {
                    matches!(function.entity(*parameter).unwrap().ty, EntityType::Loan { target, .. } if function.return_types == [target])
                })
            }).unwrap();
            let EntityId::Loan(sibling) = function.blocks[0].parameters[1] else { panic!("sibling input"); };
            let instruction = function.instructions.iter_mut().find(|instruction| matches!(instruction.operation, Operation::RangeCall { .. })).unwrap();
            let Operation::RangeCall { source, arguments, .. } = &mut instruction.operation else { unreachable!(); };
            *source = sibling;
            arguments[0] = EntityId::Loan(sibling);
            let errors = verify_program(&program).expect_err("the declared source must be the actual returned root");
            assert_diagnostic!(errors.errors.iter().any(|error| matches!(error.kind, crate::ssa::verify::VerifyErrorKind::ReturnType { .. })), "{errors:?}");
            // Signature-valid sibling substitution stays inside the pure verifier.
        }
    }

    #[test]
    fn range_view_source_nested_temporary_native_releases_each_original_collection_once() {
        use crate::native_tests::boxed_enum_tests::{
            assert_success, run_counted_allocations_in_order,
        };
        for (declaration, element, values, read, drops) in [
            (
                "",
                "String",
                "\"first\".clone(),\"second\".clone()",
                "println(item)",
                "",
            ),
            (
                "class Item(val text:String){}",
                "Item",
                "Item(\"first\"),Item(\"second\")",
                "println(item.text)",
                "",
            ),
            (
                "class Item(val text:String){deinit(){println(this.text)}}",
                "Item",
                "Item(\"first\"),Item(\"second\")",
                "println(item.text)",
                "second\nfirst\n",
            ),
        ] {
            for (count, exit) in [
                (0, ""),
                (1, ""),
                (2147483647, ""),
                (2, "break"),
                (2, "continue"),
                (2, "return"),
            ] {
                let consumer = format!(
                    "{declaration}\nfun source():List<{element}>{{println(\"source\");return listOf({values})}}\nfun read(view:View<{element}>):Unit{{for(item in view){{{read}}};println(\"read-end\")}}\nfun main():Unit{{read(take(take(source(),2),{count}));for(item in take(take(source(),2),{count})){{{read};{exit}}};println(\"done\")}}"
                );
                for (program, entry) in [
                    memory_unit_with_entry(&consumer),
                    unit_with_entry(&format!(
                        "package app\nimport koven.algorithms.take\n{consumer}"
                    )),
                ] {
                    let llvm =
                        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
                    let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2, 4, 3, 5]);
                    let visits = match count {
                        0 => "",
                        1 => "first\n",
                        _ => "first\nsecond\n",
                    };
                    let for_visits = if exit == "break" || exit == "return" {
                        "first\n"
                    } else {
                        visits
                    };
                    let done = if exit == "return" { "" } else { "done\n" };
                    assert_success(
                        &run,
                        format!(
                            "source\n{visits}read-end\n{drops}source\n{for_visits}{drops}{done}"
                        )
                        .as_bytes(),
                    );
                }
            }
        }
    }
}
