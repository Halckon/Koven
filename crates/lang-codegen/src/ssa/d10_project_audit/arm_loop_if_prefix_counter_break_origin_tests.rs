//! Original If-prefix counter-break geometry probe, not a full certificate.
use super::*;
use crate::ssa::{model::*, refine::arm_loop_receipt, unit_lower_test_support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
#[test]
fn original_if_prefix_counter_break_geometry() {
    for reverse in [false, true] {
        let mut sources = SourceMap::new();
        let (p, provider) = unit_lower_test_support::parsed(
            &mut sources,
            "provider.ko",
            "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}",
        );
        let text = "fun tick(own n:Int):Unit {}\nfun run(before:Boolean,source:String,rounds:Int,choose:Boolean,stop:Boolean):Unit {if(before){tick(1)}else{tick(2)};var remaining=rounds;while(remaining>0){if(choose){borrow val a=view(source);consume(a)}else{borrow val b=view(source);consume(b)};if(stop){break};remaining=remaining-1}}";
        let (c, caller) = unit_lower_test_support::parsed(&mut sources, "caller.ko", text);
        let mut inputs = [
            SourceUnitInput::new("root", "provider.ko", p, &provider),
            SourceUnitInput::new("root", "caller.ko", c, &caller),
        ];
        if reverse {
            inputs.reverse();
        }
        let (environment, types) = standard_environments();
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
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
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap()
            .id();
        for _ in 0..2 {
            let ((result, audits), receipts, trace) = arm_loop_receipt::observe(|| {
                observe(|| {
                    crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                        &sources, &inputs, &names, &types, &typed, &owned, entry,
                    )
                })
            });
            let (program, _) = result.expect("original If-prefix lowering");
            crate::ssa::verify::verify_program(&program).unwrap();
            assert_eq!(receipts.len(), 1);
            assert_eq!(audits.len(), 1);
            let view = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "view")
                .unwrap()
                .id();
            assert!(!audits[0].borrow_edges.contains(&(entry, view)));
            assert!(!audits[0].unsupported.is_empty());
            assert_eq!(trace.before_verifier, 1);
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
                    .filter(|i| matches!(i.operation, Operation::BorrowEnd { .. }))
                    .count(),
                2
            );
            assert_eq!(
                run.blocks
                    .iter()
                    .filter(|b| matches!(
                        b.terminator.as_ref().unwrap().kind,
                        TerminatorKind::Abort
                    ))
                    .count(),
                1
            );
            assert_eq!(
                run.blocks
                    .iter()
                    .filter(|b| matches!(
                        b.terminator.as_ref().unwrap().kind,
                        TerminatorKind::Return { .. }
                    ))
                    .count(),
                1
            );
            eprintln!(
                "IF_PREFIX_GEOMETRY reverse={reverse} blocks={} instructions={} audits={}",
                run.blocks.len(),
                run.instructions.len(),
                audits.len()
            );
            eprintln!("{}", crate::ssa::render_program(&program));
        }
    }
}
