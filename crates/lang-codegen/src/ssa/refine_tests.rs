//! SPEC-0350 refinement layer: inline nullable functions lower through slots and liveness.
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    model::{SsaTypeKind, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

fn run(source: &str) -> (String, Option<i32>) {
    let output = crate::native_tests::emit_link_and_run("refine.ko", source, "main");
    assert!(
        output.stderr.is_empty() || !output.status.success(),
        "{output:?}"
    );
    (
        String::from_utf8(output.stdout).expect("utf-8 stdout"),
        output.status.code(),
    )
}

/// Lowers a single-source unit twice, checks determinism and the inline nullable type,
/// and returns the rendered program.
fn render(source: &str) -> String {
    let (rendered, inline) = render_branching(source);
    assert!(inline, "{rendered}");
    rendered
}

/// Lowers twice, checks determinism and a `NullableBranch`; reports whether an inline
/// nullable type was interned.
fn render_branching(source: &str) -> (String, bool) {
    let mut sources = SourceMap::new();
    let text = format!("package p\n{source}");
    let (id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "p", "main");
    let lower = || {
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("refined program lowers and verifies")
    };
    let (program, _) = lower();
    let (again, _) = lower();
    let rendered = render_program(&program);
    assert_eq!(rendered, render_program(&again), "deterministic output");
    let module = &program.modules[0];
    let inline = module
        .types
        .iter()
        .any(|kind| matches!(kind, SsaTypeKind::InlineNullable { .. }));
    assert!(
        module
            .functions
            .iter()
            .any(|function| function.blocks.iter().any(|block| matches!(
                block.terminator.as_ref().map(|terminator| &terminator.kind),
                Some(TerminatorKind::NullableBranch { .. })
            ))),
        "{rendered}"
    );
    (rendered, inline)
}

#[test]
fn nullable_int_local_smart_cast_runs() {
    let source = r#"fun main(): Unit { val a: Int? = 5; if (a != null) { if (a == 5) { println("nint") } } }"#;
    render(source);
    assert_eq!(run(source), ("nint\n".to_owned(), Some(0)));
}

#[test]
fn nullable_int_while_reassignment_carries_loop_state() {
    let source = r#"fun main(): Unit {
        var x: Int? = 5
        var total = 0
        while (x != null) { total = total + x; x = null }
        if (total == 5) { println("while-nullable") }
    }"#;
    render(source);
    assert_eq!(run(source), ("while-nullable\n".to_owned(), Some(0)));
}

#[test]
fn nullable_join_return_and_short_circuit_proof() {
    let source = r#"fun pick(c: Boolean): Int? = if (c) 7 else null
    fun main(): Unit {
        val x = pick(true)
        if (x != null && x == 7) { println("join-nullable") }
        val y = pick(false)
        if (y == null || y == 0) { println("absent") }
    }"#;
    render(source);
    assert_eq!(run(source), ("join-nullable\nabsent\n".to_owned(), Some(0)));
}

#[test]
fn nullable_value_parameter_and_null_comparison_value() {
    let source = r#"fun describe(own n: Int?): Boolean { val present = n != null; return present }
    fun main(): Unit {
        if (describe(3)) { println("some") }
        if (!describe(null)) { println("none") }
    }"#;
    render(source);
    assert_eq!(run(source), ("some\nnone\n".to_owned(), Some(0)));
}

#[test]
fn checked_arithmetic_in_refined_function_aborts_with_message() {
    let source = r#"fun main(): Unit {
        var x: Int? = 2147483647
        if (x != null) { val y = x + 1; if (y == 0) { println("unreachable") } }
    }"#;
    let output = crate::native_tests::emit_link_and_run("refine.ko", source, "main");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("abort: integer overflow in +\n  at "),
        "{stderr:?}"
    );
}

fn counted(source: &str, allocations: usize, stdout: &[u8]) {
    let mut sources = SourceMap::new();
    let text = format!("package p\n{source}");
    let (id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "p", "main");
    let (program, entry) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("refined program lowers and verifies");
    let llvm =
        crate::llvm::render_verified_program_with_entry(&program, entry).expect("verified LLVM");
    let output = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, allocations);
    crate::native_tests::boxed_enum_tests::assert_success(&output, stdout);
}

