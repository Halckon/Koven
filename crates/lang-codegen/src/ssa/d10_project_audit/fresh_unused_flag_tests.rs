//! Exact unused ABI Boolean non-transport at the original first CFG edge.
use super::fresh_single_value_tests::check_parameters;
#[test]
fn unused_boolean_role_is_not_transported_past_entry() {
    for names in [
        ["source", "flag", "unused"],
        ["source", "unused", "flag"],
        ["flag", "source", "unused"],
        ["flag", "unused", "source"],
        ["unused", "source", "flag"],
        ["unused", "flag", "source"],
    ] {
        for owned in [[false, false], [false, true], [true, false], [true, true]] {
            let parameters = names
                .map(|n| match n {
                    "source" => "source:String".to_string(),
                    "flag" => format!("{}flag:Boolean", if owned[0] { "own " } else { "" }),
                    _ => format!("{}unused:Boolean", if owned[1] { "own " } else { "" }),
                })
                .join(",");
            for reverse in [false, true] {
                for segments in [(0, 0), (1, 1)] {
                    for uses in ["consume(item)", "other(item);consume(item)"] {
                        check_parameters(reverse, &parameters, segments, uses, true);
                    }
                }
            }
        }
    }
}
#[test]
fn fourth_boolean_parameter_and_long_source_segments_keep_original_gap() {
    check_parameters(
        false,
        "source:String,own flag:Boolean,unused:Boolean,extra:Boolean",
        (0, 0),
        "consume(item)",
        false,
    );
    check_parameters(
        false,
        "source:String,flag:Boolean,own unused:Boolean,own extra:Boolean",
        (1, 1),
        "consume(item)",
        false,
    );
    check_parameters(
        false,
        "source:String,flag:Boolean,own unused:Boolean",
        (2, 0),
        "consume(item)",
        false,
    );
}

// Both real original routes lower successfully without a fresh receipt; retain precise audit gaps.
#[test]
fn used_extra_and_non_boolean_parameter_keep_original_successful_gap() {
    use super::*;
    use crate::ssa::{refine::fresh_loop_receipt, unit_lower, unit_lower_test_support};
    use lang_frontend::{
        name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
    };
    for (parameter, prefix) in [("unused:Boolean", "inspect(unused);"), ("unused:Int", "")] {
        let mut sources = SourceMap::new();
        let (p, provider) = unit_lower_test_support::parsed(
            &mut sources,
            "unused-boundary-provider.ko",
            "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun inspect(value:Boolean):Unit {}",
        );
        let text = format!(
            "fun run(source:String,flag:Boolean,{parameter}):Unit {{{prefix}while(flag){{borrow val item=view(source);consume(item)}}}}"
        );
        let (c, caller) =
            unit_lower_test_support::parsed(&mut sources, "unused-boundary-caller.ko", &text);
        let inputs = [
            SourceUnitInput::new("root", "unused-boundary-provider.ko", p, &provider),
            SourceUnitInput::new("root", "unused-boundary-caller.ko", c, &caller),
        ];
        let (environment, types) = standard_environments();
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap()
            .id();
        let ((result, audits), receipts) = fresh_loop_receipt::observe(|| {
            observe(|| {
                unit_lower::lower_scalar_unit_with_entry(
                    &sources, &inputs, &names, &types, &typed, &owned, entry,
                )
            })
        });
        let (program, function_id) = result.expect("original route still lowers");
        crate::ssa::verify::verify_program(&program).unwrap();
        assert_eq!(audits.len(), 1);
        assert!(receipts.is_empty());
        assert!(
            audits[0]
                .unsupported
                .iter()
                .any(|g| g
                    == "fresh no-carry missing/duplicate/outside-domain or final evidence gap")
        );
        let view = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "view")
            .unwrap()
            .id();
        assert!(!audits[0].borrow_edges.contains(&(entry, view)));
        let function = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.id == function_id)
            .unwrap();
        if !prefix.is_empty() {
            let count = function
                .instructions
                .iter()
                .filter(|i| match &i.operation {
                    Operation::DirectCall { callee, .. } => program
                        .modules
                        .iter()
                        .flat_map(|m| &m.functions)
                        .any(|f| f.id == *callee && f.name.contains(".inspect.")),
                    _ => false,
                })
                .count();
            assert_eq!(
                count, 1,
                "extra use is retained by original lowering, never elided to qualify"
            );
            assert!(
                audits[0]
                    .unsupported
                    .iter()
                    .any(|g| g.starts_with("unclassified operation: SharedReborrow"))
            );
        }
    }
}
