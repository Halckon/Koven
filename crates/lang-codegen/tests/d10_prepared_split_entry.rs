//! Public split-source pipeline: both libraries are built without cfg(test).
//! Only d10-catalog-experiment is required; no linking or generated-code execution.
#![cfg(feature = "d10-catalog-experiment")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{d10_catalog_experiment as experiment, owned_compilation_unit_view},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
use std::{fs, process::Command};

const BEFORE: &str = r#"fun run(count: Int, childCount: Int) { var a = "first"; var b = "kept"; var outer = count; while(outer > 0) { val parentOld = replace(&a, "parent"); var inner = childCount; while(inner > 0) { val old = replace(&a, "next"); inner -= 1 }; outer -= 1 }; println(a); println(b) }"#;
const ENTRY: &str = "fun entry() { val p = 1; val c = 2; run(p,c) }";

fn check(texts: &[(&str, &str)], expected: Option<experiment::NamedEntryOrder>, case: usize) {
    let mut sources = SourceMap::new();
    let parsed: Vec<_> = texts
        .iter()
        .map(|(path, text)| {
            let id = sources.add_source(*path, *text).unwrap();
            let tokens = lex(&sources, id).unwrap();
            let parsed = parse_file(&sources, &tokens).unwrap();
            assert!(parsed.diagnostics().is_empty());
            parsed
        })
        .collect();
    let inputs: Vec<_> = texts
        .iter()
        .zip(&parsed)
        .map(|((path, _), p)| SourceUnitInput::new("root", path, p.source_id(), p))
        .collect();
    let (env, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let selected = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "entry" && d.kind() == SymbolKind::Function)
        .unwrap();
    let result = experiment::prepare_selected_split_named_entry(
        &sources, &inputs, &names, &types, typed, selected,
    );
    let Some(order) = expected else {
        assert!(matches!(
            result,
            Err(experiment::PrepareNamedEntryError::Unsupported)
        ));
        return;
    };
    let prepared = result.unwrap();
    assert_eq!(prepared.bound().order(), order);
    assert_eq!(
        prepared.bound().entry().source_unit(),
        selected.source_unit()
    );
    assert_eq!(prepared.bound().entry().item(), selected.root());
    assert_ne!(
        prepared.bound().entry().source_unit(),
        prepared.bound().callee().source_unit()
    );
    let unit = owned_compilation_unit_view(
        &sources,
        &inputs,
        &names,
        &types,
        prepared.typed(),
        prepared.bound().ownership(),
    )
    .unwrap();
    assert!(prepared.bound().matches(&unit));
    let publication = lang_codegen::d10_experiment::lower_prepared_split_named_entry(
        &sources, &inputs, &names, &types, &prepared,
    )
    .unwrap();
    assert_eq!(publication.callable(), prepared.bound().entry());
    assert_eq!(publication.order(), order);
    assert_eq!(publication.source_count(), 2);
    let llvm = publication.verified_llvm_ir().unwrap();
    // Recover actual emitted symbols from verified LLVM rather than assume FunctionId order.
    let definitions: Vec<_> = llvm
        .lines()
        .filter(|l| l.starts_with("define "))
        .map(|l| {
            l.split_once('@')
                .unwrap()
                .1
                .split_once('(')
                .unwrap()
                .0
                .trim_matches('"')
                .to_owned()
        })
        .collect();
    // LLVM also defines runtime helpers. Keep the full verified module and object;
    // only the two original source functions form this symbol expectation.
    assert!(!definitions.iter().any(|s| s == "main" || s == "_main"));
    let source_definitions: Vec<_> = definitions
        .iter()
        .filter(|s| s.contains(".run.d") || s.contains(".entry.d"))
        .cloned()
        .collect();
    assert_eq!(source_definitions.len(), 2, "{definitions:?}");
    for name in ["run", "entry"] {
        assert_eq!(
            definitions
                .iter()
                .filter(|s| s.contains(&format!(".{name}.d")))
                .count(),
            1,
            "{definitions:?}"
        );
    }
    let bytes = publication.verified_library_object_bytes().unwrap();
    if cfg!(target_os = "macos") {
        assert_eq!(bytes.get(..4), Some([0xcf, 0xfa, 0xed, 0xfe].as_slice()));
        assert_eq!(bytes.get(12..16), Some([1, 0, 0, 0].as_slice()));
    } else {
        assert_eq!(bytes.get(..4), Some(b"\x7fELF".as_slice()));
        assert_eq!(bytes.get(16..18), Some([1, 0].as_slice()));
    }
    let directory = std::env::temp_dir().join(format!(
        "koven-d10-split-public-{}-{case}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    let object = directory.join("library.o");
    fs::write(&object, &bytes).unwrap();
    let output = Command::new("/usr/bin/nm")
        .args(if cfg!(target_os = "macos") {
            ["-j", "-U"]
        } else {
            ["-j", "--defined-only"]
        })
        .arg(&object)
        .output()
        .unwrap();
    assert!(output.status.success(), "nm: {:?}", output.stderr);
    let symbols = String::from_utf8(output.stdout).unwrap();
    for definition in source_definitions {
        let expected = if cfg!(target_os = "macos") {
            format!("_{definition}")
        } else {
            definition
        };
        assert_eq!(
            symbols.lines().filter(|s| *s == expected).count(),
            1,
            "{symbols}"
        );
    }
    assert!(!symbols.lines().any(|s| s == "main" || s == "_main"));
    assert_eq!(fs::read(&object).unwrap(), bytes);
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
    check_process_objects(&publication, &directory);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn split_final_admitted_entry_emits_verified_library_for_both_source_orders() {
    let after = BEFORE
        .replace("val parentOld = replace(&a, \"parent\"); ", "")
        .replace(
            "}; outer -= 1",
            "}; val parentOld = replace(&a, \"parent\"); outer -= 1",
        );
    let mut case = 0;
    for (callee, order) in [
        (BEFORE, experiment::NamedEntryOrder::BeforeChild),
        (after.as_str(), experiment::NamedEntryOrder::AfterChild),
    ] {
        for callee_first in [false, true] {
            let (cp, wp) = if callee_first {
                ("a.ko", "z.ko")
            } else {
                ("z.ko", "a.ko")
            };
            for reverse in [false, true] {
                let mut texts = [(cp, callee), (wp, ENTRY)];
                if reverse {
                    texts.reverse();
                }
                check(&texts, Some(order), case);
                case += 1;
            }
        }
    }
    assert_eq!(case, 8);
}

#[test]
fn legal_split_scope_neighbors_do_not_reach_publication() {
    check(
        &[
            ("a.ko", BEFORE),
            ("z.ko", "fun entry() { val p = 1; val c = 2; run(1,2) }"),
        ],
        None,
        0,
    );
    let extra = format!("{ENTRY}\nfun extra() {{}}");
    check(&[("a.ko", BEFORE), ("z.ko", extra.as_str())], None, 0);
}

// Ordinary filesystem conflicts are not injected backend failures. No generated code executes.
fn check_process_objects(
    publication: &lang_codegen::d10_experiment::SplitNamedEntryProgram<'_, '_>,
    directory: &std::path::Path,
) {
    let object = directory.join("process.o");
    publication.emit_process_object(&object).unwrap();
    let bytes = fs::read(&object).unwrap();
    assert!(!bytes.is_empty());
    let output = Command::new("/usr/bin/nm")
        .args(if cfg!(target_os = "macos") {
            ["-j", "-U"]
        } else {
            ["-j", "--defined-only"]
        })
        .arg(&object)
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    let symbols = String::from_utf8(output.stdout).unwrap();
    let main = if cfg!(target_os = "macos") {
        "_main"
    } else {
        "main"
    };
    assert_eq!(
        symbols.lines().filter(|s| *s == main).count(),
        1,
        "{symbols}"
    );
    for source in ["run", "entry"] {
        assert_eq!(
            symbols
                .lines()
                .filter(|s| s.contains(&format!(".{source}.d")))
                .count(),
            1,
            "{symbols}"
        );
    }
    let fresh = directory.join("fresh.o");
    publication.emit_new_process_object(&fresh).unwrap();
    let fresh_bytes = fs::read(&fresh).unwrap();
    assert!(!fresh_bytes.is_empty());
    let error = publication.emit_new_process_object(&fresh).unwrap_err();
    assert_eq!(
        error.kind(),
        lang_codegen::NativeObjectErrorKind::OutputAlreadyExists
    );
    assert_eq!(fs::read(&fresh).unwrap(), fresh_bytes);
    assert_eq!(fs::read(&object).unwrap(), bytes);
    let folder = directory.join("folder");
    fs::create_dir(&folder).unwrap();
    publication.emit_new_process_object(&folder).unwrap_err();
    assert!(folder.is_dir());
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link = directory.join("linked.o");
        symlink(&fresh, &link).unwrap();
        let absent = directory.join("absent.o");
        let dangling = directory.join("dangling.o");
        symlink(&absent, &dangling).unwrap();
        for (path, target) in [(&link, &fresh), (&dangling, &absent)] {
            publication.emit_new_process_object(path).unwrap_err();
            assert!(fs::symlink_metadata(path).unwrap().file_type().is_symlink());
            assert_eq!(fs::read_link(path).unwrap(), *target);
        }
        assert!(!absent.exists());
        assert_eq!(fs::read(&fresh).unwrap(), fresh_bytes);
    }
    assert!(fs::read_dir(directory).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".koven-unit-object-")
    }));
}
