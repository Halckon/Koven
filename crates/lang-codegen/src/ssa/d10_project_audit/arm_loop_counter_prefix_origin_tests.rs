//! Original producer route probe only; not a complete certificate or admission change.
use super::*;
use crate::ssa::{refine::arm_loop_receipt, unit_lower, unit_lower_test_support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
#[test]
fn counter_source_prefix_preserves_original_producer_and_ownership_rejections() {
    for (before, between) in [("consume(source);", ""), ("", "consume(source);")] {
        for signature in [
            "source:String,rounds:Int,choose:Boolean",
            "source:String,own rounds:Int,own choose:Boolean",
        ] {
            for reverse in [false, true] {
                let mut sources = SourceMap::new();
                let (p, provider) = unit_lower_test_support::parsed(
                    &mut sources,
                    "counter-provider.ko",
                    "fun view(source:String):borrow String from source=source\nfun next(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun other(item:String):Unit {}",
                );
                let text = format!(
                    "fun run({signature}):Unit {{{before}var remaining=rounds;{between}while(remaining>0){{if(choose){{borrow val a=view(source);consume(a)}}else{{borrow val b=next(source);other(b);consume(b)}};remaining=remaining-1}}}}"
                );
                let (c, caller) =
                    unit_lower_test_support::parsed(&mut sources, "counter-caller.ko", &text);
                let mut inputs = [
                    SourceUnitInput::new("root", "counter-provider.ko", p, &provider),
                    SourceUnitInput::new("root", "counter-caller.ko", c, &caller),
                ];
                if reverse {
                    inputs.reverse();
                }
                let (environment, types) = standard_environments();
                if !between.is_empty() {
                    use lang_frontend::{
                        name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
                        ownership_checking::check_compilation_unit_ownership,
                        type_checking::check_compilation_unit_types,
                    };
                    let ((diagnostics, audits), receipts, _) = arm_loop_receipt::observe(|| {
                        observe(|| {
                            let index = index_compilation_unit(&sources, &inputs).unwrap();
                            let names = resolve_compilation_unit_names(
                                &sources,
                                &inputs,
                                &index,
                                &environment,
                            )
                            .unwrap()
                            .validate()
                            .unwrap();
                            let typed =
                                check_compilation_unit_types(&sources, &inputs, &names, &types)
                                    .unwrap()
                                    .validate()
                                    .unwrap();
                            let rejected = check_compilation_unit_ownership(
                                &sources, &inputs, &names, &types, &typed,
                            )
                            .unwrap()
                            .validate()
                            .expect_err("original counter adjacency ownership rejection");
                            assert!(rejected.no_carry_counter_branches().is_empty());
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
                    continue;
                }
                let (names, typed, owned) =
                    unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
                assert_eq!(owned.ownership().no_carry_counter_branches().len(), 1);
                let declaration = |name| {
                    names
                        .names()
                        .index()
                        .declarations()
                        .iter()
                        .find(|d| d.name() == name)
                        .unwrap()
                        .id()
                };
                for _ in 0..2 {
                    let ((result, audits), receipts, trace) = arm_loop_receipt::observe(|| {
                        observe(|| {
                            unit_lower::lower_scalar_unit_with_entry(
                                &sources,
                                &inputs,
                                &names,
                                &types,
                                &typed,
                                &owned,
                                declaration("run"),
                            )
                        })
                    });
                    let Err(error) = result else {
                        panic!("original counter producer rejects preceding consumption")
                    };
                    assert_eq!(
                        error.kind,
                        crate::ssa::lowering_support::LoweringErrorKind::InvalidModel
                    );
                    let span = error.span.expect("whole caller span");
                    assert_eq!(
                        (span.source_id(), span.start(), span.end()),
                        (c, 0, text.len())
                    );
                    assert!(receipts.is_empty());
                    assert!(audits.is_empty());
                    assert_eq!(trace.recorded, 0);
                }
            }
        }
    }
}
