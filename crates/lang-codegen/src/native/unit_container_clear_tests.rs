//! SPEC-0281: native MutableList.clear element reverse cleanup and buffer reuse.

use super::*;

#[test]
fn unit_container_clear_native_primitive_and_reappend() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf(1, 2, 3)
            if (list.size != 3) { error("initial size mismatch") }
            list.clear()
            if (list.size != 0) { error("size after clear mismatch") }
            list.add(10)
            list.add(20)
            if (list.size != 2) { error("size after re-add mismatch") }
            if (list[0] != 10) { error("elem 0 mismatch") }
            if (list[1] != 20) { error("elem 1 mismatch") }
            println("clear-primitive-readd-ok")
        }"#,
    );
    run(&analysis, b"clear-primitive-readd-ok\n");
}

#[test]
fn unit_container_clear_native_empty_list() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf<Int>()
            if (list.size != 0) { error("empty list size mismatch") }
            list.clear()
            if (list.size != 0) { error("empty list size after clear mismatch") }
            list.add(42)
            if (list.size != 1) { error("size after add mismatch") }
            if (list[0] != 42) { error("elem 0 mismatch") }
            println("clear-empty-ok")
        }"#,
    );
    run(&analysis, b"clear-empty-ok\n");
}

#[test]
fn unit_container_clear_native_move_only_elements_reverse_deinit() {
    let analysis = analyze_sources(
        r#"package p
        class Resource(val name: String) {
            deinit() {
                println(this.name)
            }
        }"#,
        r#"package q
        import p.Resource
        fun clearAndAdd(): Unit {
            var list = mutableListOf<Resource>()
            list.add(Resource("r1"))
            list.add(Resource("r2"))
            list.add(Resource("r3"))
            println("before-clear")
            list.clear()
            println("after-clear")
            if (list.size != 0) { error("size after clear mismatch") }
            list.add(Resource("r4"))
            list.add(Resource("r5"))
            println("after-readd")
        }
        fun entry(): Unit {
            clearAndAdd()
            println("done-all")
        }"#,
    );
    run(
        &analysis,
        b"before-clear\nr3\nr2\nr1\nafter-clear\nafter-readd\nr5\nr4\ndone-all\n",
    );
}

#[test]
fn unit_container_clear_native_multiple_cycles_buffer_reuse() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf<Int>()
            list.add(1)
            list.add(2)
            list.add(3)
            list.add(4)
            list.clear()
            if (list.size != 0) { error("cycle 1 clear mismatch") }
            list.add(100)
            list.add(200)
            if (list[0] != 100) { error("cycle 2 elem 0 mismatch") }
            if (list[1] != 200) { error("cycle 2 elem 1 mismatch") }
            list.clear()
            if (list.size != 0) { error("cycle 2 clear mismatch") }
            list.add(999)
            if (list[0] != 999) { error("cycle 3 elem 0 mismatch") }
            println("clear-cycles-ok")
        }"#,
    );
    run(&analysis, b"clear-cycles-ok\n");
}

fn run(analysis: &UnitAnalysis, expected: &[u8]) {
    let directory = TestDirectory::create();
    let object = directory.join("clear.o");
    let executable = directory.join("clear");
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
    .expect("source clear emits a verified native object");
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
