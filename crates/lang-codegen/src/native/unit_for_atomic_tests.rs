//! Public actual for products, malformed input rejection, and reservation isolation.
use super::{TestDirectory, analyze_sources};
use crate::{NativeObjectErrorKind, emit_native_unit_object, native::NEXT_UNIT_OBJECT_TEMPORARY};
use lang_frontend::{name_resolution::SourceUnitInput, source::SourceMap};
use std::{collections::BTreeMap, fs, process::Command, sync::atomic::Ordering};

pub(super) fn bytes(path: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
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
fn actual_for_rejections_are_atomic_before_reservation_and_llvm() {
    const CHILD: &str = "KOVEN_FOR_ATOMIC_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = Command::new(std::env::current_exe().unwrap()).args(["--exact", "native::unit_tests::for_atomic::actual_for_rejections_are_atomic_before_reservation_and_llvm", "--nocapture"]).env(CHILD, "1").output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
        return;
    }
    let analysis = analyze_sources(
        "package p\nfun source(): Array<Int> = arrayOf(1, 2)",
        "package q\nfun entry(): Unit { for (item in p.source()) { println(\"visit\") } }",
    );
    let foreign = analyze_sources(
        "package p\nfun source(): Array<Int> = arrayOf(1, 2)",
        "package q\nfun entry(): Unit { for (item in p.source()) { println(\"visit\") } }",
    );
    let inputs = analysis.inputs();
    let invalid = [
        SourceUnitInput::new(
            "changed-root",
            "p/provider.ko",
            analysis.provider_source,
            &analysis.provider,
        ),
        inputs[1],
    ];
    let duplicate = [inputs[0], inputs[0]];
    let empty_sources = SourceMap::default();
    let entry = analysis.declaration("q", "entry");
    for present in [false, true] {
        let directory = TestDirectory::create();
        let output = directory.join("output.o");
        fs::write(directory.join("keep"), b"sibling bytes").unwrap();
        if present {
            fs::write(&output, b"old object").unwrap();
        }
        for (sources, inputs) in [
            (&empty_sources, &inputs[..]),
            (&foreign.sources, &inputs[..]),
            (&analysis.sources, &invalid[..]),
            (&analysis.sources, &duplicate[..]),
        ] {
            let before = bytes(&directory.0);
            let reservations = NEXT_UNIT_OBJECT_TEMPORARY.load(Ordering::Relaxed);
            let blocker = directory.join("keep");
            let guard = crate::llvm::emission_failure::Guard::new(blocker.join("object.o"));
            let error = emit_native_unit_object(
                sources,
                inputs,
                &analysis.names,
                &analysis.environment,
                &analysis.typed,
                &analysis.owned,
                entry,
                &output,
            )
            .unwrap_err();
            assert!(
                matches!(
                    error.kind(),
                    NativeObjectErrorKind::MismatchedAnalysis | NativeObjectErrorKind::InvalidModel
                ),
                "{error}"
            );
            assert_eq!(guard.calls(), 0);
            assert_eq!(
                NEXT_UNIT_OBJECT_TEMPORARY.load(Ordering::Relaxed),
                reservations
            );
            assert_eq!(bytes(&directory.0), before);
            drop(guard);
        }
        // A frontend-valid field source stays outside the native capability. Failure
        // must precede reservation/LLVM and preserve both target and sibling bytes.
        let deferred = analyze_sources(
            "package p\nclass Holder(val values: Array<Int>) { fun visit(): Unit { for (item in this.values) { println(\"visit\") } } }",
            "package q\nimport p.Holder\nfun entry(): Unit { Holder(arrayOf(1)).visit() }",
        );
        let before = bytes(&directory.0);
        let reservations = NEXT_UNIT_OBJECT_TEMPORARY.load(Ordering::Relaxed);
        let guard = crate::llvm::emission_failure::Guard::new(directory.join("keep/object.o"));
        let error = emit_native_unit_object(
            &deferred.sources,
            &deferred.inputs(),
            &deferred.names,
            &deferred.environment,
            &deferred.typed,
            &deferred.owned,
            deferred.declaration("q", "entry"),
            &output,
        )
        .unwrap_err();
        assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
        let for_span = deferred
            .provider
            .ast()
            .statements()
            .iter()
            .find(|(_, node)| {
                matches!(node.payload(), lang_frontend::parser::Statement::For { .. })
            })
            .unwrap()
            .1
            .span();
        assert_eq!(error.span(), Some(for_span));
        assert_eq!(guard.calls(), 0);
        assert_eq!(
            NEXT_UNIT_OBJECT_TEMPORARY.load(Ordering::Relaxed),
            reservations
        );
        assert_eq!(bytes(&directory.0), before);
        drop(guard);
        // Recovery now uses the exact legal for analysis that was previously rejected.
        let positive = &analysis;
        emit_native_unit_object(
            &positive.sources,
            &positive.inputs(),
            &positive.names,
            &positive.environment,
            &positive.typed,
            &positive.owned,
            positive.declaration("q", "entry"),
            &output,
        )
        .unwrap();
        let executable = directory.join("program");
        let link = Command::new(crate::test_support::clang())
            .arg(&output)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(link.status.success(), "{link:?}");
        let run = Command::new(executable).output().unwrap();
        assert_eq!(run.status.code(), Some(0));
        assert_eq!(run.stdout, b"visit\nvisit\n");
        assert!(run.stderr.is_empty());
    }
}

#[test]
fn actual_unit_for_executes_source_once_and_keeps_borrowed_elements() {
    let analysis = analyze_sources(
        "package p\nfun source(): Array<String> { println(\"source\"); return arrayOf(\"first\", \"second\") }",
        "package q\nfun entry(): Unit { for (item in p.source()) { println(item) }; println(\"after\") }",
    );
    let directory = TestDirectory::create();
    let output = directory.join("iteration.o");
    emit_native_unit_object(
        &analysis.sources,
        &analysis.inputs(),
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
        &output,
    )
    .expect("legal unit for lowers");
    let executable = directory.join("iteration");
    let link = Command::new(crate::test_support::clang())
        .arg(&output)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(link.status.success(), "{link:?}");
    let run = Command::new(executable).output().unwrap();
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(run.stdout, b"source\nfirst\nsecond\nafter\n");
    assert!(run.stderr.is_empty());
}
