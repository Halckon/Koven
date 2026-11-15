//! Real branch inputs; no mutation, synthetic facts, execution or alternate analysis mixing.
use super::*;
use crate::ssa::{unit_lower, unit_lower_test_support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
#[test]
fn original_both_arm_paths_close_with_distinct_callees_and_run_isolation() {
    for reverse in [false, true] {
        let audits = run(false, reverse);
        assert_eq!(audits.len(), 2);
        for audit in audits {
            assert_eq!(audit.expected.len(), 1);
            assert_eq!(audit.linked, audit.expected);
            assert!(audit.gaps.is_empty(), "{:?}", audit.gaps);
        }
    }
}
#[test]
fn original_tail_continuation_closes_actual_fold_with_distinct_callees() {
    for reverse in [false, true] {
        let audits = run(true, reverse);
        assert_eq!(audits.len(), 2);
        for audit in audits {
            assert_eq!(audit.expected.len(), 1);
            assert_eq!(audit.linked, audit.expected);
            assert!(audit.gaps.is_empty(), "{:?}", audit.gaps);
        }
    }
}
#[test]
fn certified_branch_and_identity_edges_close_wrapper_plan() {
    for tail in [false, true] {
        let audits = run_entry(tail, true, true, false);
        assert_eq!(audits.len(), 2);
        for audit in audits {
            assert!(audit.gaps.is_empty());
        }
    }
}
#[test]
fn extra_entry_parameter_keeps_actual_call_outside_certificate_domain() {
    for reverse in [false, true] {
        let audits = run_entry(false, reverse, false, true);
        assert_eq!(audits.len(), 2);
        for audit in audits {
            assert!(audit.gaps.is_empty(), "{:?}", audit.gaps);
        }
    }
}
fn run(tail: bool, reverse: bool) -> Vec<BranchConsumption> {
    run_entry(tail, reverse, false, false)
}
fn run_entry(
    tail: bool,
    reverse: bool,
    wrapper_entry: bool,
    extra_parameter: bool,
) -> Vec<BranchConsumption> {
    run_entry_uses(tail, reverse, wrapper_entry, extra_parameter, false, false)
}
fn run_entry_uses(
    tail: bool,
    reverse: bool,
    wrapper_entry: bool,
    extra_parameter: bool,
    twice: bool,
    distinct: bool,
) -> Vec<BranchConsumption> {
    let mut sources = SourceMap::new();
    let (provider_id, provider) = unit_lower_test_support::parsed(
        &mut sources,
        "provider.ko",
        "fun view_left(source: String): borrow String from source = source\nfun view_right(source: String): borrow String from source = source",
    );
    let text = "fun wrap(source: String, flag: Boolean): borrow String from source { if (flag) { return view_left(source) } else { return view_right(source) } }\nfun consume(item: String): Unit {}\nfun run(flag: Boolean): Unit { val owner = \"kept\"; borrow val item = wrap(owner, flag); consume(item) }";
    let text = if tail {
        text.replace(
            "} else { return view_right(source) }",
            "}; return view_right(source)",
        )
    } else {
        text.to_owned()
    };
    let text = if extra_parameter {
        text.replace(
            "fun run(flag: Boolean)",
            "fun run(flag: Boolean, unused: Boolean)",
        )
    } else {
        text
    };
    let text = if twice {
        let text = text.replace(
            "consume(item) }",
            if distinct {
                "consume(item); consume_other(item) }"
            } else {
                "consume(item); consume(item) }"
            },
        );
        if distinct {
            text.replace(
                "fun consume(item: String): Unit {}",
                "fun consume(item: String): Unit {}\nfun consume_other(item: String): Unit {}",
            )
        } else {
            text
        }
    } else {
        text
    };
    let (caller_id, caller) = unit_lower_test_support::parsed(&mut sources, "caller.ko", &text);
    let mut inputs = [
        SourceUnitInput::new("root", "provider.ko", provider_id, &provider),
        SourceUnitInput::new("root", "caller.ko", caller_id, &caller),
    ];
    if reverse {
        inputs.reverse();
    }
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let evidence: Vec<_> = owned
        .ownership()
        .loan_action_observations()
        .iter()
        .flat_map(|observation| {
            observation
                .entry_binding_evidence()
                .iter()
                .map(move |e| (observation, e))
        })
        .collect();
    assert_eq!(evidence.len(), usize::from(!extra_parameter));
    for (observation, evidence) in evidence {
        use lang_frontend::ownership_checking::UnitLoanAction;
        assert!(observation.owns_entry_binding_evidence(evidence));
        assert!(!evidence.ambiguous_use());
        assert_eq!(evidence.uses().len(), if twice { 2 } else { 1 });
        let ordinal = |id| {
            (0..observation.actions().len())
                .find(|&index| observation.action_id(index).as_ref() == Some(id))
                .unwrap()
        };
        let mut previous = ordinal(evidence.handoff().unwrap());
        let end = ordinal(evidence.binding_end().unwrap());
        for use_event in evidence.uses() {
            let returned = use_event.returned().unwrap();
            assert_eq!(returned.call(), use_event.consumer());
            let begin = use_event.begin().unwrap();
            assert!(
                matches!(observation.action(begin),Some(UnitLoanAction::Begin(actual)) if actual==returned)
            );
            let index = ordinal(begin);
            assert!(previous < index && index < end);
            previous = index;
        }
        if twice {
            assert_ne!(evidence.uses()[0].consumer(), evidence.uses()[1].consumer());
        }

        assert_eq!(
            owned
                .ownership()
                .d10_entry_borrow_link(evidence.transaction().source_loan().call()),
            Some(evidence.callable())
        );
        let source = observation
            .action(evidence.source_begin().expect("actual source begin"))
            .unwrap();
        assert!(
            matches!(source, UnitLoanAction::Begin(loan) if loan == evidence.transaction().source_loan())
        );
        assert!(
            matches!(observation.action(evidence.handoff().expect("actual handoff")), Some(UnitLoanAction::BorrowHandoffCommitted(h)) if h.binding() == evidence.transaction().binding())
        );
        let returned = evidence.returned().expect("actual consumer return");
        assert_eq!(returned.call(), evidence.consumer());
        assert!(
            matches!(observation.action(evidence.use_begin().expect("actual consumer begin")), Some(UnitLoanAction::Begin(loan)) if loan == returned)
        );
        assert!(
            matches!(observation.action(evidence.binding_end().expect("actual binding end")), Some(UnitLoanAction::BorrowBindingEnd(end)) if end.point() == evidence.transaction().point() && end.binding() == evidence.transaction().binding())
        );
        assert!(
            !observation.actions().iter().any(|a| matches!(
                a,
                UnitLoanAction::BorrowBindingUseReturned(_)
                    | UnitLoanAction::BorrowBindingSourceRetired(_)
            )),
            "legacy actions are not broadened"
        );
    }
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == if wrapper_entry { "wrap" } else { "run" })
        .unwrap()
        .id();
    let expected_item = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "wrap")
        .unwrap();
    let item = UnitItemId::new(expected_item.source_unit(), expected_item.root());
    let regions: Vec<_> = owned
        .ownership()
        .loan_action_observations()
        .iter()
        .flat_map(|c| c.branch_forward_regions())
        .collect();
    assert_eq!(regions.len(), 1);
    assert_eq!(
        owned.ownership().d10_branch_link(regions[0].split()),
        Ok(item)
    );
    let ((results, receipts), audits) = crate::ssa::d10_project_audit::observe(|| {
        crate::ssa::refine::branch_receipt::observe(|| {
            (0..2)
                .map(|_| {
                    unit_lower::lower_scalar_unit_with_entry(
                        &sources, &inputs, &names, &types, &typed, &owned, entry,
                    )
                })
                .collect::<Vec<_>>()
        })
    });
    // Existing publication remains one receipt per original successful assembly.
    assert_eq!(receipts.len(), 2);
    for (receipt, result) in receipts.into_iter().zip(results) {
        assert_ne!(receipt.paths[0].callee, receipt.paths[1].callee);
        assert_eq!(receipt.folded_distinct_blocks(), tail);
        assert!(receipt.paths[0].branch.then_arm());
        assert!(!receipt.paths[1].branch.then_arm());
        let (program, entry_function) = result.unwrap();
        if !wrapper_entry && !extra_parameter {
            use crate::ssa::model::Operation;
            let caller = program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .find(|f| f.id == entry_function)
                .unwrap();
            assert_eq!(caller.blocks.len(), 1);
            assert_eq!(caller.instructions.len(), if twice { 9 } else { 8 });
            assert_eq!(
                caller
                    .instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::BorrowBegin { .. }))
                    .count(),
                1,
                "Boolean flag is passed by value; no auxiliary SSA loan"
            );
            assert_eq!(
                caller
                    .instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::BorrowEnd { .. }))
                    .count(),
                2,
                "result then continued source retirements only"
            );
        }

        for path in &receipt.paths {
            assert_ne!(path.callee, entry_function);
            assert_ne!(
                path.callee.index, 0,
                "identity evidence must support nonzero leaf IDs"
            );
        }
        let function = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.id == receipt.caller)
            .unwrap();
        assert_eq!(function.blocks.len(), 3);
        let branch = function
            .block(receipt.entry)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap();
        let crate::ssa::model::TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } = &branch.kind
        else {
            panic!("original dynamic branch");
        };
        assert_eq!(when_true, &receipt.paths[0].edge);
        assert_eq!(when_false, &receipt.paths[1].edge);
        for path in &receipt.paths {
            assert_eq!(
                function
                    .block(path.block)
                    .unwrap()
                    .terminator
                    .as_ref()
                    .unwrap()
                    .kind,
                crate::ssa::model::TerminatorKind::BorrowReturn { loan: path.result }
            );
        }
    }
    let declaration = |name| {
        names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == name)
            .unwrap()
            .id()
    };
    for audit in &audits {
        if twice && !wrapper_entry {
            assert!(
                audit
                    .direct_edges
                    .contains(&(declaration("run"), declaration("consume")))
            );
            if distinct {
                assert!(
                    audit
                        .direct_edges
                        .contains(&(declaration("run"), declaration("consume_other")))
                );
            }
        }

        let mut edges = audit.borrow_edges.clone();
        edges.sort();
        let mut expected_edges = vec![
            (declaration("wrap"), declaration("view_left")),
            (declaration("wrap"), declaration("view_right")),
        ];
        if !wrapper_entry && !extra_parameter {
            expected_edges.push((declaration("run"), declaration("wrap")));
        }
        expected_edges.sort();
        assert_eq!(edges, expected_edges);
        assert_eq!(
            audit
                .unsupported
                .iter()
                .filter(|g| g.contains("unclassified terminator: Some(BorrowReturn"))
                .count(),
            0,
            "identity leaf returns require their independent certificate: {:?}",
            audit.unsupported
        );
        assert_eq!(
            audit
                .unsupported
                .iter()
                .filter(|g| g.contains("unclassified operation: BorrowCall"))
                .count(),
            usize::from(extra_parameter),
            "run -> wrap requires its independent actual entry certificate"
        );
        assert_eq!(
            audit
                .unsupported
                .iter()
                .any(|g| g.contains("entry/certified-edge closure")),
            extra_parameter
        );
        assert_eq!(
            audit
                .unsupported
                .iter()
                .filter(|g| g.contains("entry binding lifetime unproven"))
                .count(),
            0,
            "{:?}",
            audit.unsupported
        );
        assert!(
            !audit.unsupported.iter().any(|g| g.contains("identity")),
            "{:?}",
            audit.unsupported
        );
        assert_eq!(
            audit
                .unsupported
                .iter()
                .filter(|g| g.contains("missing/duplicate entry call receipt"))
                .count(),
            usize::from(extra_parameter),
            "{:?}",
            audit.unsupported
        );
        assert!(
            !audit
                .unsupported
                .iter()
                .any(|g| g.contains("unconsumed certified"))
        );
        if !extra_parameter {
            assert_eq!(audit.unsupported.len(), 0, "{:?}", audit.unsupported);
        }
        if wrapper_entry {
            assert!(audit.direct_edges.is_empty());
        }
    }
    audits.into_iter().map(|a| a.branch_consumption).collect()
}

