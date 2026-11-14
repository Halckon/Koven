//! An owned Boolean condition keeps execution independent of optional receipt coverage.
use super::*;
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};
#[test]
fn ordinary_local_loop_owned_condition_keeps_observer_parity() {
    let text = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(own flag: Boolean): Unit {val source=\"value\";borrow val item=view(source);while(flag){consume(item);consume(item)}}";
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "owned-condition.ko", text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "owned-condition.ko", source, &parsed),
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
        assert_eq!(
            on.ownership()
                .loan_action_observations()
                .iter()
                .flat_map(|c| c.loop_invariant_lineages())
                .count(),
            1,
            "FE Partial does not impose a new condition-mode language restriction"
        );
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap()
            .id();
        let mut previous = None;
        for owned in [&on, &off] {
            let ((result, verified), receipts) = crate::ssa::refine::loop_receipt::observe(|| {
                crate::ssa::unit_lower::observe_final_verifier(|| {
                    crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                        &sources, &inputs, &names, &types, &typed, owned, entry,
                    )
                })
            });
            let (program, _) = result.unwrap();
            assert_eq!(verified.len(), 1);
            assert!(
                receipts.is_empty(),
                "this receipt requires a Borrow Boolean input, not a forged flag loan"
            );
            verify_program(&program).unwrap();
            let run = program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .find(|f| f.name.contains(".run."))
                .unwrap();
            assert_eq!(
                run.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::BorrowBegin { .. }))
                    .count(),
                1
            );
            assert_eq!(
                run.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::BorrowCall { .. }))
                    .count(),
                1
            );
            assert_eq!(
                run.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
                    .count(),
                2
            );
            super::invariant_outer_while_tests::assert_transport(run, true, "flag", None, 0);
            let rendered = format!(
                "{verified:?}\n{}",
                crate::llvm::render_verified_program(&program).unwrap()
            );
            if let Some(previous) = previous.as_ref() {
                assert_eq!(&rendered, previous);
            }
            previous = Some(rendered);
        }
    }
}
