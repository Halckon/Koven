//! Ordinary bounded counter + stable conditional break paths; no injected faults.
use super::*;
#[cfg(feature = "loan-observation-test-support")]
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};
#[test]
fn ordinary_counter_arm_conditional_break_closes_both_paths() {
    check_counter_break_paths(false);
}

#[test]
fn ordinary_counter_arm_conditional_break_shares_stable_flag() {
    check_counter_break_paths(true);
}

fn check_counter_break_paths(shared: bool) {
    for mask in 0..if shared { 4 } else { 8 } {
        let mode = |bit| if mask & bit != 0 { "own " } else { "" };
        let (rounds, choose, stop) = (mode(1), mode(2), mode(4));
        let stop_parameter = if shared {
            String::new()
        } else {
            format!("{stop}stop:Boolean,")
        };
        let stop_condition = if shared { "choose" } else { "stop" };
        let text = format!(
            "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun run({rounds}rounds:Int,{stop_parameter}source:String,{choose}choose:Boolean):Unit {{var remaining=rounds;while(remaining>0){{if(choose){{borrow val a=view(source);consume(a)}}else{{borrow val b=view(source);consume(b);consume(b)}};if({stop_condition}){{break}};remaining=remaining-1}};consume(source)}}"
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
                assert_eq!(owned.ownership().no_carry_counter_break_branches().len(), 1);
                assert!(owned.ownership().no_carry_counter_branches().is_empty());
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

#[test]
fn ordinary_counter_arm_conditional_break_rejects_extra_control() {
    for tail in [
        "if(stop){break}else{}",
        "if(stop){continue}",
        "if(stop){return}",
        "if(stop){if(choose){break}}",
        "if(stop){consume(source);break}",
        "if(stop){break};if(choose){break}",
        "if(stop&&choose){break}",
    ] {
        let text = format!(
            "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun run(own rounds:Int,own stop:Boolean,source:String,own choose:Boolean):Unit {{var remaining=rounds;while(remaining>0){{if(choose){{borrow val a=view(source);consume(a)}}else{{borrow val b=view(source);consume(b);consume(b)}};{tail};remaining=remaining-1}};consume(source)}}"
        );
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "counter-break-rejected.ko", &text);
        let inputs = [SourceUnitInput::memory_document(source, &parsed)];
        let (environment, types) = standard_environments();
        // Rejected source may fail the frontend ownership gate before codegen.
        // Do not use the positive-only helper, which requires valid ownership.
        let index = lang_frontend::name_resolution::index_compilation_unit(&sources, &inputs)
            .expect("valid unit input");
        let names = lang_frontend::name_resolution::resolve_compilation_unit_names(
            &sources,
            &inputs,
            &index,
            &environment,
        )
        .unwrap()
        .validate()
        .expect("valid names");
        let typed = lang_frontend::type_checking::check_compilation_unit_types(
            &sources, &inputs, &names, &types,
        )
        .unwrap()
        .validate()
        .expect("valid types");
        let owned = lang_frontend::ownership_checking::check_compilation_unit_ownership(
            &sources, &inputs, &names, &types, &typed,
        )
        .expect("ownership analysis completes");
        assert!(owned.no_carry_counter_break_branches().is_empty(), "{tail}");
        assert!(owned.no_carry_counter_branches().is_empty(), "{tail}");
        assert!(owned.no_carry_break_branches().is_empty(), "{tail}");
        let owned = match owned.validate() {
            Ok(owned) => owned,
            Err(rejected) => {
                assert!(
                    !rejected.diagnostics().is_empty() || !rejected.deferred().is_empty(),
                    "frontend must explain rejection: {tail}"
                );
                continue;
            }
        };
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap()
            .id();
        let (result, receipts, trace) = crate::ssa::refine::arm_loop_receipt::observe(|| {
            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                &sources, &inputs, &names, &types, &typed, &owned, entry,
            )
        });
        assert!(result.is_err(), "unapproved {tail}");
        assert!(receipts.is_empty());
        assert_eq!(trace.pending_after, 0);
    }
}
