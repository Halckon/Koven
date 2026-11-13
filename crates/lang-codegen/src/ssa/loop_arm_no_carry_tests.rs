//! Ordinary branch-local no-carry source and independent SSA evidence.
use super::*;
#[cfg(feature = "loan-observation-test-support")]
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};
#[test]
fn ordinary_loop_arm_no_carry_closes_each_arm_before_join() {
    for reversed in [false, true] {
        let params = if reversed {
            "choose:Boolean,flag:Boolean,source:String"
        } else {
            "source:String,flag:Boolean,choose:Boolean"
        };
        let text = format!(
            "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun run({params}):Unit {{while(flag){{if(choose){{borrow val a=view(source);consume(a)}}else{{borrow val b=view(source);consume(b);consume(b)}}}}}}"
        );
        let mut sources = SourceMap::new();
        let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "loop-arm.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "loop-arm.ko", source, &parsed),
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
            let mut baseline = None;
            #[cfg(feature = "loan-observation-test-support")]
            let products = [&owned, &off];
            #[cfg(not(feature = "loan-observation-test-support"))]
            let products = [&owned];
            for owned in products {
                assert_eq!(owned.ownership().no_carry_branches().len(), 1);
                assert!(owned.ownership().no_carry_loops().is_empty());
                let entry = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.name() == "run")
                    .unwrap()
                    .id();
                let (program, _) = crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                    &sources, &inputs, &names, &types, &typed, owned, entry,
                )
                .expect("both actual arms lower");
                verify_program(&program).unwrap();
                let run = program
                    .modules
                    .iter()
                    .flat_map(|m| &m.functions)
                    .find(|f| f.name.contains(".run."))
                    .unwrap();
                let fresh: Vec<_> = run
                    .instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::BorrowCall { .. }))
                    .collect();
                assert_eq!(fresh.len(), 2);
                assert_ne!(fresh[0].block, fresh[1].block);
                assert_ne!(fresh[0].results, fresh[1].results);
                for acquire in fresh {
                    let EntityId::Loan(loan) = acquire.results[0] else {
                        panic!("fresh loan")
                    };
                    assert_eq!(
                        run.instructions
                            .iter()
                            .filter(|i| i.block == acquire.block
                                && i.operation == Operation::BorrowEnd { loan })
                            .count(),
                        1
                    );
                }
                let rendered = super::super::render_program(&program);
                if let Some(old) = &baseline {
                    assert_eq!(&rendered, old);
                } else {
                    baseline = Some(rendered);
                }
            }
        }
    }
}
