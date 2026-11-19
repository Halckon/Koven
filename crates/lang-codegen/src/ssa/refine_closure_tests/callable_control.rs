//! Callable control-flow joins, reassignment, and borrowed view arguments.
use super::{lower, run_counted, try_lower};

#[test]
fn temporary_iteration_keeps_callable_only_routes() {
    let source = r#"fun repeat(action: () -> Unit): Unit {
        for (_ in listOf(1)) { action() }
    }
    fun main(): Unit {
        val run: () -> Unit = { for (item in listOf("kept")) { println(item) } }
        repeat(run)
    }"#;
    let (program, routed, entry) = lower(source);
    assert!(
        routed
            .iter()
            .any(|name| name.starts_with("koven.p.repeat.d"))
    );
    assert!(routed.iter().any(|name| name.ends_with(".thunk")));
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = crate::native_tests::emit_link_and_run("callable_iteration.ko", source, "main");
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"kept\n");
}

#[test]
fn temporary_factory_callee_without_cleanup_is_rejected() {
    let source = r#"class Data(val v: Int)
    fun make(own d: Data): (Int) -> Int = move { n -> n + d.v }
    fun main(): Unit { val result = make(Data(10))(4); if (result == 14) { println("ok") } }"#;
    let Err(error) = try_lower(source) else {
        panic!("direct factory callee needs frontend cleanup facts");
    };
    assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
}

#[test]
fn callable_if_when_join_and_local_reassignment_lower_in_refine() {
    let source = r#"class Data(val v: Int)
    fun make(own d: Data): (Int) -> Int = move { n -> n + d.v }
    fun pick(flag: Boolean): Int {
        val f: (Int) -> Int = if (flag) make(Data(3)) else make(Data(7))
        val g: (Int) -> Int = when {
            flag -> ({ n -> n + 1 })
            else -> ({ n -> n + 2 })
        }
        return f(10) + g(10)
    }
    fun main(): Unit {
        var h: (Int) -> Int = make(Data(4))
        if (h(1) == 5) { println("h1") }
        h = make(Data(9))
        if (h(1) == 10) { println("h2") }
        if (pick(true) == 24) { println("p1") }
        if (pick(false) == 29) { println("p2") }
    }"#;
    let (program, routed, entry) = lower(source);
    for function in ["koven.p.main.d", "koven.p.pick.d", "koven.p.make.d"] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    run_counted(&program, entry, 4, b"h1\nh2\np1\np2\n");
}

#[test]
fn callable_invocation_preserves_evaluated_callee_across_arguments() {
    let source = r#"class Data(val v: Int)
    fun make(own d: Data): (Int) -> Int = move { n -> n + d.v }
    fun arg1(): Int = 4
    fun arg2(): Int = 6
    fun main(): Unit {
        val f = make(Data(10))
        val r = f(arg1())
        if (r == 14) { println("call") }
        val g: (Int) -> Int = if (true) make(Data(6)) else make(Data(8))
        val direct = g(arg2())
        if (direct == 12) { println("direct") }
    }"#;
    let (program, routed, entry) = lower(source);
    assert!(
        routed.iter().any(|name| name.starts_with("koven.p.main.d")),
        "{routed:?}"
    );
    run_counted(&program, entry, 2, b"call\ndirect\n");
}

#[test]
fn callable_while_loop_reassignment_and_view_argument_lower_in_refine() {
    let source = r#"class Data(val v: Int)
    class Box(var v: Int)
    fun make(own d: Data): (Int) -> Int = move { n -> n + d.v }
    fun apply(f: (Int) -> Int, x: Int): Int = f(x)
    fun main(): Unit {
        var h: (Int) -> Int = make(Data(1))
        var fp: (Int) -> Int = { n -> n + 1 }
        val box_v = Box(10)
        val outer_shared: () -> Int = { box_v.v }
        var i = 0
        var sum = 0
        while (i < 4) {
            if (i == 2) {
                h = make(Data(10))
                fp = { n -> n + 5 }
                i += 1
                continue
            }
            val inner_shared: () -> Int = { box_v.v + i }
            sum += h(i) + fp(i) + outer_shared() + inner_shared()
            i += 1
        }
        val via_view = apply(if (true) make(Data(3)) else make(Data(7)), 4)
        if (sum == 91) { println("loop") }
        if (via_view == 7) { println("view") }
    }"#;
    let (program, routed, entry) = lower(source);
    for function in ["koven.p.main.d", "koven.p.make.d", "koven.p.apply.d"] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    run_counted(&program, entry, 4, b"loop\nview\n");
}

#[test]
fn callable_when_and_block_factory_joins_as_view_arguments_lower_in_refine() {
    let source = r#"class Data(val v: Int)
    fun make(own d: Data): (Int) -> Int = move { n -> n + d.v }
    fun apply(f: (Int) -> Int, x: Int): Int = f(x)
    fun main(): Unit {
        val a = apply(if (false) { make(Data(1)) } else { make(Data(2)) }, 5)
        val b = apply(when {
            false -> { make(Data(3)) }
            else -> make(Data(4))
        }, 5)
        if (a == 7 && b == 9) { println("ok") }
    }"#;
    let (program, routed, entry) = lower(source);
    assert!(routed.iter().any(|n| n.starts_with("koven.p.main.d")));
    run_counted(&program, entry, 2, b"ok\n");
}