#[test]
fn entry_evidence_rejects_extra_statements_multiple_uses_and_control_flow() {
    for body in [
        r#"val owner = "kept"; consume(owner); borrow val item = wrap(owner, flag); consume(item)"#,
        r#"val owner = "kept"; borrow val item = wrap(owner, flag); consume(item); consume(item); consume(item)"#,
        r#"val owner = "kept"; borrow val item = wrap(owner, flag); if(flag) { consume(item) } else { consume(item) }"#,
    ] {
        let mut sources = SourceMap::new();
        let text = format!(
            "fun wrap(source:String, flag:Boolean):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun run(flag:Boolean):Unit {{{body}}}"
        );
        let (id, parsed) = unit_lower_test_support::parsed(&mut sources, "entry-domain.ko", &text);
        let inputs = [SourceUnitInput::memory_document(id, &parsed)];
        let (environment, types) = standard_environments();
        let (_, _, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        assert!(
            owned
                .ownership()
                .loan_action_observations()
                .iter()
                .all(|o| o.entry_binding_evidence().is_empty()),
            "{body}"
        );
    }
}

#[test]
fn entry_lifetime_keeps_unsupported_transfer_and_lifetime_gaps() {
    for body in [
        r#"val owner="kept"; borrow val item=wrap(owner,flag); consume(item); return"#,
        r#"val owner="kept"; borrow val item=wrap(owner,flag); consume(item); consume(item); consume(item)"#,
        r#"val owner="kept"; borrow val item=wrap(owner,flag); consume(item); val n=1; consume(item)"#,
        r#"val owner="kept"; borrow val item=wrap(owner,flag); if(flag){consume(item)}else{consume(item)}"#,
    ] {
        let mut sources = SourceMap::new();
        let text = format!(
            "fun identity(source:String):borrow String from source=source\nfun wrap(source:String,flag:Boolean):borrow String from source {{if(flag){{return identity(source)}}else{{return identity(source)}}}}\nfun consume(item:String):Unit {{}}\nfun run(flag:Boolean):Unit {{{body}}}"
        );
        let (id, parsed) = unit_lower_test_support::parsed(&mut sources, "lifetime-gap.ko", &text);
        let inputs = [SourceUnitInput::memory_document(id, &parsed)];
        let (environment, types) = standard_environments();
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap()
            .id();
        let (result, audits) = crate::ssa::d10_project_audit::observe(|| {
            unit_lower::lower_scalar_unit_with_entry(
                &sources, &inputs, &names, &types, &typed, &owned, entry,
            )
        });
        if body.ends_with("return") || body.contains("if(flag)") {
            let Err(error) = result else {
                panic!("existing control-transfer/branch route stays unsupported");
            };
            assert_eq!(
                error.kind,
                crate::ssa::lowering_support::LoweringErrorKind::UnsupportedNode
            );
            assert_eq!(
                sources.slice(error.span.unwrap()),
                Ok(if body.ends_with("return") {
                    "return"
                } else {
                    "if(flag){consume(item)}else{consume(item)}"
                })
            );
            assert!(
                audits.is_empty(),
                "failed lowering publishes no successful audit"
            );
            continue;
        }
        let (program, _) = result.unwrap_or_else(|e| panic!("normal source {body}: {e:?}"));
        crate::ssa::verify::verify_program(&program).unwrap();
        let [audit] = audits.as_slice() else {
            panic!("one assembly");
        };
        assert!(
            audit
                .unsupported
                .iter()
                .any(|g| g.contains("entry binding lifetime unproven")),
            "{body}: {:?}",
            audit.unsupported
        );
        assert!(
            !audit
                .unsupported
                .iter()
                .any(|g| g.contains("missing/duplicate entry call receipt")),
            "{body}: {:?}",
            audit.unsupported
        );
        let wrap = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "wrap")
            .unwrap()
            .id();
        assert!(
            audit.borrow_edges.contains(&(entry, wrap)),
            "{body}: {:?}",
            audit.borrow_edges
        );
        assert!(
            !audit
                .unsupported
                .iter()
                .any(|g| g.contains("entry final call identity gap")),
            "{:?}",
            audit.unsupported
        );
    }
}

#[test]
fn two_ordered_consumers_share_result_until_single_retirement() {
    for distinct in [false, true] {
        for reverse in [false, true] {
            for tail in [false, true] {
                let audits = run_entry_uses(tail, reverse, false, false, true, distinct);
                assert_eq!(audits.len(), 2);
                assert!(audits.iter().all(|a| a.gaps.is_empty()));
            }
        }
    }
}
