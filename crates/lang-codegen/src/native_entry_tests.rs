//! Native entry validation, argv bridge, and object publication boundaries.

use super::{NativeObjectErrorKind, TestDirectory, UnitSource};
use crate::{NativeUnitEntry, test_support::diagnostics::assert_diagnostic};
use lang_frontend::name_resolution::SymbolKind;
use std::{fs, process::Command};

#[test]
fn resolved_unit_entry_emits_the_existing_native_wrapper_object() {
    let analysis = UnitSource::analyze("facade.ko", "fun bootstrap(): Unit {}\n")
        .expect("original source must pass strict Unit admission");
    let directory = TestDirectory::create();
    let object = directory.join("bootstrap.o");

    analysis
        .emit(
            analysis.declaration("bootstrap", SymbolKind::Function),
            &object,
        )
        .expect("resolved entry must emit object");

    crate::test_support::assert_native_object(&fs::read(&object).expect("object bytes"));
    let symbols = Command::new("/usr/bin/nm")
        .arg("-gj")
        .arg(&object)
        .output()
        .expect("nm must launch");
    assert!(symbols.status.success(), "{symbols:?}");
    let names = String::from_utf8(symbols.stdout).expect("nm output must be UTF-8");
    assert_eq!(
        names
            .lines()
            .filter(|name| *name == crate::test_support::main_symbol())
            .count(),
        1
    );
}

#[test]
#[cfg(unix)]
fn borrowed_argv_entry_accepts_utf8_boundaries_and_rejects_invalid_sequences_before_call() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let analysis = UnitSource::analyze("argv.ko", "fun main(args: Array<String>): Unit {}\n")
        .expect("original source must pass strict Unit admission");
    analysis.assert_frontend_clean();
    let directory = TestDirectory::create();
    let object = directory.join("argv.o");
    let executable = directory.join("argv");
    analysis
        .emit(
            NativeUnitEntry::BorrowedArguments(analysis.declaration("main", SymbolKind::Function)),
            &object,
        )
        .expect("borrowed argv entry object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");

    for arguments in [
        vec![],
        vec![b"".to_vec(), b"ascii".to_vec()],
        vec![
            "\u{80}\u{7ff}\u{800}\u{ffff}\u{10000}\u{10ffff}"
                .as_bytes()
                .to_vec(),
        ],
    ] {
        let status = Command::new(&executable)
            .args(arguments.into_iter().map(OsString::from_vec))
            .status()
            .expect("linked executable must launch");
        assert!(
            status.success(),
            "valid UTF-8 argv must reach and return from main"
        );
    }

    for invalid in [
        vec![0xc0, 0x80],
        vec![0xc1, 0xbf],
        vec![0xe0, 0x9f, 0x80],
        vec![0xed, 0xa0, 0x80],
        vec![0xf0, 0x8f, 0xbf, 0xbf],
        vec![0xf4, 0x90, 0x80, 0x80],
        vec![0xe2, 0x82],
        vec![0xff],
    ] {
        let status = Command::new(&executable)
            .arg(OsString::from_vec(invalid))
            .status()
            .expect("linked executable must launch");
        assert_eq!(
            status.code(),
            Some(1),
            "invalid UTF-8 must fail operationally"
        );
    }

    let observing = UnitSource::analyze(
        "argv-observe.ko",
        "fun main(args: Array<String>): Unit {\n\
             val zero: Int = 0\n\
             val one: Int = 1\n\
             val two: Int = 2\n\
             val three: Int = 3\n\
             if (true) { println(args[zero]) }\n\
             if (true) { println(args[one]) }\n\
             if (true) { println(args[two]) }\n\
             println(args[three])\n\
         }\n",
    )
    .expect("original source must pass strict Unit admission");
    observing.assert_frontend_clean();
    let observing_object = directory.join("argv-observe.o");
    let observing_executable = directory.join("argv-observe");
    observing
        .emit(
            NativeUnitEntry::BorrowedArguments(observing.declaration("main", SymbolKind::Function)),
            &observing_object,
        )
        .expect("observing borrowed argv entry object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&observing_object)
        .arg("-o")
        .arg(&observing_executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let output = Command::new(&observing_executable)
        .args(["first", "", "你好", "last"])
        .output()
        .expect("linked executable must launch");
    assert_diagnostic!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, "first\n\n你好\nlast\n".as_bytes());
}

