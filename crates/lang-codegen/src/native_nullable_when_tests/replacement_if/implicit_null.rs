use super::*;

#[test]
fn nullable_when_replacement_if_accepts_an_implicit_empty_null_arm() {
    let quiet = NOT_EQUAL.replace("{ observe((x)) } else {}", "{ observe((x)) }");
    assert_complete(
        "replacement-if-implicit-quiet.ko",
        &quiet,
        Shape {
            implicit_null: true,
            ..SHAPES[1]
        },
    );
    let text = NATIVE.replace(
        "if (((x) != null)) { observe((x)) } else {}",
        "if (((x) != null)) { observe((x)) }",
    );
    assert_ne!(text, NATIVE, "the explicit null arm must be removed");
    let source = checked("replacement-if-implicit-else.ko", &text);
    let declaration = source.declaration("entry", SymbolKind::Function);
    let (result, routed) = crate::ssa::observe_routing(|| source.lower_entry(declaration));
    let (program, _) = result.expect("the implicit empty null arm must lower");
    let helper = function(&source, &program, "notEqual");
    assert!(!routed.contains(&helper.name));
    verify_program(&program).expect("actual Unit SSA verifies");
    let output =
        crate::native_tests::emit_link_and_run("replacement-if-implicit-else.ko", &text, "entry");
    crate::native_tests::boxed_enum_tests::assert_success(&output, NATIVE_STDOUT);
}

const LIVE_GUARD: &str = r#"class Node {}
class Guard { deinit() { println("drop-guard") } }
fun observe(node: Node): Unit { println("observe") }
fun keep(guard: Guard): Unit { println("keep-guard") }
fun run(own input: Node?, own replacement: Node?): Unit {
    var x = input
    when (x) {
        null -> { println("old-null") }
        else -> {
            println("replace")
            x = replacement
            val guard = Guard()
            if (x != null) { observe(x) }
            else { println("replacement-null"); return }
            keep(guard)
            println("after-if")
        }
    }
    println("after-when")
}
fun entry(): Unit {
    val old: Node? = Node()
    run(old, null)
    println("caller-after-return")
    val other: Node? = Node()
    val new: Node? = Node()
    run(other, new)
    println("caller-after-normal")
}
"#;

#[test]
fn nullable_when_replacement_if_runs_the_null_arm_before_return() {
    let expected = concat!(
        "replace\nreplacement-null\ndrop-guard\ncaller-after-return\n",
        "replace\nobserve\nkeep-guard\nafter-if\ndrop-guard\nafter-when\ncaller-after-normal\n",
    );
    let nested_write = LIVE_GUARD.replace(
        r#"else { println("replacement-null"); return }"#,
        r#"else { if (true) { x = Node(); println("replacement-null"); return } }"#,
    );
    for (path, text) in [
        ("replacement-if-null-return.ko", LIVE_GUARD),
        ("replacement-if-inner-write-return.ko", &nested_write),
    ] {
        let source = checked(path, text);
        let entry = source.declaration("entry", SymbolKind::Function);
        let (result, routed) = crate::ssa::observe_routing(|| source.lower_entry(entry));
        let (program, _) = result.expect("the null arm must lower");
        assert!(!routed.contains(&function(&source, &program, "run").name));
        verify_program(&program).expect("actual Unit SSA verifies");

        let output = crate::native_tests::emit_link_and_run(path, text, "entry");
        crate::native_tests::boxed_enum_tests::assert_success(&output, expected.as_bytes());
    }
}

#[test]
fn nullable_when_replacement_if_aborts_without_dropping_the_live_guard() {
    use std::os::unix::process::ExitStatusExt;

    let text = LIVE_GUARD.replace(
        "println(\"replacement-null\"); return",
        "println(\"replacement-null\"); error(\"nullable-stop\")",
    );
    const PATH: &str = "replacement-if-null-abort.ko";
    let source = checked(PATH, &text);
    let entry = source.declaration("entry", SymbolKind::Function);
    let (result, routed) = crate::ssa::observe_routing(|| source.lower_entry(entry));
    let (program, _) = result.expect("the nullable Abort arm must lower");
    let helper = function(&source, &program, "run");
    assert!(!routed.contains(&helper.name));
    assert!(helper.blocks.iter().any(|block| {
        block
            .terminator
            .as_ref()
            .is_some_and(|term| matches!(term.kind, TerminatorKind::Abort))
    }));
    verify_program(&program).expect("actual Unit SSA verifies");

    let output = crate::native_tests::emit_link_and_run(PATH, &text, "entry");
    assert_eq!(output.status.signal(), Some(6), "{output:?}");
    assert_eq!(output.stdout, b"replace\nreplacement-null\n");
    assert_eq!(
        output.stderr,
        b"abort: nullable-stop\n  at replacement-if-null-abort.ko:14:49\n"
    );
}

