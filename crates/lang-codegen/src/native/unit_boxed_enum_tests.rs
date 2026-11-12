//! SPEC-0230: boxed enum owners cross compilation-unit call boundaries.
use super::{Command, TestDirectory, UnitAnalysis, analyze_sources, emit_native_unit_object};
use crate::native_tests::boxed_enum_tests::{
    RECURSIVE_ALLOCATIONS, RECURSIVE_STDOUT, SIMPLE_DECLARATIONS, SIMPLE_STDOUT, assert_success,
    recursive_declarations, run_counted_allocations,
};
use crate::test_support::diagnostics::assert_text_eq;

fn simple_analysis() -> UnitAnalysis {
    analyze_sources(
        &format!("package p\n{SIMPLE_DECLARATIONS}"),
        "package q\nfun entry(): Unit {\n\
         val first = p.take(p.make())\n\
         p.take(p.makeEmpty())\n\
         }",
    )
}

fn recursive_analysis(enum_first: bool) -> UnitAnalysis {
    let declarations = recursive_declarations(enum_first);
    analyze_sources(
        &format!("package p\n{declarations}"),
        "package q\nfun entry(): Unit {\n\
         val boxed = p.makeDeepTree()\n\
         val first = p.takeBox(boxed)\n\
         val inline = p.makeInline()\n\
         p.takeInline(inline)\n\
         }",
    )
}

#[test]
fn boxed_enum_cross_file_return_and_value_delivery_run_natively() {
    emit_link_run(&simple_analysis(), SIMPLE_STDOUT);
}

#[test]
fn recursive_boxed_enum_cross_file_tree_and_inline_root_run_natively() {
    for enum_first in [false, true] {
        emit_link_run(&recursive_analysis(enum_first), RECURSIVE_STDOUT);
    }
}

#[test]
fn boxed_enum_cross_file_cases_allocate_and_free_each_owner_once() {
    let llvm = lower_to_llvm(&simple_analysis());
    let run = run_counted_allocations(&llvm, 2);
    assert_success(&run, SIMPLE_STDOUT);
}

#[test]
fn recursive_boxed_enum_cross_file_drop_frees_all_descendants_once() {
    for enum_first in [false, true] {
        let llvm = lower_to_llvm(&recursive_analysis(enum_first));
        let run = run_counted_allocations(&llvm, RECURSIVE_ALLOCATIONS);
        assert_success(&run, RECURSIVE_STDOUT);
    }
}

fn lower_to_llvm(analysis: &UnitAnalysis) -> String {
    let inputs = analysis.inputs();
    let reversed = [inputs[1], inputs[0]];
    let lower = |inputs: &[_]| {
        super::lower_scalar_unit_with_entry(
            &analysis.sources,
            inputs,
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
        )
        .expect("boxed enum unit must lower to verified SSA")
    };
    let (program, entry) = lower(&inputs);
    let (backward, backward_entry) = lower(&reversed);
    assert_eq!(entry.index(), backward_entry.index());
    assert_eq!(entry.module().index(), backward_entry.module().index());
    assert_text_eq!(
        crate::ssa::render_program(&program),
        crate::ssa::render_program(&backward),
        "input order must not change recursive type registration"
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("boxed enum unit LLVM must verify");
    assert_eq!(
        llvm,
        crate::llvm::render_verified_program_with_entry(&backward, backward_entry)
            .expect("reversed boxed enum unit LLVM must verify")
    );
    llvm
}

fn emit_link_run(analysis: &UnitAnalysis, stdout: &[u8]) {
    let directory = TestDirectory::create();
    let object = directory.join("boxed-enum.o");
    let executable = directory.join("boxed-enum");
    emit_native_unit_object(
        &analysis.sources,
        &analysis.inputs(),
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
        &object,
    )
    .expect("boxed enum unit must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("boxed enum executable must launch");
    assert_success(&run, stdout);
}

pub(crate) mod iteration_boundaries {
//! Deferred Inout/field providers and the supported natural captured-Borrow iteration path.
use super::super::*;
use crate::test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic};
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
                let before = super::super::for_atomic::bytes(&directory.0);
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
                assert_eq_diagnostic!(
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
                assert_eq!(super::super::for_atomic::bytes(&directory.0), before);
            }
        }
    }
}