#[test]
fn entry_shape_and_analysis_identity_fail_before_object_emission() {
    let directory = TestDirectory::create();

    for (index, (source, entry_name, kind)) in [
        (
            "fun parameterized(value: Int): Unit {}",
            "parameterized",
            SymbolKind::Function,
        ),
        (
            "fun <T> generic(): Unit {}",
            "generic",
            SymbolKind::Function,
        ),
        ("fun nonunit(): Int = 1", "nonunit", SymbolKind::Function),
        ("class Value {}", "Value", SymbolKind::Classifier),
    ]
    .into_iter()
    .enumerate()
    {
        let analysis = UnitSource::analyze("invalid-entry.ko", source)
            .expect("original source must pass strict Unit admission");
        let output = directory.join(&format!("invalid-{index}.o"));
        let error = analysis
            .emit(analysis.declaration(entry_name, kind), &output)
            .expect_err("invalid entry must fail");
        assert_eq!(error.kind(), NativeObjectErrorKind::InvalidEntry);
        assert!(!output.exists());
    }

    let one_symbol = UnitSource::analyze("one.ko", "fun only(): Unit {}")
        .expect("original source must pass strict Unit admission");
    let two_symbols = UnitSource::analyze("two.ko", "fun first(): Unit {}\nfun foreign(): Unit {}")
        .expect("original source must pass strict Unit admission");
    let unknown_output = directory.join("unknown.o");
    let error = one_symbol
        .emit(
            two_symbols.declaration("foreign", SymbolKind::Function),
            &unknown_output,
        )
        .expect_err("unknown symbol id must fail");
    assert_eq!(error.kind(), NativeObjectErrorKind::InvalidEntry);
    assert!(!unknown_output.exists());

    let first = UnitSource::analyze("first.ko", "fun bootstrap(): Unit {}")
        .expect("original source must pass strict Unit admission");
    let second = UnitSource::analyze("second.ko", "fun bootstrap(): Unit {}")
        .expect("original source must pass strict Unit admission");
    let output = directory.join("foreign.o");
    let error = first
        .emit_with_products_from(
            &second,
            &second,
            first.declaration("bootstrap", SymbolKind::Function),
            &output,
        )
        .expect_err("mixed analysis must fail");
    assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
    assert!(!output.exists());
}

#[test]
fn frontend_diagnostics_and_unsupported_source_do_not_write_objects() {
    let directory = TestDirectory::create();
    let diagnostic_output = directory.join("diagnostic.o");
    fs::write(directory.join("keep"), b"unchanged sibling").unwrap();
    let before = directory.snapshot();
    let Err(diagnostic) = UnitSource::analyze("diagnostic.ko", "fun broken(): Unit { missing }")
    else {
        panic!("name diagnostics must reject before an owned Unit source exists");
    };
    assert!(
        diagnostic
            .report
            .diagnostics
            .iter()
            .any(|(stage, _)| *stage == "names")
    );
    assert_eq!(directory.snapshot(), before);
    assert!(!diagnostic_output.exists());

    let unsupported = UnitSource::analyze(
        "unsupported.ko",
        "fun closure(): Unit { val callback: ()->Unit = { val nested: ()->Unit = {}\nnested() }\ncallback() }",
    ).expect("original source must pass strict Unit admission");
    unsupported.assert_frontend_clean();
    let unsupported_output = directory.join("unsupported.o");
    let error = unsupported
        .emit(
            unsupported.declaration("closure", SymbolKind::Function),
            &unsupported_output,
        )
        .expect_err("unsupported source must fail");
    assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
    assert!(!unsupported_output.exists());
}

#[test]
fn standard_error_emits_object_while_interpolated_message_fails_before_writing() {
    let directory = TestDirectory::create();
    let canonical = UnitSource::analyze(
        "error-entry.ko",
        "fun abortEntry(): Unit { error(\"fatal\") }",
    )
    .expect("original source must pass strict Unit admission");
    canonical.assert_frontend_clean();
    let object = directory.join("error.o");
    canonical
        .emit(
            canonical.declaration("abortEntry", SymbolKind::Function),
            &object,
        )
        .expect("canonical standard error must emit an object");
    assert!(object.is_file());

    let unsupported = UnitSource::analyze(
        "interpolated-error.ko",
        "fun abortEntry(): Unit { error(\"${1}\") }",
    )
    .expect("original source must pass strict Unit admission");
    unsupported.assert_frontend_clean();
    let rejected = directory.join("interpolated.o");
    let error = unsupported
        .emit(
            unsupported.declaration("abortEntry", SymbolKind::Function),
            &rejected,
        )
        .expect_err("interpolated String remains unsupported");
    assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
    assert!(!rejected.exists());
}
