//! SPEC-0274: native source length and temporary resource cleanup.
use super::*;

#[path = "unit_generic_container_tests.rs"]
mod generic_containers;

#[test]
fn unit_container_size_native_covers_empty_nonempty_and_reuses_owners() {
    let analysis = analyze_sources(
        r#"package p
        fun a(items: Array<Int>): Int = ((items)).size
        fun b(items: List<Int>): Int = ((items)).size
        fun c(items: MutableList<Int>): Int = ((items)).size
        fun take(own items: Array<Int>): Int = ((items)).size
        fun takeList(own items: List<Int>): Int = ((items)).size
        fun takeMutable(own items: MutableList<Int>): Int = ((items)).size
        fun emptyArray(): Array<Int> = arrayOf()
        fun emptyList(): List<Int> = listOf()
        fun emptyMutable(): MutableList<Int> = mutableListOf()"#,
        r#"package q
        fun entry(): Unit {
            val a0: Array<Int> = arrayOf()
            val a2 = arrayOf(1, 2)
            val b0: List<Int> = listOf()
            val b2 = listOf(1, 2)
            val c0: MutableList<Int> = mutableListOf()
            val c2 = mutableListOf(1, 2)
            if (((a0)).size != 0) { error("array empty") }
            if (((a2)).size != 2) { error("array nonempty") }
            if (((b0)).size != 0) { error("list empty") }
            if (((b2)).size != 2) { error("list nonempty") }
            if (((c0)).size != 0) { error("mutable empty") }
            if (((c2)).size != 2) { error("mutable nonempty") }
            if (p.a(a0) != 0) { error("borrow array empty") }
            if (p.a(a2) != 2) { error("borrow array nonempty") }
            if (p.b(b0) != 0) { error("borrow list empty") }
            if (p.b(b2) != 2) { error("borrow list nonempty") }
            if (p.c(c0) != 0) { error("borrow mutable empty") }
            if (p.c(c2) != 2) { error("borrow mutable nonempty") }
            if (p.take(a0) != 0) { error("empty array Value") }
            if (p.takeList(b0) != 0) { error("empty list Value") }
            if (p.takeMutable(c0) != 0) { error("empty mutable Value") }
            if (p.a(a2) != 2) { error("repeat borrow") }
            if (a2[0] != 1) { error("source reuse") }
            if (p.take(a2) != 2) { error("owner move after reads") }
            if (p.b(b2) != 2) { error("repeat list borrow") }
            if (b2[0] != 1) { error("list source reuse") }
            if (p.takeList(b2) != 2) { error("list owner move after reads") }
            if (p.c(c2) != 2) { error("repeat mutable borrow") }
            if (c2[0] != 1) { error("mutable source reuse") }
            if (p.takeMutable(c2) != 2) { error("mutable owner move after reads") }
            if (((p.emptyArray())).size != 0) { error("empty array temporary") }
            if (((p.emptyList())).size != 0) { error("empty list temporary") }
            if (((p.emptyMutable())).size != 0) { error("empty mutable temporary") }
            println("lengths-ok")
        }"#,
    );
    run(&analysis, b"lengths-ok\n");
}

#[test]
fn unit_container_size_native_temporary_resources_drop_once_after_header_read() {
    let analysis = analyze_sources(
        r#"package p
        class Resource(val name: String) { deinit() { println(this.name) } }
        fun sourceArray(): Array<Resource> {
            println("make-array")
            return arrayOf(Resource("a-one"), Resource("a-two"))
        }
        fun sourceList(): List<Resource> {
            println("make-list")
            return listOf(Resource("l-one"), Resource("l-two"))
        }
        fun sourceMutable(): MutableList<Resource> {
            println("make-mutable")
            return mutableListOf(Resource("m-one"), Resource("m-two"))
        }"#,
        r#"package q
        fun entry(): Unit {
            if (((p.sourceArray())).size != 2) { error("array temporary length") }
            println("after-array")
            if (((p.sourceList())).size != 2) { error("list temporary length") }
            println("after-list")
            if (((p.sourceMutable())).size != 2) { error("mutable temporary length") }
            println("after-mutable")
        }"#,
    );
    let expected = b"make-array\na-two\na-one\nafter-array\nmake-list\nl-two\nl-one\nafter-list\nmake-mutable\nm-two\nm-one\nafter-mutable\n";
    run(&analysis, expected);
    let (program, entry) = lower_scalar_unit_with_entry(
        &analysis.sources,
        &analysis.inputs(),
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
    )
    .expect("temporary resource containers lower with exact cleanup");
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    // Each temporary owns two Resource allocations and one container buffer.
    let counted = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 9);
    crate::native_tests::boxed_enum_tests::assert_success(&counted, expected);
}

#[test]
fn unit_container_size_native_borrow_survives_cfg_and_reuse() {
    let analysis = analyze_sources(
        r#"package p
        fun arraySize(items: Array<Int>, own flag: Boolean): Int {
            if (flag) { println("true") } else { println("false") }
            return ((items)).size
        }
        fun listSize(items: List<Int>, own flag: Boolean): Int {
            if (flag) { println("true") } else { println("false") }
            return ((items)).size
        }
        fun mutableSize(items: MutableList<Int>, own flag: Boolean): Int {
            if (flag) { println("true") } else { println("false") }
            return ((items)).size
        }"#,
        r#"package q
        fun entry(): Unit {
            val a = arrayOf(1, 2)
            val b = listOf(1, 2)
            val c = mutableListOf(1, 2)
            if (p.arraySize(a, true) != 2) { error("CFG array true") }
            if (p.arraySize(a, false) != 2) { error("CFG array false") }
            if (p.listSize(b, true) != 2) { error("CFG list true") }
            if (p.listSize(b, false) != 2) { error("CFG list false") }
            if (p.mutableSize(c, true) != 2) { error("CFG mutable true") }
            if (p.mutableSize(c, false) != 2) { error("CFG mutable false") }
            if (a[0] != 1) { error("array after CFG") }
            if (b[0] != 1) { error("list after CFG") }
            if (c[0] != 1) { error("mutable after CFG") }
            println("cfg-lengths-ok")
        }"#,
    );
    run(
        &analysis,
        b"true\nfalse\ntrue\nfalse\ntrue\nfalse\ncfg-lengths-ok\n",
    );
}

#[test]
fn unit_container_size_native_assignment_rhs_keeps_old_owner_until_replacement() {
    let analysis = analyze_sources(
        r#"package p
        class Resource(val name: String) { deinit() { println(this.name) } }
        fun make(own length: Int): Resource {
            if (length != 1) { error("RHS reads the old container") }
            println("rhs")
            return Resource("new")
        }
        fun finish(items: List<Resource>): Unit { println("finish") }"#,
        r#"package q
        import p.Resource
        fun entry(): Unit {
            var values = listOf(Resource("old"))
            values = listOf(p.make(((values)).size))
            println("after")
            p.finish(values)
        }"#,
    );
    run(&analysis, b"rhs\nold\nafter\nfinish\nnew\n");
}

fn run(analysis: &UnitAnalysis, expected: &[u8]) {
    let directory = TestDirectory::create();
    let object = directory.join("length.o");
    let executable = directory.join("length");
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
    .expect("source size emits a verified native object");
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