#[test]
fn unit_for_captured_borrow_closure_executes_and_releases_each_iteration() {
    for (elements, expected) in [
        ("", ""),
        ("\"item\"", "item\n"),
        ("\"one\", \"二\", \"three\"", "one\n二\nthree\n"),
    ] {
        let provider = format!(
            "package p\nfun visit(): Unit {{ for (item in listOf<String>({elements})) {{ val action: () -> Unit = {{ println(item) }}; action() }} }}"
        );
        let consumer = "package q\nfun entry(): Unit { p.visit() }";
        let analysis = analyze_sources(&provider, consumer);
        assert!(
            analysis
                .owned
                .ownership()
                .captures()
                .iter()
                .any(|capture| capture.effect()
                    == lang_frontend::ownership_checking::ClosureCaptureEffect::Borrow)
        );
        for constants in [false, true] {
            let (output, _) =
                super::super::resource_exchange_tests::run_unit(&provider, consumer, constants);
            assert_diagnostic!(output.status.success(), "{output:?}");
            assert_eq!(output.stdout, expected.as_bytes());
            assert_diagnostic!(output.stderr.is_empty(), "{output:?}");
        }
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
    let before = super::super::for_atomic::bytes(&directory.0);
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
    assert_eq!(super::super::for_atomic::bytes(&directory.0), before);
}
}

pub(crate) mod for_atomic {
//! Public actual for products, malformed input rejection, and reservation isolation.
use super::super::{TestDirectory, analyze_sources};
use crate::test_support::diagnostics::assert_diagnostic;
use crate::{NativeObjectErrorKind, emit_native_unit_object, native::NEXT_UNIT_OBJECT_TEMPORARY};
use lang_frontend::{name_resolution::SourceUnitInput, source::SourceMap};
use std::{collections::BTreeMap, fs, process::Command, sync::atomic::Ordering};

pub(crate) fn bytes(path: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
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
        assert_diagnostic!(output.status.success(), "{output:?}");
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
            assert_diagnostic!(
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
}

pub(crate) mod container_remove_at_tests {
//! SPEC-0282: native MutableList.removeAt element removal and memory shift.

use super::super::*;
use crate::test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic};

#[test]
fn unit_container_remove_at_native_primitive_sequential() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf(10, 20, 30, 40, 50)
            if (list.size != 5) { error("initial size mismatch") }

            val first = list.removeAt(0)
            if (first != 10) { error("first removed mismatch") }
            if (list.size != 4) { error("size after remove 0 mismatch") }
            if (list[0] != 20) { error("elem 0 mismatch") }
            if (list[1] != 30) { error("elem 1 mismatch") }
            if (list[2] != 40) { error("elem 2 mismatch") }
            if (list[3] != 50) { error("elem 3 mismatch") }

            val mid = list.removeAt(1)
            if (mid != 30) { error("mid removed mismatch") }
            if (list.size != 3) { error("size after remove mid mismatch") }
            if (list[0] != 20) { error("elem 0 mismatch") }
            if (list[1] != 40) { error("elem 1 mismatch") }
            if (list[2] != 50) { error("elem 2 mismatch") }

            val last = list.removeAt(2)
            if (last != 50) { error("last removed mismatch") }
            if (list.size != 2) { error("size after remove last mismatch") }
            if (list[0] != 20) { error("elem 0 mismatch") }
            if (list[1] != 40) { error("elem 1 mismatch") }

            list.add(99)
            if (list.size != 3) { error("size after add mismatch") }
            if (list[0] != 20) { error("elem 0 mismatch") }
            if (list[1] != 40) { error("elem 1 mismatch") }
            if (list[2] != 99) { error("elem 2 mismatch") }

            println("remove-at-primitive-ok")
        }"#,
    );
    run(&analysis, b"remove-at-primitive-ok\n");
}

