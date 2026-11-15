//! Real typed Boolean modes across both tail-break and false-path exit.
use super::arm_loop_tail_break_tests::{check, check_roles};
#[test]
fn tail_break_value_conditions_preserve_mode_and_exit_source() {
    let names = ["source", "flag", "choose"];
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        for modes in [[false, true], [true, false], [true, true]] {
            let parameters = order
                .map(|i| {
                    format!(
                        "{}{}:{}",
                        if i > 0 && modes[i - 1] { "own " } else { "" },
                        names[i],
                        if i == 0 { "String" } else { "Boolean" }
                    )
                })
                .join(",");
            for suffix in ["", ";after(source)"] {
                check(&parameters, "", suffix, true);
            }
        }
    }
}
#[test]
fn tail_break_value_conditions_keep_unused_and_same_role_gaps() {
    check(
        "source:String,own flag:Boolean,own choose:Boolean,extra:Boolean",
        "",
        ";after(source)",
        false,
    );
    check_roles(
        "source:String,own flag:Boolean,own choose:Boolean",
        "",
        ";after(source)",
        false,
        true,
    );
}

use super::*;
use crate::ssa::{refine::arm_loop_receipt, unit_lower_test_support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
#[test]
fn tail_break_value_conditions_preserve_original_source_and_group_rejections() {
    use lang_frontend::{
        name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
        ownership_checking::check_compilation_unit_ownership,
        type_checking::check_compilation_unit_types,
    };
    for (parameters, condition) in [
        (
            "own source:String,own flag:Boolean,own choose:Boolean",
            "flag",
        ),
        (
            "source:String,own flag:Boolean,own choose:Boolean",
            "(flag)",
        ),
    ] {
        let mut sources = SourceMap::new();
        let (p, provider) = unit_lower_test_support::parsed(
            &mut sources,
            "arm-mode-boundary-provider.ko",
            "fun view(source:String):borrow String from source=source\nfun next(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun other(item:String):Unit {}",
        );
        let text = format!(
            "fun run({parameters}):Unit {{while({condition}){{if(choose){{borrow val a=view(source);consume(a)}}else{{borrow val b=next(source);other(b);consume(b)}};break}}}}"
        );
        let (c, caller) =
            unit_lower_test_support::parsed(&mut sources, "arm-mode-boundary-caller.ko", &text);
        let inputs = [
            SourceUnitInput::new("root", "arm-mode-boundary-provider.ko", p, &provider),
            SourceUnitInput::new("root", "arm-mode-boundary-caller.ko", c, &caller),
        ];
        let (environment, types) = standard_environments();
        let ((diagnostics, audits), receipts, _) = arm_loop_receipt::observe(|| {
            observe(|| {
                let index = index_compilation_unit(&sources, &inputs).unwrap();
                let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                    .unwrap()
                    .validate()
                    .unwrap();
                let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                    .unwrap()
                    .validate()
                    .unwrap();
                let rejected =
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                        .validate()
                        .expect_err("original unsupported ownership domain");
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
    }
}
