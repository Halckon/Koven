//! Ordinary actual zero/one-iteration break paths; no injected faults.
use super::*;
#[cfg(feature = "loan-observation-test-support")]
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};
#[test]
fn ordinary_loop_arm_tail_break_closes_before_actual_exit() {
    for own in [false, true] {
        let mode = if own { "own " } else { "" };
        let text = format!(
            "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun run({mode}choose:Boolean,source:String,{mode}flag:Boolean):Unit {{while(flag){{if(choose){{borrow val a=view(source);consume(a)}}else{{borrow val b=view(source);consume(b);consume(b)}};break}};consume(source)}}"
        );
        let mut sources = SourceMap::new();
        let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "arm-break.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "arm-break.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
        ] {
            let inputs = [input];
            let (names, typed, owned) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            #[cfg(feature = "loan-observation-test-support")]
            let off = with_loan_observation(false, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .validate()
                    .unwrap()
            });
            #[cfg(feature = "loan-observation-test-support")]
            assert!(legacy_ownership_eq(owned.ownership(), off.ownership()));
            #[cfg(feature = "loan-observation-test-support")]
            let products = [&owned, &off];
            #[cfg(not(feature = "loan-observation-test-support"))]
            let products = [&owned];
            let mut baseline = None;
            for owned in products {
                assert_eq!(owned.ownership().no_carry_break_branches().len(), 1);
                assert!(owned.ownership().no_carry_branches().is_empty());
                assert!(owned.ownership().no_carry_counter_branches().is_empty());
                assert!(owned.ownership().no_carry_loops().is_empty());
                let entry = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.name() == "run")
                    .unwrap()
                    .id();
                let (result, receipts, trace) =
                    crate::ssa::refine::arm_loop_receipt::observe(|| {
                        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                            &sources, &inputs, &names, &types, &typed, owned, entry,
                        )
                    });
                let (program, _) = result.expect("actual closed-arm break");
                verify_program(&program).unwrap();
                assert_eq!(receipts.len(), 1);
                assert_eq!(trace.before_verifier, 1);
                assert_eq!(trace.pending_after, 0);
                let run = program
                    .modules
                    .iter()
                    .flat_map(|m| &m.functions)
                    .find(|f| f.id == receipts[0].caller)
                    .unwrap();
                receipts[0].validate(run).unwrap();
                assert_eq!(
                    run.instructions
                        .iter()
                        .filter(|i| matches!(i.operation, Operation::BorrowCall { .. }))
                        .count(),
                    2
                );
                assert_eq!(
                    run.instructions
                        .iter()
                        .filter(|i| matches!(i.operation, Operation::BorrowEnd { .. }))
                        .count(),
                    2
                );
                let rendered = super::super::render_program(&program);
                if let Some(old) = &baseline {
                    assert_eq!(old, &rendered)
                } else {
                    baseline = Some(rendered)
                }
            }
        }
    }
}
