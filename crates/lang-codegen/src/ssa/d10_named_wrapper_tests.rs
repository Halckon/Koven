//! Legal source acceptance/rejection and object inspection only. No native execution or IR faults.
use super::*;
use crate::d10_experiment::lower_named_wrapper as lower_wrapper;
use experiment::analyze_named_wrapper as analyze_wrapper;
use lang_frontend::{
    name_resolution::SourceUnitInput,
    ownership_checking::{d10_catalog_experiment as experiment, owned_compilation_unit_view},
    source::SourceMap,
    type_checking::standard_environments,
};
const SOURCE: &str = r#"fun run(count: Int, childCount: Int) {
var a = "first"; var b = "kept"; var outer = count;
while(outer > 0) { val parentOld = replace(&a, "parent"); var inner = childCount;
while(inner > 0) { val old = replace(&a, "next"); inner -= 1 }; outer -= 1 };
println(a); println(b) }
fun entry() { val p = 1; val c = 2; run(p, c) }"#;
#[test]
fn named_wrapper_publishes_one_complete_program_and_library_object() {
    for (text, accepted) in [
        (SOURCE.to_owned(), true),
        (
            SOURCE
                .replace("val p = 1", "val p = 0")
                .replace("val c = 2", "val c = 0"),
            true,
        ),
        (
            SOURCE
                .replace("run", "execute")
                .replace("entry", "launch")
                .replace("count", "limit")
                .replace("childCount", "innerLimit")
                .replace(";", ";\n"),
            true,
        ),
        (
            SOURCE
                .replace("val parentOld = replace(&a, \"parent\"); ", "")
                .replace(
                    "}; outer -= 1",
                    "}; val parentOld = replace(&a, \"parent\"); outer -= 1",
                ),
            false,
        ),
        (
            {
                let (callee, wrapper) = SOURCE.split_once("\nfun entry").unwrap();
                format!("fun entry{wrapper}\n{callee}")
            },
            false,
        ),
        (SOURCE.replace("run(p, c)", "run(1, 2)"), false),
        (SOURCE.replace("run(p, c)", "run(c, p)"), false),
        (SOURCE.replace("run(p, c)", "run(p, c); run(p, c)"), false),
        (
            SOURCE.replace("println(a); println(b)", "println(b)"),
            false,
        ),
    ] {
        let mut sources = SourceMap::new();
        let (id, parsed) = unit_lower_test_support::parsed(&mut sources, "named-object.ko", &text);
        let (env, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "named-object.ko", id, &parsed),
            SourceUnitInput::memory_document(id, &parsed),
        ] {
            let inputs = [input];
            let (names, typed, _) =
                unit_lower_test_support::analyze(&sources, &inputs, &env, &types);
            let bound = analyze_wrapper(&sources, &inputs, &names, &types, &typed).unwrap();
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
            let artifact = lower_wrapper(unit, &bound).unwrap();
            assert_eq!(artifact.callable(), bound.entry());
            assert_eq!(artifact.source_count(), 1);
            artifact.verified_llvm_ir().unwrap();
            let (program, entry) = artifact.replay_input();
            verify::verify_program(program).unwrap();
            assert_eq!(program.modules.len(), 1);
            let functions = &program.modules[0].functions;
            assert_eq!(functions.len(), 2);
            assert!(functions.iter().any(|f| f.id() == entry));
            if text == SOURCE {
                let bytes = artifact.verified_library_object_bytes().unwrap();
                for f in functions {
                    let symbol = format!("f{}.{}", f.id().index(), f.name);
                    crate::d10_experiment::object_tests::check(&bytes, &symbol);
                }
            }
        }
    }
}

super::d10_named_process_object_tests::suite!(SOURCE, analyze_wrapper, lower_wrapper);
