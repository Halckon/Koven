//! SPEC-0283: native MutableList.removeLast tail removal and empty list abort.

use super::*;

#[test]
fn unit_container_remove_last_native_primitive_sequential() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf(10, 20, 30)
            if (list.size != 3) { error("initial size mismatch") }

            val last1 = list.removeLast()
            if (last1 != 30) { error("first removed mismatch") }
            if (list.size != 2) { error("size after removeLast 1 mismatch") }
            if (list[0] != 10) { error("elem 0 mismatch") }
            if (list[1] != 20) { error("elem 1 mismatch") }

            val last2 = list.removeLast()
            if (last2 != 20) { error("second removed mismatch") }
            if (list.size != 1) { error("size after removeLast 2 mismatch") }
            if (list[0] != 10) { error("elem 0 mismatch") }

            val last3 = list.removeLast()
            if (last3 != 10) { error("third removed mismatch") }
            if (list.size != 0) { error("size after removeLast 3 mismatch") }

            list.add(99)
            if (list.size != 1) { error("size after add mismatch") }
            if (list[0] != 99) { error("elem 0 mismatch") }

            println("remove-last-primitive-ok")
        }"#,
    );
    run(&analysis, b"remove-last-primitive-ok\n");
}

#[test]
fn unit_container_remove_last_native_move_only_elements_lifecycle() {
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
            println("before-remove-last")
            consume(list.removeLast())
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
        b"before-remove-last\nconsuming\nr2\nr2\nafter-consume\nr1\nr0\ndone-all\n",
    );
}

#[test]
fn unit_container_remove_last_native_empty_list_aborts() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf<Int>()
            println("before-empty-remove-last")
            val x = list.removeLast()
            println("unexpected-reach")
        }"#,
    );
    run_abort(&analysis, b"before-empty-remove-last\n");
}

#[test]
fn unit_container_remove_last_native_pop_and_empty_abort() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf(42)
            val popped = list.removeLast()
            if (popped != 42) { error("popped mismatch") }
            println("popped-first")
            val x = list.removeLast()
            println("unexpected-reach")
        }"#,
    );
    run_abort(&analysis, b"popped-first\n");
}

fn run(analysis: &UnitAnalysis, expected: &[u8]) {
    let directory = TestDirectory::create();
    let object = directory.join("remove_last.o");
    let executable = directory.join("remove_last");
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
    .expect("source removeLast emits a verified native object");
    crate::test_support::assert_native_object(&fs::read(&object).unwrap());
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("native link runs");
    assert!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let output = Command::new(&executable)
        .output()
        .expect("native program runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.is_empty());
}

fn run_abort(analysis: &UnitAnalysis, expected_stdout: &[u8]) {
    let directory = TestDirectory::create();
    let object = directory.join("remove_last_abort.o");
    let executable = directory.join("remove_last_abort");
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
    .expect("source removeLast emits a verified native object");
    crate::test_support::assert_native_object(&fs::read(&object).unwrap());
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("native link runs");
    assert!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let output = Command::new(&executable)
        .output()
        .expect("native program runs");
    assert!(
        !output.status.success(),
        "expected abort on empty removeLast"
    );
    assert_eq!(output.stdout, expected_stdout);
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(output.status.signal(), Some(6), "{output:?}");
    }
}
