//! Ordinary finite counter loop bodies; no test-side state corruption.
use super::*;
#[cfg(feature = "loan-observation-test-support")]
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};
#[test]
fn ordinary_counter_loop_arm_closure_uses_real_countdown() {
    for own in [false, true] {
        let mode = if own { "own " } else { "" };
        let text = format!(
            "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun run(source:String,{mode}rounds:Int,choose:Boolean):Unit {{var remaining=rounds;while(remaining>0){{if(choose){{borrow val a=view(source);consume(a)}}else{{borrow val b=view(source);consume(b);consume(b)}};remaining=remaining-1}}}}"
        );
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "counter-arms.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "counter-arms.ko", source, &parsed),
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
                assert_eq!(owned.ownership().no_carry_counter_branches().len(), 1);
                assert!(owned.ownership().no_carry_branches().is_empty());
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
                let (program, _) = result.expect("finite two-arm counter lowering");
                assert_eq!(receipts.len(), 1);
                assert_eq!(trace.before_verifier, 1);
                assert_eq!(trace.pending_after, 0);
                verify_program(&program).unwrap();
                let run = program
                    .modules
                    .iter()
                    .flat_map(|m| &m.functions)
                    .find(|f| f.name.contains(".run."))
                    .unwrap();
                receipts[0].validate(run).unwrap();
                let rendered = super::super::render_program(&program);
                if let Some(old) = &baseline {
                    assert_eq!(&rendered, old);
                } else {
                    baseline = Some(rendered);
                }
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
                        .filter(|i| matches!(
                            i.operation,
                            Operation::CheckedArithmetic {
                                operator: CheckedArithmeticOperator::Subtract,
                                ..
                            }
                        ))
                        .count(),
                    1
                );
            }
        }
    }
}