#[test]
fn unit_container_remove_at_native_move_only_elements_lifecycle() {
    let analysis = analyze_sources(
        r#"package p
        class Resource(val name: String) {
            deinit() {
                println(this.name)
            }
        }"#,
        r#"package q
        import p.Resource
        fun consume(res: Resource): Unit {
            println("consuming")
            println(res.name)
        }
        fun runTest(): Unit {
            var list = mutableListOf<Resource>()
            list.add(Resource("r0"))
            list.add(Resource("r1"))
            list.add(Resource("r2"))
            println("before-remove")
            consume(list.removeAt(1))
            println("after-consume")
            if (list.size != 2) { error("size mismatch") }
        }
        fun entry(): Unit {
            runTest()
            println("done-all")
        }"#,
    );
    run(
        &analysis,
        b"before-remove\nconsuming\nr1\nr1\nafter-consume\nr2\nr0\ndone-all\n",
    );
}

#[test]
fn unit_container_remove_at_native_shrink_to_empty() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf(1, 2)
            val a = list.removeAt(0)
            if (a != 1) { error("mismatch a") }
            val b = list.removeAt(0)
            if (b != 2) { error("mismatch b") }
            if (list.size != 0) { error("size should be 0") }
            list.add(42)
            if (list.size != 1) { error("size after add mismatch") }
            if (list[0] != 42) { error("elem 0 mismatch") }
            println("shrink-to-empty-ok")
        }"#,
    );
    run(&analysis, b"shrink-to-empty-ok\n");
}

#[test]
fn unit_container_remove_at_native_out_of_bounds_aborts() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf(1, 2)
            println("before-out-of-bounds")
            val x = list.removeAt(2)
            println("unexpected-reach")
        }"#,
    );
    run_abort(&analysis, b"before-out-of-bounds\n");
}

fn run(analysis: &UnitAnalysis, expected: &[u8]) {
    let directory = TestDirectory::create();
    let object = directory.join("remove_at.o");
    let executable = directory.join("remove_at");
    emit_native_unit_object(
        &analysis.sources,
        &analysis.inputs(),
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
        &object,
    )
    .expect("source removeAt emits a verified native object");
    crate::test_support::assert_native_object(&fs::read(&object).unwrap());
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("native link runs");
    assert_diagnostic!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let output = Command::new(&executable)
        .output()
        .expect("native program runs");
    assert_diagnostic!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.is_empty());
}

fn run_abort(analysis: &UnitAnalysis, expected_stdout: &[u8]) {
    let directory = TestDirectory::create();
    let object = directory.join("remove_at_abort.o");
    let executable = directory.join("remove_at_abort");
    emit_native_unit_object(
        &analysis.sources,
        &analysis.inputs(),
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
        &object,
    )
    .expect("source removeAt emits a verified native object");
    crate::test_support::assert_native_object(&fs::read(&object).unwrap());
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("native link runs");
    assert_diagnostic!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let output = Command::new(&executable)
        .output()
        .expect("native program runs");
    assert!(!output.status.success(), "expected abort on out of bounds");
    assert_eq!(output.stdout, expected_stdout);
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq_diagnostic!(output.status.signal(), Some(6), "{output:?}");
    }
}
}

pub(crate) mod string_clone_tests {
use super::super::*;
use crate::test_support::diagnostics::assert_text_eq;

#[test]
fn string_clone_cross_file_borrow_container_and_return_reach_native() {
    let analysis = analyze_sources(
        "package p\nfun duplicate(text: String): String = text.clone()\nfun take(own text: String): Unit { println(text) }",
        r#"package q
        fun entry(): Unit {
            val source = "界\0" + "é"
            val copy = p.duplicate(source)
            p.take(copy)
            println(source)
            val texts = listOf(source, "tail")
            val item = texts[0].clone()
            println(texts[0])
            p.take(item)
            println("".clone())
            println("static".clone())
        }"#,
    );
    let inputs = analysis.inputs();
    let directory = TestDirectory::create();
    let object = directory.join("clone.o");
    let executable = directory.join("clone");
    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        NativeUnitEntry::NoArguments(analysis.declaration("q", "entry")),
        &object,
    )
    .expect("unit clone emits native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable).output().unwrap();
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        "界\0é\n界\0é\n界\0é\n界\0é\n\nstatic\n".as_bytes()
    );
}

