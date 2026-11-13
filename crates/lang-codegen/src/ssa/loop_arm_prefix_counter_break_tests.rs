//! Ordinary approved prefix + counter + conditional break, no injected faults.
use super::*;
#[cfg(feature = "loan-observation-test-support")]
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};
#[test]
fn ordinary_prefix_counter_break_transports_all_live_roles() {
    for scenario in 0..3 {
        for mask in 0..[16, 8, 4][scenario] {
            let mode = |bit| if mask & bit != 0 { "own " } else { "" };
            let (rounds, choose, stop, before) = (mode(1), mode(2), mode(4), mode(8));
            let prefix_parameter = if scenario == 0 {
                format!("{before}before:Boolean,")
            } else {
                String::new()
            };
            let stop_parameter = if scenario < 2 {
                format!("{stop}stop:Boolean,")
            } else {
                String::new()
            };
            let prefix_condition = ["before", "stop", "choose"][scenario];
            let stop_condition = if scenario == 2 { "choose" } else { "stop" };
            let text = format!(
                "fun tick(own n:Int):Unit {{}}\nfun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun run({prefix_parameter}{rounds}rounds:Int,{stop_parameter}source:String,{choose}choose:Boolean):Unit {{if({prefix_condition}){{tick(1)}}else{{tick(2)}};var remaining=rounds;while(remaining>0){{if(choose){{borrow val a=view(source);consume(a)}}else{{borrow val b=view(source);consume(b);consume(b)}};if({stop_condition}){{break}};remaining=remaining-1}};consume(source)}}"
            );
            let mut sources = SourceMap::new();
            let (source, parsed) =
                unit_lower_test_support::parsed(&mut sources, "counter-break-arms.ko", &text);
            let (environment, types) = standard_environments();
            for input in [
                SourceUnitInput::new("root", "counter-break-arms.ko", source, &parsed),
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
                    let facts = owned.ownership().no_carry_prefix_counter_breaks();
                    assert_eq!(facts.len(), 1);
                    assert!(
                        owned
                            .ownership()
                            .owns_no_carry_prefix_counter_break(&facts[0])
                    );
                    assert!(
                        !owned
                            .ownership()
                            .owns_no_carry_counter_break_branch(facts[0].counter_break())
                    );
                    assert!(
                        owned
                            .ownership()
                            .no_carry_counter_break_branches()
                            .is_empty()
                    );
                    assert!(owned.ownership().no_carry_counter_branches().is_empty());
                    assert!(owned.ownership().no_carry_break_prefixes().is_empty());
                    assert!(owned.ownership().no_carry_break_branches().is_empty());
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
                    let (program, _) = result.expect("bounded conditional break lowering");
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
                    let rendered = super::super::render_program(&program);
                    if let Some(old) = &baseline {
                        assert_eq!(old, &rendered);
                    } else {
                        baseline = Some(rendered);
                    }
                }
            }
        }
    }
}