#[test]
fn nullable_when_replaces_a_lexical_resource_before_inner_return() {
    let text = r#"class Resource(val label: String) { deinit() { println(label) } }
class Guard { deinit() { println("drop-guard") } }
fun make(value: Resource): Resource { println("rhs"); println(value.label); println("rhs-finished"); return Resource("drop-new") }
fun observe(value: Resource): Unit { println("observe") }
fun run(own input: Resource?, flag: Boolean): Unit {
    var x = input
    val guard = Guard()
    when (x) {
        null -> { println("old-null") }
        else -> {
            if (flag) {
                x = make(x)
                if (x != null) { observe(x) }
                println("returning")
                return
            }
        }
    }
    println("normal")
}
fun entry(): Unit {
    run(Resource("drop-old"), true)
    println("caller")
    run(Resource("drop-kept"), false)
    run(null, true)
}
"#;
    const PATH: &str = "resource-replacement-return.ko";
    let source = checked(PATH, text);
    let entry = source.declaration("entry", SymbolKind::Function);
    let (result, routed) = crate::ssa::observe_routing(|| source.lower_entry(entry));
    let (program, _) = result.expect("replacement retires the old proof and owner");
    assert!(!routed.contains(&function(&source, &program, "run").name));
    verify_program(&program).expect("actual Unit SSA verifies");

    let output = crate::native_tests::emit_link_and_run(PATH, text, "entry");
    let expected = concat!(
        "rhs\ndrop-old\nrhs-finished\ndrop-old\nobserve\nreturning\ndrop-guard\ndrop-new\ncaller\n",
        "normal\ndrop-guard\ndrop-kept\nold-null\nnormal\ndrop-guard\n",
    );
    crate::native_tests::boxed_enum_tests::assert_success(&output, expected.as_bytes());
}

#[test]
fn nullable_when_rechecks_an_inner_writeback_before_return_with_an_outer_borrow() {
    let text = r#"class Resource(val label: String) { deinit() { println(label) } }
fun make(value: Resource): Resource? { println("rhs"); println(value.label); println("rhs-finished"); return Resource("drop-new") }
fun seeOuter(value: Resource): Unit { println("outer"); println(value.label) }
fun seeInner(value: Resource): Unit { println("inner"); println(value.label) }
fun run(own outer: Resource?, own inner: Resource?, flag: Boolean): Unit {
    var y = inner
    when (outer) {
        null -> { println("outer-null") }
        else -> {
            when (y) {
                null -> { println("inner-null") }
                else -> {
                    if (flag) {
                        y = make(y)
                        if (y != null) { seeInner(y) }
                        seeOuter(outer)
                        println("returning")
                        return
                    }
                    seeInner(y)
                }
            }
            println("outer-end")
        }
    }
    println("normal")
}
fun entry(): Unit {
    run(Resource("drop-outer"), Resource("drop-inner"), true)
    println("caller")
    run(Resource("drop-outer"), Resource("drop-kept"), false)
    run(Resource("drop-outer"), null, true)
}
"#;
    const PATH: &str = "nested-resource-writeback-return.ko";
    let source = checked(PATH, text);
    let entry = source.declaration("entry", SymbolKind::Function);
    let (result, routed) = crate::ssa::observe_routing(|| source.lower_entry(entry));
    let (program, _) = result.expect("distinct nullable owners survive the inner writeback");
    assert!(!routed.contains(&function(&source, &program, "run").name));
    verify_program(&program).expect("actual Unit SSA verifies");

    let output = crate::native_tests::emit_link_and_run(PATH, text, "entry");
    let expected = concat!(
        "rhs\ndrop-inner\nrhs-finished\ndrop-inner\ninner\ndrop-new\nouter\ndrop-outer\nreturning\ndrop-new\ndrop-outer\ncaller\n",
        "inner\ndrop-kept\nouter-end\nnormal\ndrop-kept\ndrop-outer\n",
        "inner-null\nouter-end\nnormal\ndrop-outer\n",
    );
    crate::native_tests::boxed_enum_tests::assert_success(&output, expected.as_bytes());
}
