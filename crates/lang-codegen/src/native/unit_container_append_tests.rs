//! SPEC-0280: native MutableList.add sequential appending and dynamic capacity growth.

use super::*;

#[test]
fn unit_container_append_native_growth_and_ordering() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf<Int>()
            if (list.size != 0) { error("initial size not zero") }
            list.add(10)
            list.add(20)
            list.add(30)
            list.add(40)
            list.add(50)
            list.add(60)
            list.add(70)
            list.add(80)
            list.add(90)
            if (list.size != 9) { error("final size mismatch") }
            if (list[0] != 10) { error("element 0 mismatch") }
            if (list[1] != 20) { error("element 1 mismatch") }
            if (list[2] != 30) { error("element 2 mismatch") }
            if (list[3] != 40) { error("element 3 mismatch") }
            if (list[4] != 50) { error("element 4 mismatch") }
            if (list[5] != 60) { error("element 5 mismatch") }
            if (list[6] != 70) { error("element 6 mismatch") }
            if (list[7] != 80) { error("element 7 mismatch") }
            if (list[8] != 90) { error("element 8 mismatch") }
            println("append-growth-ok")
        }"#,
    );
    run(&analysis, b"append-growth-ok\n");
}

#[test]
fn unit_container_append_native_nonempty_start() {
    let analysis = analyze_sources(
        "package p\n",
        r#"package q
        fun entry(): Unit {
            var list = mutableListOf(1, 2, 3)
            if (list.size != 3) { error("start size mismatch") }
            list.add(4)
            list.add(5)
            if (list.size != 5) { error("size mismatch") }
            if (list[0] != 1) { error("elem 0") }
            if (list[1] != 2) { error("elem 1") }
            if (list[2] != 3) { error("elem 2") }
            if (list[3] != 4) { error("elem 3") }
            if (list[4] != 5) { error("elem 4") }
            println("nonempty-append-ok")
        }"#,
    );
    run(&analysis, b"nonempty-append-ok\n");
}

#[test]
fn unit_container_append_native_move_only_elements_deinit_in_reverse_order() {
    let analysis = analyze_sources(
        r#"package p
        class Resource(val name: String) {
            deinit() {
                println(this.name)
            }
        }"#,
        r#"package q
        import p.Resource
        fun buildAndGrow(): Unit {
            var list = mutableListOf<Resource>()
            list.add(Resource("r1"))
            list.add(Resource("r2"))
            list.add(Resource("r3"))
            list.add(Resource("r4"))
            list.add(Resource("r5"))
            println("done-append")
        }
        fun entry(): Unit {
            buildAndGrow()
            println("done-all")
        }"#,
    );
    run(&analysis, b"done-append\nr5\nr4\nr3\nr2\nr1\ndone-all\n");
}

fn run(analysis: &UnitAnalysis, expected: &[u8]) {
    let directory = TestDirectory::create();
    let object = directory.join("append.o");
    let executable = directory.join("append");
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
    .expect("source append emits a verified native object");
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