#[test]
fn nullable_string_return_and_borrowed_view() {
    let source = r#"fun find(flag: Boolean): String? { if (flag) { return "hit" }; return null }
    fun main(): Unit { val s = find(true); if (s != null) { println(s) } }"#;
    render(source);
    assert_eq!(run(source), ("hit\n".to_owned(), Some(0)));
}

#[test]
fn nullable_string_owner_is_freed_once_on_both_edges() {
    let source = r#"fun find(flag: Boolean): String? { if (flag) { return "h" + "it" }; return null }
    fun main(): Unit {
        val s = find(true)
        if (s != null) { println(s) }
        val t = find(false)
        if (t != null) { println(t) } else { println("none") }
        val u = find(true)
        if (u != null && u == "hit") { println("equal") }
    }"#;
    render(source);
    counted(source, 2, b"hit\nnone\nequal\n");
}

const NODE: &str =
    "class Node(val v: Int)\n    fun pick(flag: Boolean): Node? = if (flag) Node(5) else null\n";

#[test]
fn class_nullable_view_field_read_and_bang_bang_free_each_owner_once() {
    let source = format!(
        r#"{NODE}fun main(): Unit {{
        val a = pick(true)
        if (a != null) {{ if (a.v == 5) {{ println("smart") }} }}
        val b = pick(true)!!
        if (b.v == 5) {{ println("bangbang") }}
        val c = pick(true)
        val d = c!!
        if (d.v == 5) {{ println("root") }}
    }}"#
    );
    let (rendered, inline) = render_branching(&source);
    assert!(
        !inline,
        "a pointer-like nullable needs no inline type: {rendered}"
    );
    assert!(rendered.contains("heap.field_read"), "{rendered}");
    counted(&source, 3, b"smart\nbangbang\nroot\n");
}

#[test]
fn bang_bang_on_null_aborts_at_the_assertion() {
    let source = format!(
        r#"{NODE}fun main(): Unit {{ val b = pick(false)!!; if (b.v == 5) {{ println("unreachable") }} }}"#
    );
    let output = crate::native_tests::emit_link_and_run("refine.ko", &source, "main");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("abort: null assertion failed\n  at "),
        "{stderr:?}"
    );
}

#[test]
fn checked_arithmetic_abort_edge_carries_live_owners() {
    let source = r#"fun main(): Unit {
        val s: String? = "a" + "b"
        var x: Int? = 2147483647
        if (x != null) { val y = x + 1; if (y == 0) { println("unreachable") } }
        if (s != null) { println(s) }
    }"#;
    render(source);
    let output = crate::native_tests::emit_link_and_run("refine.ko", source, "main");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("abort: integer overflow in +\n  at "),
        "{stderr:?}"
    );
}

#[test]
fn nullable_boolean_join_and_smart_cast_run() {
    let source = r#"fun flag(c: Boolean): Boolean? = if (c) false else null
    fun main(): Unit {
        val a = flag(true)
        if (a != null) { if (!a) { println("false") } }
        val b = flag(false)
        if (b == null) { println("none") }
    }"#;
    render(source);
    assert_eq!(run(source), ("false\nnone\n".to_owned(), Some(0)));
}

#[test]
fn subjectless_when_statement_keeps_the_proof_view_for_a_later_call() {
    let source = r#"class Node(val v: Int)
    fun outer(node: Node, own result: Int): Unit { if (node.v == result) { println("same") } }
    fun test(own subject: Node?, flag: Boolean): Unit {
        if (subject != null) {
            when { flag -> { println("flag") }; else -> {} }
            outer(subject, 5)
        }
    }
    fun main(): Unit { test(Node(5), true); test(Node(5), false); test(null, true) }"#;
    let (_, routed) = super::refine::observe_routing(|| render_branching(source));
    assert!(
        routed.iter().any(|name| name.starts_with("koven.p.test.d")),
        "{routed:?}"
    );
    counted(source, 2, b"flag\nsame\nsame\n");
}

