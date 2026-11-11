//! Normal same-source branch forwarding: establish real lowering before coverage claims.
use super::*;
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};
const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun wrap(source: String, flag: Boolean): borrow String from source { if (flag) { return view(source) }; return view(source) }\nfun consume(item: String): Unit {}\nfun run(flag: Boolean): Unit { val owner = \"kept\"; borrow val item = wrap(owner, flag); consume(item) }";
#[test]
fn ordinary_same_source_branch_requires_real_lowering_with_true_off() {
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "branch-forward.ko", SOURCE);
    let (environment, types) = standard_environments();
    let mut completed = Vec::new();
    for (layout, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "branch-forward.ko", source, &parsed),
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
            panic!("catalog")
        };
        assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
        assert!(catalog.forward_regions().is_empty());
        let [first, second] = on.ownership().borrow_forward_transports() else {
            panic!("two real returns")
        };
        assert_eq!(first.source_loan().target(), second.source_loan().target());
        assert_ne!(first.point(), second.point());
        assert_ne!(first.source_loan().call(), second.source_loan().call());
        let run = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap();
        for (setting, owned) in [("on", &on), ("off", &off)] {
            let (((result, routes), receipts), verification) =
                crate::ssa::unit_lower::observe_final_verifier(|| {
                    crate::ssa::refine::forward_receipt::observe(|| {
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
            assert_eq!(verification.len(), 1, "one original final verifier call");
            println!(
                "layout={layout}; observer={setting}; original_final_verifier={}",
                verification[0]
            );
            assert!(
                receipts.is_empty(),
                "branch remains outside expression receipt scope"
            );
            println!(
                "layout={layout}; observer={setting}; ownership_phase_succeeded=true; lowering_ok={}; routes={routes:?}",
                result.is_ok()
            );
            completed.push((layout, setting, result, routes));
        }
    }
    let mut previous = None;
    for (layout, setting, lowered, routes) in completed {
        let (program, _) = lowered.unwrap_or_else(|error| {
            panic!("{layout}/{setting}: normal same-source branch lowering required: {error:?}")
        });
        verify_program(&program).unwrap();
        let wrapper = program.modules[0]
            .functions
            .iter()
            .find(|f| f.name.contains(".wrap."))
            .unwrap();
        assert!(routes.contains(&wrapper.name), "real refine route");
        let view = program.modules[0]
            .functions
            .iter()
            .find(|f| f.name.contains(".view."))
            .unwrap();
        let [EntityId::Loan(source), EntityId::Loan(flag)] =
            wrapper.blocks[0].parameters.as_slice()
        else {
            panic!("two original borrowed entry parameters")
        };
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &wrapper.blocks[0].terminator.as_ref().unwrap().kind
        else {
            panic!("dynamic condition")
        };
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
        let first = check_branch(wrapper, when_true, *source, view.id());
        let second = check_branch(wrapper, when_false, *source, view.id());
        assert_ne!(
            first, second,
            "distinct calls and return exits, not one linear protocol"
        );
        assert_eq!(
            wrapper
                .instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::BorrowCall { .. }))
                .count(),
            2
        );
        assert_eq!(
            wrapper
                .blocks
                .iter()
                .filter(|b| matches!(
                    b.terminator.as_ref().map(|t| &t.kind),
                    Some(TerminatorKind::BorrowReturn { .. })
                ))
                .count(),
            2
        );
        assert!(!wrapper.instructions.iter().any(|i| matches!(
            i.operation,
            Operation::BorrowBegin { .. } | Operation::BorrowEnd { .. } | Operation::Drop { .. }
        )));
        let llvm = crate::llvm::render_verified_program(&program).unwrap();
        println!(
            "layout={layout}; observer={setting}; branch_ssa_verified=true; llvm_bytes={}",
            llvm.len()
        );
        let output = (crate::ssa::render_program(&program), llvm);
        if setting == "on" {
            previous = Some(output);
        } else {
            assert_eq!(previous.take().unwrap(), output);
        }
    }
}

fn check_branch(
    function: &Function,
    first: &Edge,
    source: LoanId,
    callee: FunctionId,
) -> (BlockId, InstructionId) {
    let mut edge = first;
    let mut source = source;
    let mut call_result = None;
    for _ in 0..function.blocks.len() {
        let block = function.block(edge.target).unwrap();
        let positions = edge
            .arguments
            .iter()
            .enumerate()
            .filter(|(_, e)| **e == EntityId::Loan(source))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let [position] = positions.as_slice() else {
            panic!("one explicit same-source edge transport")
        };
        let EntityId::Loan(next) = block.parameters[*position] else {
            panic!("transported entry loan")
        };
        source = next;
        for instruction in function
            .instructions
            .iter()
            .filter(|i| i.block == edge.target)
        {
            let Operation::BorrowCall {
                callee: actual,
                arguments,
                source: actual_source,
            } = &instruction.operation
            else {
                panic!("only original forward call on return path")
            };
            assert_eq!(*actual, callee);
            assert_eq!(*actual_source, source);
            assert_eq!(arguments, &[EntityId::Loan(source)]);
            let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                panic!("borrowed call result")
            };
            assert_eq!(result.function(), function.id());
            assert_ne!(*result, source);
            assert!(call_result.replace((*result, instruction.id)).is_none());
        }
        match &block.terminator.as_ref().unwrap().kind {
            TerminatorKind::Branch(next) => edge = next,
            TerminatorKind::BorrowReturn { loan } => {
                let (result, instruction) = call_result.unwrap();
                assert_eq!(*loan, result);
                return (edge.target, instruction);
            }
            other => panic!("unexpected nested/other path: {other:?}"),
        }
    }
    panic!("acyclic bounded branch never reached return")
}

#[test]
fn ordinary_branch_neighbors_keep_the_unit_declaration_gate() {
    for body in [
        "{ return view(source) }",
        "{ if (flag) { return view(source) }; return ((view(source))) }",
        "{ if (flag) { return view(source) } else { return view(source) } }",
    ] {
        let text = format!(
            "fun view(source: String): borrow String from source = source\nfun wrap(source: String, flag: Boolean): borrow String from source {body}\nfun run(): Unit {{}}"
        );
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "branch-neighbor.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "branch-neighbor.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
        ] {
            let inputs = [input];
            let (names, typed, owned) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            let run = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap();
            let error = crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                &owned,
                run.id(),
            )
            .err()
            .expect("unadmitted block must stay rejected");
            assert_eq!(
                error.kind,
                crate::ssa::lowering_support::LoweringErrorKind::UnsupportedNode
            );
            let span = error.span.expect("borrow marker frontier");
            assert_eq!(sources.slice(span).unwrap(), "borrow");
            assert!(
                owned
                    .ownership()
                    .loan_action_observations()
                    .iter()
                    .all(|c| c.forward_regions().is_empty())
            );
        }
    }
}
