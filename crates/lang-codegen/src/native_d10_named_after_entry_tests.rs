//! Real source entry -> sealed common Program -> process object -> original native deadline.
//! No test ABI trampoline or synthetic SSA entry is added.
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{d10_catalog_experiment as experiment, owned_compilation_unit_view},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

#[test]
fn after_child_source_entry_native_preserves_zero_and_repeated_generations() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    for (parent, child, expected) in [
        (0, 0, "first"),
        (0, 2, "first"),
        (1, 0, "parent"),
        (2, 3, "parent"),
    ] {
        let text = format!(
            r#"fun run(count: Int, childCount: Int) {{
var a = "first"; var b = "kept"; var outer = count;
while(outer > 0) {{ var inner = childCount;
while(inner > 0) {{ val old = replace(&a, "next"); inner -= 1 }}; val parentOld = replace(&a, "parent"); outer -= 1 }};
println(a); println(b) }}
fun entry() {{ val p = {parent}; val c = {child}; run(p, c) }}"#
        );
        let mut sources = SourceMap::new();
        let id = sources.add_source("named-native.ko", &text).unwrap();
        let lexed = lex(&sources, id).unwrap();
        let parsed = parse_file(&sources, &lexed).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "named-native.ko", id, &parsed),
            SourceUnitInput::memory_document(id, &parsed),
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
            let bound = experiment::analyze_named_after_child_wrapper(
                &sources, &inputs, &names, &types, &typed,
            )
            .unwrap()
            .unwrap();
            let unit = owned_compilation_unit_view(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                bound.ownership(),
            )
            .unwrap();
            let publication =
                crate::d10_experiment::lower_named_after_child_wrapper(unit, &bound).unwrap();
            let directory = loop {
                let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!(
                    "koven-d10-after-named-native-{}-{sequence}",
                    std::process::id()
                ));
                match fs::create_dir(&path) {
                    Ok(()) => break path,
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => panic!("native evidence directory: {error}"),
                }
            };
            fs::write(directory.join("source.ko"), &text).unwrap();
            fs::write(
                directory.join("expected.stdout"),
                format!("{expected}\nkept\n"),
            )
            .unwrap();
            let object = directory.join("entry.o");
            let executable = directory.join("entry");
            publication.emit_process_object(&object).unwrap();
            let mut command = Command::new(crate::test_support::clang());
            command.arg(&object).arg("-o").arg(&executable);
            fs::write(directory.join("link.command"), format!("{command:?}\n")).unwrap();
            let linked = command.output().unwrap();
            fs::write(directory.join("link.stdout"), &linked.stdout).unwrap();
            fs::write(directory.join("link.stderr"), &linked.stderr).unwrap();
            assert!(
                linked.status.success(),
                "{linked:?}; artifacts={}",
                directory.display()
            );
            super::execute_binary(
                &executable,
                format!("{expected}\nkept\n").as_bytes(),
                &directory,
            );
            fs::remove_dir_all(directory).unwrap();
        }
    }
}