#[test]
fn subjectless_when_value_tests_alternatives_in_order() {
    let source = r#"fun grade(n: Int?): String =
        if (n == null) "none" else when { n > 9, n < 0 -> "out"; else -> "digit" }
    fun note(x: Int?): Unit {
        if (x != null) { when { x == 1 -> println("one"); x > 1 -> println("many") } } else { println("absent") }
    }
    fun main(): Unit {
        println(grade(null)); println(grade(12)); println(grade(0 - 1)); println(grade(4))
        note(2); note(null); note(1); note(0)
    }"#;
    let (_, routed) = super::refine::observe_routing(|| render_branching(source));
    for function in ["koven.p.grade.d", "koven.p.note.d"] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    assert_eq!(
        run(source),
        (
            "none\nout\nout\ndigit\nmany\nabsent\none\n".to_owned(),
            Some(0)
        )
    );
}

#[test]
fn loop_break_and_continue_keep_the_outer_proof_view() {
    let source = r#"class Node(val v: Int)
    fun outer(node: Node, own result: Int): Unit { if (node.v == result) { println("same") } }
    fun test(own subject: Node?, n: Int): Unit {
        if (subject != null) {
            var i = 0
            loop { i = i + 1; if (i < n) continue else break }
            println("loop")
            while (i > 0) { i = i - 1; if (i == 1) { continue }; outer(subject, i) }
            outer(subject, 5)
        }
    }
    fun main(): Unit { test(Node(5), 3); test(null, 3) }"#;
    let (_, routed) = super::refine::observe_routing(|| render_branching(source));
    assert!(
        routed.iter().any(|name| name.starts_with("koven.p.test.d")),
        "{routed:?}"
    );
    counted(source, 1, b"loop\nsame\n");
}

#[test]
fn subject_when_compares_scalars_and_range_membership() {
    let source = r#"fun name(n: Int): String = when (n) { 1 -> "one"; 2 -> "two"; else -> "many" }
    fun bucket(x: Int): String = when (x) { in 0..9 -> "small"; !in 0..<100 -> "huge"; else -> "big" }
    fun main(): Unit {
        println(name(2)); println(name(5))
        println(bucket(3)); println(bucket(50)); println(bucket(200))
        val k = 7
        val n = when (k > 5) { false -> 1; else -> 2 }
        if (n == 2) { println("when-bool") }
        when (n) { 1 -> 1; else -> "s" }
        println("stmt")
    }"#;
    let ((program, _), routed) = super::refine::observe_routing(|| {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("subject when lowers and verifies")
    });
    let rendered = render_program(&program);
    // `in 0..9` tests `>=` then `<=`; `!in 0..<100` tests `>=` then `<`.
    for comparison in ["cmp.ge", "cmp.le", "cmp.lt"] {
        assert!(rendered.contains(comparison), "{comparison}: {rendered}");
    }
    for function in ["koven.p.name.d", "koven.p.bucket.d", "koven.p.main.d"] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    assert_eq!(
        run(source),
        (
            "two\nmany\nsmall\nbig\nhuge\nwhen-bool\nstmt\n".to_owned(),
            Some(0)
        )
    );
}

#[test]
fn enum_case_tests_and_payload_reads_run_in_methods_and_functions() {
    let source = r#"enum class Shape { Circle(r: Int), Square(s: Int), Point;
        fun area(): Int = when (this) { is Circle -> 3 * r * r; is Square -> s * s; is Point -> 0 }
        fun round(): Boolean = this is Circle
    }
    fun side(x: Shape): Int = when (x) { is Shape.Square -> x.s; !is Shape.Circle -> 100; else -> 0 }
    fun point(x: Shape): String = when (x) { Shape.Point -> "point"; else -> "shape" }
    fun main(): Unit {
        val c = Shape.Circle(2)
        val q = Shape.Square(3)
        val p = Shape.Point
        if (c.area() == 12 && q.area() == 9 && p.area() == 0) { println("area") }
        if (c.round() && !q.round()) { println("round") }
        if (side(q) == 3 && side(p) == 100 && side(c) == 0) { println("side") }
        println(point(p))
        println(point(c))
    }"#;
    let ((program, _), routed) = super::refine::observe_routing(|| {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("enum when lowers and verifies")
    });
    // The exhaustive value `when` over the enum has an unreachable fall-through abort.
    let area = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains(".area."))
        .expect("area is lowered");
    assert!(
        area.blocks.iter().any(|block| matches!(
            block.terminator.as_ref().and_then(|terminator| terminator.abort.as_ref()),
            Some(super::model::AbortReason::Text(text)) if text.contains("matched no branch")
        )),
        "{}",
        render_program(&program)
    );
    for function in ["area", "side", "point"] {
        assert!(
            routed
                .iter()
                .any(|name| name.contains(&format!(".{function}."))),
            "{function}: {routed:?}"
        );
    }
    assert_eq!(
        run(source),
        ("area\nround\nside\npoint\nshape\n".to_owned(), Some(0))
    );
}

