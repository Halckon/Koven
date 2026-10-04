//! Frontend success does not grant native capability for Inout, field or captured Borrow sources.
use super::*;
use lang_frontend::ownership_checking::check_compilation_unit_constant_ownership;

#[test]
fn unit_for_deferred_sources_reject_both_native_views_without_artifact_changes() {
    let cases = [
        (
            "package p\nclass Holder(val xs: Array<Int>) { fun visit(): Unit { for (x in this.xs) {} } }",
            "package q\nimport p.Holder\nfun entry(): Unit { Holder(arrayOf(1)).visit() }",
            "for (x in this.xs) {}",
        ),
        (
            "package p\nclass Holder(val xs: List<Int>) { fun visit(): Unit { for (x in xs) {} } }",
            "package q\nimport p.Holder\nfun entry(): Unit { Holder(listOf(1)).visit() }",
            "for (x in xs) {}",
        ),
        (
            "package p\nclass Holder(val xs: MutableList<Int>)\nfun visit(holder: Holder): Unit { for (x in holder.xs) {} }",
            "package q\nimport p.Holder\nfun entry(): Unit { p.visit(Holder(mutableListOf(1))) }",
            "for (x in holder.xs) {}",
        ),
        (
            "package p\nfun visit(inout xs: Array<Int>): Unit { for (x in xs) {} }",
            "package q\nfun entry(): Unit { var xs = arrayOf(1); p.visit(&xs) }",
            "inout xs: Array<Int>",
        ),
    ];
    for (provider, consumer, rejected) in cases {
        let analysis = analyze_sources(provider, consumer);
        let inputs = analysis.inputs();
        let entry = analysis.declaration("q", "entry");
        for constants in [false, true] {
            for present in [false, true] {
                let directory = TestDirectory::create();
                let output = directory.join("output.o");
                fs::write(directory.join("keep"), b"sibling bytes").unwrap();
                if present {
                    fs::write(&output, b"previous artifact").unwrap();
                }
                let before = super::for_atomic::bytes(&directory.0);
                let guard =
                    crate::llvm::emission_failure::Guard::new(directory.join("keep/object.o"));
                let error = if constants {
                    let typed = check_compilation_unit_types(
                        &analysis.sources,
                        &inputs,
                        &analysis.names,
                        &analysis.environment,
                    )
                    .unwrap()
                    .validate_constants()
                    .unwrap();
                    let owned = check_compilation_unit_constant_ownership(
                        &analysis.sources,
                        &inputs,
                        &analysis.names,
                        &analysis.environment,
                        &typed,
                    )
                    .unwrap()
                    .validate()
                    .unwrap();
                    crate::emit_native_constant_unit_object(
                        &analysis.sources,
                        &inputs,
                        &analysis.names,
                        &analysis.environment,
                        &typed,
                        &owned,
                        entry,
                        &output,
                    )
                    .unwrap_err()
                } else {
                    emit_native_unit_object(
                        &analysis.sources,
                        &inputs,
                        &analysis.names,
                        &analysis.environment,
                        &analysis.typed,
                        &analysis.owned,
                        entry,
                        &output,
                    )
                    .unwrap_err()
                };
                assert_eq!(
                    error.kind(),
                    NativeObjectErrorKind::UnsupportedSource,
                    "{error}"
                );
                let rejected_text = analysis
                    .sources
                    .slice(error.span().expect("precise unsupported span"))
                    .unwrap();
                assert_eq!(rejected_text, rejected);
                assert_eq!(guard.calls(), 0, "failure must precede LLVM emission");
                assert_eq!(super::for_atomic::bytes(&directory.0), before);
            }
        }
    }
}

#[test]
fn unit_for_captured_borrow_closure_retains_explicit_native_boundary() {
    let analysis = analyze_sources(
        "package p\nfun visit(): Unit { for (item in listOf(\"item\")) { val action: () -> Unit = { println(item) }; action() } }",
        "package q\nfun entry(): Unit { p.visit() }",
    );
    assert!(
        analysis
            .owned
            .ownership()
            .captures()
            .iter()
            .any(|capture| capture.effect()
                == lang_frontend::ownership_checking::ClosureCaptureEffect::Borrow),
        "the frontend publishes a borrowed capture even though native representation is deferred"
    );
    let inputs = analysis.inputs();
    let directory = TestDirectory::create();
    let output = directory.join("output.o");
    fs::write(&output, b"previous artifact").unwrap();
    fs::write(directory.join("keep"), b"sibling").unwrap();
    let before = super::for_atomic::bytes(&directory.0);
    for constants in [false, true] {
        let guard = crate::llvm::emission_failure::Guard::new(directory.join("keep/object.o"));
        let error = if constants {
            let typed = check_compilation_unit_types(
                &analysis.sources,
                &inputs,
                &analysis.names,
                &analysis.environment,
            )
            .unwrap()
            .validate_constants()
            .unwrap();
            let owned = check_compilation_unit_constant_ownership(
                &analysis.sources,
                &inputs,
                &analysis.names,
                &analysis.environment,
                &typed,
            )
            .unwrap()
            .validate()
            .unwrap();
            crate::emit_native_constant_unit_object(
                &analysis.sources,
                &inputs,
                &analysis.names,
                &analysis.environment,
                &typed,
                &owned,
                analysis.declaration("q", "entry"),
                &output,
            )
            .unwrap_err()
        } else {
            emit_native_unit_object(
                &analysis.sources,
                &inputs,
                &analysis.names,
                &analysis.environment,
                &analysis.typed,
                &analysis.owned,
                analysis.declaration("q", "entry"),
                &output,
            )
            .unwrap_err()
        };
        assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
        assert!(error.span().is_some());
        assert_eq!(guard.calls(), 0);
        assert_eq!(super::for_atomic::bytes(&directory.0), before);
    }
}

#[test]
fn unit_for_constant_foreign_ownership_rejects_before_native_output() {
    let analysis = analyze_sources(
        "package p\nfun source(): List<Int> = listOf(1)",
        "package q\nfun entry(): Unit { for (_ in p.source()) {} }",
    );
    let inputs = analysis.inputs();
    let typed = check_compilation_unit_types(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
    )
    .unwrap()
    .validate_constants()
    .unwrap();
    let foreign_typed = check_compilation_unit_types(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
    )
    .unwrap()
    .validate_constants()
    .unwrap();
    let foreign_owned = check_compilation_unit_constant_ownership(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &foreign_typed,
    )
    .unwrap()
    .validate()
    .unwrap();
    let directory = TestDirectory::create();
    let output = directory.join("output.o");
    fs::write(&output, b"previous artifact").unwrap();
    fs::write(directory.join("keep"), b"sibling").unwrap();
    let before = super::for_atomic::bytes(&directory.0);
    let guard = crate::llvm::emission_failure::Guard::new(directory.join("keep/object.o"));
    let error = crate::emit_native_constant_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &typed,
        &foreign_owned,
        analysis.declaration("q", "entry"),
        &output,
    )
    .unwrap_err();
    assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
    assert_eq!(guard.calls(), 0);
    assert_eq!(super::for_atomic::bytes(&directory.0), before);
}
