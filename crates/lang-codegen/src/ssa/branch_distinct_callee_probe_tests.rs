//! Real distinct-callee branch forwarding, preserving per-path identity.
use super::*;
use lang_frontend::ownership_checking::{
    UnitBorrowBranchForwardTopology, check_compilation_unit_ownership, legacy_ownership_eq,
    with_loan_observation,
};
const SOURCE: &str = "fun view_left(source: String): borrow String from source = source\nfun view_right(source: String): borrow String from source = source\nfun wrap(source: String, flag: Boolean): borrow String from source { if (flag) { return view_left(source) } else { return view_right(source) } }\nfun consume(item: String): Unit {}\nfun run(flag: Boolean): Unit { val owner = \"kept\"; borrow val item = wrap(owner, flag); consume(item) }";
#[test]
fn ordinary_distinct_callees_both_return() {
    check(UnitBorrowBranchForwardTopology::BothArmsReturn);
}
#[test]
fn ordinary_distinct_callees_tail_continuation() {
    check(UnitBorrowBranchForwardTopology::TailContinuation);
}
fn check(topology: UnitBorrowBranchForwardTopology) {
    let text = match topology {
        UnitBorrowBranchForwardTopology::BothArmsReturn => SOURCE.to_owned(),
        UnitBorrowBranchForwardTopology::TailContinuation => SOURCE.replace(
            "} else { return view_right(source) }",
            "}; return view_right(source)",
        ),
    };
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "branch-distinct.ko", &text);
    let (environment, types) = standard_environments();
    let mut completed = Vec::new();
    for (layout, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "branch-distinct.ko", source, &parsed),
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
        assert!(off.ownership().loan_action_observations().is_empty());
        let [catalog] = on.ownership().loan_action_observations() else {
            panic!("one catalog")
        };
        assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
        // Capture all four real lowering attempts before asserting frontend coverage.
        let expected = catalog
            .branch_forward_regions()
            .iter()
            .filter(|r| {
                r.coverage() == UnitLoanActionCoverage::Complete
                    && r.topology() == topology
                    && catalog.owns_branch_forward_region(r)
            })
            .flat_map(|r| {
                r.paths()
                    .iter()
                    .map(|p| (p.branch(), p.action_ids().collect::<Vec<_>>()))
            })
            .collect::<Vec<_>>();
        let run = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap();
        for (setting, owned) in [("on", &on), ("off", &off)] {
            let ((lowered, verification), receipts) =
                crate::ssa::refine::branch_receipt::observe(|| {
                    crate::ssa::unit_lower::observe_final_verifier(|| {
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
            println!(
                "{layout}/{setting}: lowering={:?}, original_verifier_calls={}, receipts={}",
                lowered.as_ref().map(|_| ()),
                verification.len(),
                receipts.len()
            );
            completed.push((
                layout,
                setting,
                lowered,
                verification,
                receipts,
                expected.clone(),
            ));
        }
    }
    let mut previous = None;
    for (layout, setting, lowered, verification, receipts, expected) in completed {
        let (program, _) = lowered.unwrap_or_else(|e| {
            panic!("{layout}/{setting}: real exhaustive branch lowering: {e:?}")
        });
        assert_eq!(verification.len(), 1);
        verify_program(&program).unwrap();
        let wrapper = program.modules[0]
            .functions
            .iter()
            .find(|f| f.name.contains(".wrap."))
            .unwrap();
        let left = program.modules[0]
            .functions
            .iter()
            .find(|f| f.name.contains(".view_left."))
            .unwrap();
        let right = program.modules[0]
            .functions
            .iter()
            .find(|f| f.name.contains(".view_right."))
            .unwrap();
        let callees = [left.id(), right.id()];
        assert_ne!(callees[0], callees[1]);
        assert_eq!(expected.len(), 2, "two Complete frontend paths");
        assert_eq!(wrapper.blocks.len(), 3, "exact final branch topology");
        let [EntityId::Loan(source), EntityId::Loan(flag)] =
            wrapper.blocks[0].parameters.as_slice()
        else {
            panic!("two borrowed entry parameters")
        };
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &wrapper.blocks[0].terminator.as_ref().unwrap().kind
        else {
            panic!("real dynamic split")
        };
        assert_ne!(when_true.target, when_false.target);
        let read = wrapper
            .instructions
            .iter()
            .find(|i| i.results == [EntityId::Value(*condition)])
            .unwrap();
        assert_eq!(
            read.operation,
            Operation::Read {
                source: PlaceAccess::Loan(*flag)
            }
        );
        let mut actual = Vec::new();
        for (i, edge) in [when_true, when_false].into_iter().enumerate() {
            let block = wrapper.block(edge.target).unwrap();
            let source_pos = edge
                .arguments
                .iter()
                .position(|e| *e == EntityId::Loan(*source))
                .unwrap();
            let EntityId::Loan(path_source) = block.parameters[source_pos] else {
                panic!("source edge parameter")
            };
            let TerminatorKind::BorrowReturn { loan: result } =
                block.terminator.as_ref().unwrap().kind
            else {
                panic!("both arms return directly")
            };
            let call = wrapper
                .instructions
                .iter()
                .find(|i| i.results == [EntityId::Loan(result)])
                .unwrap();
            assert_eq!(
                call.operation,
                Operation::BorrowCall {
                    callee: callees[i],
                    arguments: vec![EntityId::Loan(path_source)],
                    source: path_source
                }
            );
            assert_ne!(path_source, result);
            actual.push((edge, call.id, path_source, result));
        }
        assert_eq!(
            wrapper
                .instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::BorrowCall { .. }))
                .count(),
            2
        );
        assert!(!wrapper.instructions.iter().any(|i| matches!(
            i.operation,
            Operation::BorrowBegin { .. } | Operation::BorrowEnd { .. } | Operation::Drop { .. }
        )));
        let output = (
            crate::ssa::render_program(&program),
            crate::llvm::render_verified_program(&program).unwrap(),
        );
        if setting == "on" {
            previous = Some(output);
            let [receipt] = receipts.as_slice() else {
                panic!("{layout}: one aggregate receipt required")
            };
            assert_eq!(receipt.paths.len(), 2);
            assert_eq!(receipt.topology, topology);
            assert_eq!(
                receipt.folded_distinct_blocks(),
                topology == UnitBorrowBranchForwardTopology::TailContinuation
            );
            assert_eq!(receipt.caller, wrapper.id());
            assert_eq!(receipt.source, *source);
            assert_eq!(receipt.flag, *flag);
            assert_eq!(receipt.read, read.id);
            assert_eq!(receipt.entry, wrapper.blocks[0].id);
            assert_eq!(receipt.condition_value, *condition);
            assert_eq!(receipt.callable, expected[0].0.callable());
            assert_eq!(receipt.split, expected[0].0.split());
            assert_eq!(receipt.condition, expected[0].0.condition());
            for path in &receipt.paths {
                let i = usize::from(!path.branch.then_arm());
                let (edge, call, path_source, result) = actual[i];
                assert_eq!(&path.edge, edge);
                assert_eq!(path.block, edge.target);
                assert_eq!(path.instruction, call);
                assert_eq!(path.source, path_source);
                assert_eq!(path.result, result);
                assert_eq!(path.callee, callees[i]);
                let matching = expected
                    .iter()
                    .filter(|(branch, _)| *branch == path.branch)
                    .collect::<Vec<_>>();
                assert_eq!(matching.len(), 1);
                assert_eq!(path.ids, matching[0].1);
            }
            assert_ne!(
                receipt.paths[0].branch.then_arm(),
                receipt.paths[1].branch.then_arm()
            );
        } else {
            assert!(receipts.is_empty());
            assert_eq!(previous.as_ref(), Some(&output));
        }
    }
}