#[test]
fn inout_scalar_parameters_replace_the_callers_binding() {
    let source = r#"fun bump(inout x: Int): Unit { x = x + 1 }
    fun add_to(inout total: Int, own amount: Int): Unit {
        total = total + amount
        if (total > 5) { total = 0 }
    }
    fun main(): Unit {
        var n = 1
        bump(&n)
        add_to(&n, 2)
        if (n == 4) { println("four") }
        add_to(&n, 3)
        if (n == 0) { println("reset") }
    }"#;
    let ((program, _), routed) = super::refine::observe_routing(|| {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("inout lowers and verifies")
    });
    let rendered = render_program(&program);
    assert!(rendered.contains("borrow.exclusive"), "{rendered}");
    for function in ["koven.p.bump.d", "koven.p.add_to.d", "koven.p.main.d"] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    assert_eq!(run(source), ("four\nreset\n".to_owned(), Some(0)));
}

#[test]
fn field_replacement_drops_the_old_value_and_keeps_the_owner() {
    let source = r#"class Counter(var count: Int, var name: String)
    fun main(): Unit {
        var c = Counter(0, "a" + "b")
        c.count = 5
        c.name = "x" + "y"
        if (c.count == 5) { println("field-assign") }
    }"#;
    let (_, routed) = super::refine::observe_routing(|| {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("field replacement lowers and verifies")
    });
    assert!(
        routed.iter().any(|name| name.starts_with("koven.p.main.d")),
        "{routed:?}"
    );
    // The Counter and both concatenations are each freed exactly once.
    counted(source, 3, b"field-assign\n");
}

#[test]
fn inout_class_parameters_and_receivers_mutate_the_callers_owner() {
    let source = r#"class Counter(var n: Int) {
        inout fun inc(): Unit { this.n = this.n + 1 }
        inout fun add(own k: Int): Unit { n = n + k }
        fun get(): Int = n
    }
    fun bump(inout c: Counter): Unit { c.inc(); c.add(10) }
    fun main(): Unit {
        var c = Counter(0)
        bump(&c)
        if (c.n == 11 && c.get() == 11) { println("inout-class") }
        c.inc()
        if (c.get() == 12) { println("receiver") }
    }"#;
    let (_, routed) = super::refine::observe_routing(|| {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("inout class lowers and verifies")
    });
    // `get` touches no migrated family and keeps the per-family path.
    for function in ["koven.p.bump.d", "koven.p.main.d", ".inc.", ".add."] {
        assert!(
            routed.iter().any(|name| name.contains(function)),
            "{function}: {routed:?}"
        );
    }
    // One Counter, freed once.
    counted(source, 1, b"inout-class\nreceiver\n");
}

#[test]
fn rc_construction_share_and_value_projections_release_once() {
    let source = r#"class Data(val n: Int)
    fun peek(own rc: Rc<Data>): Int = rc.value.n
    fun main(): Unit {
        val a = Rc(Data(4))
        val b = a.share()
        if (b.value.n == 4 && peek(a.share()) == 4) { println("rc") }
        val c = Rc(7)
        val d = c.share()
        if (d.value == 7) { println("copy") }
    }"#;
    let (_, routed) = super::refine::observe_routing(|| {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("Rc lowers and verifies")
    });
    for function in ["koven.p.main.d", "koven.p.peek.d"] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    // Two shared owners and one Data payload, each freed exactly once.
    counted(source, 3, b"rc\ncopy\n");
}