#[test]
fn string_clone_unit_borrows_rc_payload() {
    let analysis = analyze_sources(
        "package p\nfun duplicate(text: String): String = text.clone()",
        r#"package q
        fun entry(): Unit { val shared = Rc("shared" + "text"); println(shared.value.clone()); println(shared.value) }
    "#,
    );
    let inputs = analysis.inputs();
    let (program, entry) = lower_scalar_unit_with_entry(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
    )
    .expect("Rc payload is a String place");
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let directory = TestDirectory::create();
    let ir = directory.join("rc-clone.ll");
    let executable = directory.join("rc-clone");
    fs::write(&ir, llvm).unwrap();
    let linked = Command::new(crate::test_support::ir_clang())
        .arg(&ir)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable).output().unwrap();
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"sharedtext\nsharedtext\n");
}

#[test]
fn string_clone_unit_borrows_named_fields_without_moving_them() {
    let analysis = analyze_sources(
        r#"package p
class Holder(val text: String)
fun duplicate(holder: Holder): String = holder.text.clone()"#,
        r#"package q
import p.Holder
        fun entry(): Unit { val holder = Holder("field" + "text"); println(p.duplicate(holder)); println(holder.text.clone()) }
    "#,
    );
    let inputs = analysis.inputs();
    let (program, entry) = lower_scalar_unit_with_entry(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
    )
    .expect("String fields are shared borrowed places");
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let directory = TestDirectory::create();
    let ir = directory.join("field-clone.ll");
    let executable = directory.join("field-clone");
    fs::write(&ir, llvm).unwrap();
    let linked = Command::new(crate::test_support::ir_clang())
        .arg(&ir)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable).output().unwrap();
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"fieldtext\nfieldtext\n");
}

#[test]
fn string_clone_unit_borrowed_rc_payload_is_rejected_before_native_emission() {
    let analysis = analyze_sources(
        "package p\nfun duplicate(shared: Rc<String>): String = shared.value.clone()",
        r#"package q
        fun entry(): Unit { val shared = Rc("text"); println(p.duplicate(shared)) }
    "#,
    );
    let inputs = analysis.inputs();
    let error = lower_scalar_unit_with_entry(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
    )
    .err()
    .expect("Borrow Rc payload must not reinterpret a handle-slot pointer as a control block");
    assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
}

#[test]
fn integrated_numeric_clone_and_boxed_enum_cross_file_facts_reach_native() {
    let analysis = analyze_sources(
        r#"package p
        enum class Text { Value(text: String), Empty }
        fun boxed(text: String): Box<Text> {
            val value = Text.Value(text.clone())
            return Box(value)
        }
        fun consume(own value: Box<Text>): Unit { println("boxed") }
        "#,
        r#"package q
        fun entry(): Unit {
            val texts = listOf("unused", "界" + "é")
            p.consume(p.boxed(texts[0b0_1]))
            println(texts[0x0_1].clone())
            println(texts[1])
        }
        "#,
    );
    let inputs = analysis.inputs();
    let lower = |inputs: &[_]| {
        lower_scalar_unit_with_entry(
            &analysis.sources,
            inputs,
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
        )
        .expect("cross-file numeric clone and boxed enum facts compose")
    };
    let (program, entry) = lower(&inputs);
    let (reversed, reversed_entry) = lower(&[inputs[1], inputs[0]]);
    assert_text_eq!(
        crate::ssa::render_program(&program),
        crate::ssa::render_program(&reversed)
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    assert_eq!(
        llvm,
        crate::llvm::render_verified_program_with_entry(&reversed, reversed_entry).unwrap()
    );
    let output = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 5);
    crate::native_tests::boxed_enum_tests::assert_success(&output, "boxed\n界é\n界é\n".as_bytes());
}
}
