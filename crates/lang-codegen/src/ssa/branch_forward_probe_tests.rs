//! Normal same-source branch forwarding: establish real lowering before coverage claims.
use super::*;
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};
const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun wrap(source: String, flag: Boolean): borrow String from source { if (flag) { return view(source) }; return view(source) }\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = wrap(owner, true); consume(item) }";
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
            let (result, routes) = crate::ssa::refine::observe_routing(|| {
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
            println!(
                "layout={layout}; observer={setting}; ownership_complete=true; lowering_ok={}; routes={routes:?}",
                result.is_ok()
            );
            completed.push((layout, setting, result));
        }
    }
    let mut previous = None;
    for (layout, setting, lowered) in completed {
        let (program, _) = lowered.unwrap_or_else(|error| {
            panic!("{layout}/{setting}: normal same-source branch lowering required: {error:?}")
        });
        verify_program(&program).unwrap();
        let wrapper = program.modules[0]
            .functions
            .iter()
            .find(|f| f.name.contains(".wrap."))
            .unwrap();
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
