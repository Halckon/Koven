//! Final-owned loop observation must not silently bypass ordinary admission.
use super::*;
#[test]
fn actual_nested_replace_preserves_both_roots_and_parameter_loan() {
    use super::unit_lower_test_support::parsed;
    use lang_frontend::{
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::{d10_catalog_experiment, owned_compilation_unit_view},
        source::SourceMap,
        type_checking::{check_compilation_unit_types, standard_environments},
    };
    let base = "fun run(count: Int) { var a = \"first\"; var b = \"kept\"; var outer = count; while(outer > 0) { var inner = count; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; outer -= 1 }; println(b) }";
    let parent = base.replace(
        "var inner = count; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }",
        "val old = replace(&a, \"next\"); var inner = count; while(inner > 0) { inner -= 1 }",
    );
    for (text, accepted) in [
        (parent.clone(), true),
        (
            parent
                .replace("first", "甲")
                .replace("kept", "保留")
                .replace("next", "乙"),
            true,
        ),
        (
            parent.replace("var inner = count", "var inner = outer"),
            false,
        ),
        (parent.replace("inner -= 1", "inner -= 2"), false),
        (parent.replace("println(b)", "println(a)"), false),
        (base.to_owned(), true),
        (
            base.replace("first", "甲")
                .replace("kept", "保留")
                .replace("next", "乙"),
            true,
        ),
        (
            base.replace("var inner = count", "var inner = outer"),
            false,
        ),
        (base.replace("inner -= 1", "inner -= 2"), false),
        (base.replace("println(b)", "println(a)"), false),
    ] {
        let mut sources = SourceMap::new();
        let (source, ast) = parsed(&mut sources, "loop-bound.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "loop-bound.ko", source, &ast),
            SourceUnitInput::memory_document(source, &ast),
        ] {
            let inputs = [input];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            let bound = d10_catalog_experiment::analyze_nested_loop(
                &sources, &inputs, &names, &types, &typed,
            )
            .unwrap();
            assert_eq!(bound.is_some(), accepted, "{text}");
            let Some(bound) = bound else {
                continue;
            };
            let unit = owned_compilation_unit_view(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                bound.ownership(),
            )
            .unwrap();
            assert!(bound.matches(&unit));
            assert!(
                source::facts(&unit).is_some(),
                "fixture evidence must agree with independently closed source"
            );
            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap()
                .id();
            let (program, function) =
                unit_lower::lower_owned_unit_with_entry(&unit, entry).unwrap();
            assert!(
                lang_frontend::ownership_checking::d10_catalog_experiment::analyze_loop(
                    &sources, &inputs, &names, &types, &typed
                )
                .unwrap()
                .is_none(),
                "single-loop sidecar must not accept nesting"
            );
            if bound.replacement_control() == bound.controls().1 {
                parent_replay::replay(&unit, &program, function)
                    .expect("actual parent Replace / child counter CFG replay");
            } else {
                replay::replay(&unit, &program, function).expect("actual child Replace CFG replay");
            }
        }
    }
}

use super::model::*;
use lang_frontend::ownership_checking::OwnedCompilationUnitView;
mod replay;
mod source;

mod parent_replay;
