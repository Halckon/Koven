//! Exact two-operation nested observation handed to normal lowering and independently replayed.
use super::model::*;
use super::*;
use lang_frontend::ownership_checking::OwnedCompilationUnitView;
mod replay;
mod source;
const SOURCE: &str = "fun run(count: Int) { var a = \"first\"; var b = \"kept\"; var outer = count; while(outer > 0) { val parentOld = replace(&a, \"parent\"); var inner = count; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; outer -= 1 }; println(b) }";
#[test]
fn actual_dual_nested_replacements_preserve_zero_and_backedge_generations() {
    use lang_frontend::{
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::{d10_catalog_experiment as experiment, owned_compilation_unit_view},
        source::SourceMap,
        type_checking::{check_compilation_unit_types, standard_environments},
    };
    for (text, accepted) in [
        (SOURCE.to_owned(), true),
        (
            SOURCE
                .replace("first", "初始")
                .replace("kept", "保留")
                .replace("parent\"", "父\"")
                .replace("next", "子"),
            true,
        ),
        (SOURCE.replace("parent\"", "next\""), true),
        (
            SOURCE.replace("replace(&a, \"parent\")", "replace(&b, \"parent\")"),
            false,
        ),
        (
            SOURCE.replace("replace(&a, \"next\")", "replace(&b, \"next\")"),
            false,
        ),
        (
            SOURCE.replace("var inner = count", "var inner = outer"),
            false,
        ),
        (SOURCE.replace("inner -= 1", "inner -= 2"), false),
        (SOURCE.replace("println(b)", "println(a)"), false),
    ] {
        let mut sources = SourceMap::new();
        let (source, ast) = super::unit_lower_test_support::parsed(&mut sources, "dual.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "dual.ko", source, &ast),
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
            let bound =
                experiment::analyze_dual_nested_loop(&sources, &inputs, &names, &types, &typed)
                    .unwrap();
            assert_eq!(bound.is_some(), accepted, "{text}");
            assert!(
                experiment::analyze_nested_loop(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .is_none()
            );
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
            assert_ne!(bound.replacements()[0], bound.replacements()[1]);
            assert_ne!(bound.producer_sites()[1], bound.producer_sites()[2]);
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
            replay::replay(&unit, &program, function)
                .expect("actual dual Replace CFG and cleanup replay");
        }
    }
}
