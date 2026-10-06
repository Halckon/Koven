//! SPEC-0282: native MutableList.removeAt element removal and memory shift.

use super::*;

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
    assert!(
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
        assert_eq!(output.status.signal(), Some(6), "{output:?}");
    }
}
