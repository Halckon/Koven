//! SPEC-0285: native MutableList.insertAt element insertion and memory shift.

use super::*;

#[test]
fn unit_container_insert_at_native_primitive_sequential() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf<Int>()
            if (list.size != 0) { error("initial size mismatch") }

            list.insertAt(0, 100)
            if (list.size != 1) { error("size after first insert mismatch") }
            if (list[0] != 100) { error("elem 0 mismatch") }

            list.insertAt(0, 50)
            if (list.size != 2) { error("size after prepend mismatch") }
            if (list[0] != 50) { error("elem 0 prepend mismatch") }
            if (list[1] != 100) { error("elem 1 prepend mismatch") }

            list.insertAt(2, 200)
            if (list.size != 3) { error("size after append mismatch") }
            if (list[0] != 50) { error("elem 0 append mismatch") }
            if (list[1] != 100) { error("elem 1 append mismatch") }
            if (list[2] != 200) { error("elem 2 append mismatch") }

            list.insertAt(1, 75)
            if (list.size != 4) { error("size after mid insert mismatch") }
            if (list[0] != 50) { error("elem 0 mid mismatch") }
            if (list[1] != 75) { error("elem 1 mid mismatch") }
            if (list[2] != 100) { error("elem 2 mid mismatch") }
            if (list[3] != 200) { error("elem 3 mid mismatch") }

            list.insertAt(4, 300)
            list.insertAt(0, 25)
            if (list.size != 6) { error("size after growth mismatch") }
            if (list[0] != 25) { error("elem 0 after growth mismatch") }
            if (list[1] != 50) { error("elem 1 after growth mismatch") }
            if (list[2] != 75) { error("elem 2 after growth mismatch") }
            if (list[3] != 100) { error("elem 3 after growth mismatch") }
            if (list[4] != 200) { error("elem 4 after growth mismatch") }
            if (list[5] != 300) { error("elem 5 after growth mismatch") }

            println("insert-at-primitive-ok")
        }"#,
    );
    run(&analysis, b"insert-at-primitive-ok\n");
}

#[test]
fn unit_container_insert_at_native_move_only_elements_lifecycle() {
    let analysis = analyze_sources(
        r#"package p
        class Resource(val name: String) {
            deinit() {
                println(this.name)
            }
        }"#,
        r#"package q
        import p.Resource
        fun runTest(): Unit {
            var list = mutableListOf<Resource>()
            list.add(Resource("r0"))
            list.add(Resource("r2"))
            println("before-insert")
            list.insertAt(1, Resource("r1"))
            println("after-insert")
            if (list.size != 3) { error("size mismatch") }
        }
        fun entry(): Unit {
            runTest()
            println("done-all")
        }"#,
    );
    run(
        &analysis,
        b"before-insert\nafter-insert\nr2\nr1\nr0\ndone-all\n",
    );
}

#[test]
fn unit_container_insert_at_native_out_of_bounds_negative_aborts() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf(1, 2)
            println("before-negative-insert")
            list.insertAt(-1, 99)
            println("unexpected-reach")
        }"#,
    );
    run_abort(&analysis, b"before-negative-insert\n");
}

#[test]
fn unit_container_insert_at_native_out_of_bounds_beyond_size_aborts() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf(1, 2)
            println("before-beyond-insert")
            list.insertAt(3, 99)
            println("unexpected-reach")
        }"#,
    );
    run_abort(&analysis, b"before-beyond-insert\n");
}

fn run(analysis: &UnitAnalysis, expected: &[u8]) {
    let directory = TestDirectory::create();
    let object = directory.join("insert_at.o");
    let executable = directory.join("insert_at");
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
    .expect("source insertAt emits a verified native object");
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
}

fn run_abort(analysis: &UnitAnalysis, expected_prefix: &[u8]) {
    let directory = TestDirectory::create();
    let object = directory.join("insert_at_abort.o");
    let executable = directory.join("insert_at_abort");
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
    .expect("source insertAt abort emits a verified native object");
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
    assert!(!output.status.success());
    assert!(
        output.stdout.starts_with(expected_prefix),
        "stdout was: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}
