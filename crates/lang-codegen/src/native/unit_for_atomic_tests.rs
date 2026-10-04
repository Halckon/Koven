//! Public actual for products, malformed input rejection, and reservation isolation.
use super::{TestDirectory, analyze_sources};
use crate::{NativeObjectErrorKind, emit_native_unit_object, native::NEXT_UNIT_OBJECT_TEMPORARY};
use lang_frontend::{name_resolution::SourceUnitInput, source::SourceMap};
use std::{collections::BTreeMap, fs, process::Command, sync::atomic::Ordering};

fn bytes(path: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
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
        // Unit lowering retains its existing for capability boundary. The actual for
        // product rejects before reservation; recovery uses a supported unit below.
        let before = bytes(&directory.0);
        let reservations = NEXT_UNIT_OBJECT_TEMPORARY.load(Ordering::Relaxed);
        let guard = crate::llvm::emission_failure::Guard::new(directory.join("keep/object.o"));
        let error = emit_native_unit_object(
            &analysis.sources,
            &inputs,
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            entry,
            &output,
        )
        .unwrap_err();
        assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
        let for_span = analysis
            .consumer
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
        let positive = analyze_sources(
            "package p\nfun marker(): Unit {}",
            "package q\nfun entry(): Unit { println(\"ready\") }",
        );
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
        assert_eq!(run.stdout, b"ready\n");
        assert!(run.stderr.is_empty());
    }
}
