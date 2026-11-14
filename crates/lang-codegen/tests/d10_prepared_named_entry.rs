//! External integration: lang-codegen and frontend are built without cfg(test).
//! Run with only d10-catalog-experiment to prove no loan-observation-test-support is required.
#![cfg(feature = "d10-catalog-experiment")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::d10_catalog_experiment as experiment,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
use std::{fs, process::Command};
#[test]
fn explicit_final_admitted_entry_emits_real_object_without_test_observation_hooks() {
    for (sequence, body) in [
        "val parentOld = replace(&a, \"parent\"); var inner = childCount; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; outer -= 1",
        "var inner = childCount; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; val parentOld = replace(&a, \"parent\"); outer -= 1",
    ].into_iter().enumerate() {
        let text = format!("fun run(count: Int, childCount: Int) {{ var a = \"first\"; var b = \"kept\"; var outer = count; while(outer > 0) {{ {body} }}; println(a); println(b) }}\nfun entry() {{ val p = 1; val c = 2; run(p, c) }}");
        let mut sources = SourceMap::new();
        let id = sources.add_source("prepared.ko", &text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (env, types) = standard_environments();
        for (layout, input) in [SourceUnitInput::new("root", "prepared.ko", id, &parsed), SourceUnitInput::memory_document(id, &parsed)].into_iter().enumerate() {
            let inputs = [input];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env).unwrap().validate().unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap().validate().unwrap();
            let prepared = experiment::prepare_named_entry(&sources, &inputs, &names, &types, typed).unwrap();
            let publication = lang_codegen::d10_experiment::lower_prepared_named_entry(&sources, &inputs, &names, &types, &prepared).unwrap();
            assert_eq!(publication.callable(), prepared.bound().entry());
            assert_eq!(publication.order(), prepared.bound().order());
            assert!(!publication.verified_llvm_ir().unwrap().is_empty());
            let directory = std::env::temp_dir().join(format!("koven-d10-public-{}-{sequence}-{layout}", std::process::id()));
            fs::create_dir(&directory).unwrap();
            let object = directory.join("entry.o");
            publication.emit_process_object(&object).unwrap();
            let bytes = fs::read(&object).unwrap();
            assert!(!bytes.is_empty());
            let output = Command::new("/usr/bin/nm").args(if cfg!(target_os="macos") {["-j", "-U"]}else{["-j", "--defined-only"]}).arg(&object).output().unwrap();
            assert!(output.status.success());
            let symbols = String::from_utf8(output.stdout).unwrap();
            assert_eq!(symbols.lines().filter(|s| *s == if cfg!(target_os="macos") {"_main"} else {"main"}).count(), 1);
            assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
            // Public create-only publication bypasses CLI preflight, proving the actual commit
            // refuses existing directory entries without a rename fallback.
            let fresh = directory.join("fresh.o");
            publication.emit_new_process_object(&fresh).unwrap();
            let fresh_bytes = fs::read(&fresh).unwrap();
            let conflict = publication.emit_new_process_object(&fresh).expect_err("existing object is never replaced");
            assert_eq!(conflict.kind(), lang_codegen::NativeObjectErrorKind::OutputAlreadyExists);
            assert_eq!(fs::read(&fresh).unwrap(), fresh_bytes);
            assert_eq!(fs::read(&object).unwrap(), bytes);
            let folder = directory.join("folder");
            fs::create_dir(&folder).unwrap();
            publication.emit_new_process_object(&folder).expect_err("directory is never replaced");
            assert!(folder.is_dir());
            #[cfg(unix)] {
                use std::os::unix::fs::symlink;
                let link = directory.join("link.o");
                symlink(&fresh, &link).unwrap();
                let absent = directory.join("absent.o");
                let dangling = directory.join("dangling.o");
                symlink(&absent, &dangling).unwrap();
                for path in [&link, &dangling] {
                    publication.emit_new_process_object(path).expect_err("symlink directory entry is never replaced");
                    assert!(fs::symlink_metadata(path).unwrap().file_type().is_symlink());
                }
                assert!(!absent.exists());
                assert_eq!(fs::read(&fresh).unwrap(), fresh_bytes);
            }
            assert!(fs::read_dir(&directory).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with(".koven-unit-object-")));
            fs::remove_dir_all(directory).unwrap();
        }
    }
}
