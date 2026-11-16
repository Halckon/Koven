//! Original ownership refusal for a source call before counter-break initialization.
use super::*;
use crate::ssa::{refine::arm_loop_receipt, unit_lower_test_support};
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::check_compilation_unit_ownership,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
#[test]
fn counter_break_source_prefix_preserves_original_ownership_rejection() {
    for signature in [
        "source:String,rounds:Int,choose:Boolean,stop:Boolean",
        "source:String,own rounds:Int,own choose:Boolean,own stop:Boolean",
        "source:String,rounds:Int,choose:Boolean",
        "source:String,own rounds:Int,own choose:Boolean",
    ] {
        let stop = if signature.contains("stop:Boolean") {
            "stop"
        } else {
            "choose"
        };
        for reverse in [false, true] {
            let mut sources = SourceMap::new();
            let (p, provider) = unit_lower_test_support::parsed(
                &mut sources,
                "provider.ko",
                "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}",
            );
            let text = format!(
                "fun run({signature}):Unit {{consume(source);var remaining=rounds;while(remaining>0){{if(choose){{borrow val a=view(source);consume(a)}}else{{borrow val b=view(source);consume(b)}};if({stop}){{break}};remaining=remaining-1}}}}"
            );
            let (c, caller) = unit_lower_test_support::parsed(&mut sources, "caller.ko", &text);
            let mut inputs = [
                SourceUnitInput::new("root", "provider.ko", p, &provider),
                SourceUnitInput::new("root", "caller.ko", c, &caller),
            ];
            if reverse {
                inputs.reverse();
            }
            let (environment, types) = standard_environments();
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            let ((diagnostics, audits), receipts, trace) = arm_loop_receipt::observe(|| {
                observe(|| {
                    let rejected =
                        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                            .unwrap()
                            .validate()
                            .expect_err("original source-prefix ownership rejection");
                    assert!(rejected.no_carry_counter_break_branches().is_empty());
                    assert!(rejected.no_carry_prefix_counter_breaks().is_empty());
                    rejected
                        .diagnostics()
                        .iter()
                        .map(|d| (format!("{:?}", d.code()), d.primary_span()))
                        .collect::<Vec<_>>()
                })
            });
            assert_eq!(diagnostics.len(), 2);
            for ((code, span), (start, _)) in diagnostics
                .into_iter()
                .zip(text.match_indices("borrow val"))
            {
                assert_eq!(code, "L0164");
                assert_eq!(
                    (span.source_id(), span.start(), span.end()),
                    (c, start, start + 6)
                );
            }
            assert!(audits.is_empty());
            assert!(receipts.is_empty());
            assert_eq!(trace.recorded, 0);
        }
    }
}
