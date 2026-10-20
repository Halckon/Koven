use super::super::{
    NativeObjectErrorKind, SymbolKind, TestDirectory, emit_link_and_run,
};
use std::{collections::BTreeMap, fs};
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;

fn snapshot(path: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().into_string().unwrap(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn actual_single_file_for_rejects_foreign_sources_and_facts_without_touching_directory() {
    let source = "fun entry(): Unit { for (item in arrayOf(1, 2, 3)) { println(\"visit\") } }";
    let first = UnitSource::analyze("single-for.ko", source).expect("original source must pass strict Unit admission");
    let second = UnitSource::analyze("single-for.ko", source).expect("original source must pass strict Unit admission");
    for present in [false, true] {
        let directory = TestDirectory::create();
        let output = directory.join("program.o");
        fs::write(directory.join("keep"), b"sibling").unwrap();
        if present {
            fs::write(&output, b"old object").unwrap();
        }
        for (foreign_sources, typed, owned) in [
            (Some(&second), &first, &first),
            (None, &second, &second),
            (None, &first, &second),
        ] {
            let before = snapshot(&directory.0);
            let guard =
                crate::llvm::emission_failure::Guard::new(directory.join("keep").join("object.o"));
            let entry = first.declaration("entry", SymbolKind::Function);
            let error = match foreign_sources {
                Some(sources) => first.emit_with_sources_from(sources, entry, &output),
                None => first.emit_with_products_from(typed, owned, entry, &output),
            }
            .unwrap_err();
            assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
            assert_eq!(error.span(), None);
            assert_eq!(guard.calls(), 0);
            assert_eq!(snapshot(&directory.0), before);
            drop(guard);
        }
        first.emit(first.declaration("entry", SymbolKind::Function), &output)
        .unwrap();
        crate::test_support::assert_native_object(&fs::read(output).unwrap());
    }
    let run = emit_link_and_run("single-for.ko", source, "entry");
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(run.stdout, b"visit\nvisit\nvisit\n");
    assert!(run.stderr.is_empty());
}

#[test]
fn independent_analysis_chains_render_identical_for_ssa_and_verified_llvm() {
    let source = "fun entry(): Unit { for (item in listOf(1, 2, 3)) { if (item == 2) { continue }; if (item == 3) { break }; println(\"visit\") } }";
    let mut expected = None;
    for _ in 0..3 {
        let analysis = UnitSource::analyze("deterministic-for.ko", source).expect("original source must pass strict Unit admission");
        let (program, entry) = analysis.lower_entry(analysis.declaration("entry", SymbolKind::Function))
        .unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        let rendered = (
            crate::ssa::render_program(&program),
            crate::llvm::render_verified_program_with_entry(&program, entry).unwrap(),
        );
        if let Some(expected) = &expected {
            assert_eq!(&rendered, expected);
        } else {
            expected = Some(rendered);
        }
    }
}

#[test]
fn dynamic_resource_and_non_source_owner_choices_reject_before_llvm_and_output() {
    for source in [
        "class Leaf(val name: String) { deinit() { println(this.name) } }\nfun scan(flag: Boolean): Unit { var guard = Leaf(\"initial\"); for (item in arrayOf(1, 2)) { if (flag) { guard = Leaf(\"replacement\") } } }\nfun entry(): Unit { scan(true) }",
        "fun scan(flag: Boolean): Unit { var text = \"initial\"; for (item in arrayOf(1, 2)) { if (flag) { text = \"replacement\" } }; println(text) }\nfun entry(): Unit { scan(true) }",
    ] {
        let analysis = UnitSource::analyze("bounded-presence.ko", source).expect("original source must pass strict Unit admission");
        for present in [false, true] {
            let directory = TestDirectory::create();
            let output = directory.join("program.o");
            fs::write(directory.join("keep"), b"sibling").unwrap();
            if present {
                fs::write(&output, b"old object").unwrap();
            }
            let before = snapshot(&directory.0);
            let guard = crate::llvm::emission_failure::Guard::new(directory.join("keep/object.o"));
            let error = analysis.emit(analysis.declaration("entry", SymbolKind::Function), &output)
            .unwrap_err();
            assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
            assert!(error.span().is_some());
            assert_eq!(guard.calls(), 0);
            assert_eq!(snapshot(&directory.0), before);
        }
    }
}
