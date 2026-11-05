use super::*;

#[test]
fn early_exit_move_only_reassignment_drops_both_owners_cleanly() {
    let source = r#"fun test_early_exit(flag: Boolean): Unit {
        var a = "first".clone()
        if (flag) {
            a = "second".clone()
            println(a)
            return
        }
        println("not-reached")
    }
    fun main(): Unit {
        test_early_exit(true)
    }"#;
    counted(source, 2, b"second\n");
}

#[test]
fn move_only_map_remove_direct_conditions_and_branch_exits() {
    let source = r#"class Tracked(val tag: String, val code: Int) {
        deinit() {
            println(this.tag)
        }
    }

    fun branch_return(key: String): Int {
        var m = mutableMapOf<String, Tracked>()
        m["r_hit"] = Tracked("drop-r_hit", 1)
        if (m.remove(key) != null) {
            println("ret-hit")
            return 10
        } else {
            println("ret-miss")
            return 20
        }
    }

    fun loop_break_continue(): Int {
        var m = mutableMapOf<String, Tracked>()
        m["c1"] = Tracked("drop-c1", 1)
        m["b1"] = Tracked("drop-b1", 1)
        var i = 0
        var count = 0
        while (i < 3) {
            i = i + 1
            if (m.remove("c1") != null) {
                println("loop-continue")
                continue
            }
            if (m.remove("b1") != null) {
                println("loop-break")
                break
            }
            count = count + 1
        }
        return count
    }

    fun main(): Unit {
        var m = mutableMapOf<String, Tracked>()
        m["hit1"] = Tracked("drop-hit1", 1)
        m["hit2"] = Tracked("drop-hit2", 1)
        m["s1"] = Tracked("drop-s1", 1)
        m["s2"] = Tracked("drop-s2", 1)

        // 1. Direct if != null (hit vs miss)
        if (m.remove("hit1") != null) {
            println("hit1-then")
        } else {
            println("hit1-else-unreachable")
        }
        if (m.remove("miss1") != null) {
            println("miss1-then-unreachable")
        } else {
            println("miss1-else")
        }

        // 1b. Named MoveOnly remove result entering NullableBranch with proof view and borrow
        m["named1"] = Tracked("view-named1", 42)
        val rem_named = m.remove("named1")
        if (rem_named != null) {
            if (rem_named.code == 42) {
                println("named1-view-ok")
            }
            println("named1-then")
        } else {
            println("named1-else-unreachable")
        }

        // 2. Direct if == null (hit enters else, miss enters then)
        if (m.remove("hit2") == null) {
            println("hit2-then-unreachable")
        } else {
            println("hit2-else")
        }
        if (m.remove("miss2") == null) {
            println("miss2-then")
        } else {
            println("miss2-else-unreachable")
        }

        // 3. Short-circuit && and ||
        if (m.remove("s1") != null && m.remove("s2") != null) {
            println("short-both-hit")
        }
        m["s3"] = Tracked("drop-s3", 1)
        if (m.remove("miss_short") != null && m.remove("s3") != null) {
            println("unreachable")
        } else {
            println("short-first-miss")
        }
        val s3_kept = m.withValue("s3", { it -> println("s3-kept") })
        if (s3_kept) {
            println("s3-kept-ok")
        }

        // 3b. Short-circuit ||
        m["o1"] = Tracked("drop-o1", 1)
        m["o2"] = Tracked("drop-o2", 1)
        if (m.remove("o1") != null || m.remove("o2") != null) {
            println("short-or-first-hit")
        }
        val o2_kept = m.withValue("o2", { it -> println("o2-kept") })
        if (o2_kept) {
            println("o2-kept-ok")
        }

        // 4. Two-branch function return
        val r1 = branch_return("r_hit")
        val r2 = branch_return("r_miss")

        // 5. Loop break and continue exits
        val l_res = loop_break_continue()
        if (r1 == 10 && r2 == 20 && l_res == 0) {
            println("map-condition-all-ok")
        }
    }"#;

    let mut sources = SourceMap::new();
    let text = format!("package p\n{source}");
    let (id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
    let (name_env, type_env) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_env, &type_env);
    let entry = declaration(&names, "p", "main");
    let (program, entry) =
        lower_scalar_unit_with_entry(&sources, &inputs, &names, &type_env, &typed, &owned, entry)
            .expect("refined program lowers and verifies");
    let rendered = render_program(&program);
    assert!(
        rendered.contains("cond ") || rendered.contains("branch"),
        "program must contain conditional branching: {rendered}"
    );
    let _llvm =
        crate::llvm::render_verified_program_with_entry(&program, entry).expect("verified LLVM");

    // Native run & deterministic exact drop output verification
    let (stdout, code) = run(source);
    assert_eq!(code, Some(0));
    assert_eq!(
        stdout,
        concat!(
            "drop-hit1\n",
            "hit1-then\n",
            "miss1-else\n",
            "named1-view-ok\n",
            "named1-then\n",
            "drop-hit2\n",
            "hit2-else\n",
            "miss2-then\n",
            "drop-s1\n",
            "drop-s2\n",
            "short-both-hit\n",
            "short-first-miss\n",
            "s3-kept\n",
            "s3-kept-ok\n",
            "drop-o1\n",
            "short-or-first-hit\n",
            "o2-kept\n",
            "o2-kept-ok\n",
            "drop-r_hit\n",
            "ret-hit\n",
            "ret-miss\n",
            "drop-r_hit\n",
            "drop-c1\n",
            "loop-continue\n",
            "drop-b1\n",
            "loop-break\n",
            "map-condition-all-ok\n",
            "view-named1\n",
            "drop-s3\n",
            "drop-o2\n",
        )
    );
}

#[test]
fn refine_and_legacy_validation_gate_matrix_tests() {
    use crate::ssa::LoweringErrorKind;

    // Case 1a: Inout-only unit succeeds without entering refine-exclusive bypass.
    let inout_ok_source = r#"
    fun inc(inout n: Int): Unit {
        n = n + 1
    }
    fun main(): Unit {
        var x = 1
        inc(&x)
        if (x == 2) {
            println("inout-ok")
        }
    }
    "#;
    let ((program, entry), routed) = super::super::refine::observe_routing(|| {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{inout_ok_source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_env, type_env) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_env, &type_env);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(&sources, &inputs, &names, &type_env, &typed, &owned, entry)
            .expect("inout-only unit lowers and verifies")
    });
    // Inout-only unit passes legacy whole-unit checks; inout functions still route to refine.
    assert!(routed.iter().any(|name| name.starts_with("koven.p.main.d")));
    assert!(routed.iter().any(|name| name.starts_with("koven.p.inc.d")));
    let _llvm =
        crate::llvm::render_verified_program_with_entry(&program, entry).expect("verified LLVM");
    let (stdout, code) = run(inout_ok_source);
    assert_eq!(code, Some(0));
    assert_eq!(stdout, "inout-ok\n");

    // Case 1b: Inout call inside borrow_val scope is rejected by legacy validate_bindings at exact span.
    let inout_reject_source = r#"
    class Holder(val label: String)
    fun read(view: String): Unit {}
    fun show(flag: Boolean): Unit {}
    fun change(inout n: Int): Boolean { n = 2; return true }
    fun borrow_part(h: Holder): borrow String from h = h.label
    fun scan(flag: Boolean): Unit {
        val root = Holder("a")
        var n = 1
        {
            borrow val part = borrow_part(root)
            show(flag || change(&n))
            read(part)
        }
    }
    fun main(): Unit {
        scan(true)
    }
    "#;
    let mut sources = SourceMap::new();
    let text = format!("package p\n{inout_reject_source}");
    let (id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
    let (name_env, type_env) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_env, &type_env);
    let entry = declaration(&names, "p", "main");
    let err =
        lower_scalar_unit_with_entry(&sources, &inputs, &names, &type_env, &typed, &owned, entry)
            .err()
            .expect("legacy validate_bindings must reject inout call within borrow scope");
    assert_eq!(err.kind, LoweringErrorKind::UnsupportedNode);
    let span = err.span.expect("exact span of inout call");
    assert_eq!(sources.slice(span).unwrap(), "flag || change(&n)");

    // Explicit proof that rejection originates specifically from the legacy validate_bindings stage:
    let parsed_files = [&file];
    let view = lang_frontend::ownership_checking::owned_compilation_unit_view(
        &sources, &inputs, &names, &type_env, &typed, &owned,
    )
    .expect("view construction succeeds");
    let direct_stage_err = crate::ssa::unit_lower::validate_bindings(
        &parsed_files,
        view.names(),
        view.types(),
        view.ownership(),
    )
    .expect_err("validate_bindings phase directly rejects this construct");
    assert_eq!(direct_stage_err.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(direct_stage_err.span, err.span);

    // Case 2: Unit with replace/swap field replacement routes past legacy check and runs successfully.
    let replace_ok_source = r#"
    class Box(var v: Int)
    fun main(): Unit {
        val b = Box(1)
        val old = replace(&b.v, 100)
        var opt: String? = "safe"
        if (opt != null && old == 1 && b.v == 100) {
            println("replace-field-ok")
        }
    }
    "#;
    let (stdout, code) = run(replace_ok_source);
    assert_eq!(code, Some(0));
    assert_eq!(stdout, "replace-field-ok\n");

    // Case 3: Mixed unit with refine main and legacy helper.
    let mixed_unit_source = r#"
    class Box(var v: Int)
    fun legacy_math_helper(x: Int, y: Int): Int {
        return x * 2 + y
    }
    fun main(): Unit {
        val b = Box(10)
        val old = replace(&b.v, 20)
        val res = legacy_math_helper(old, b.v)
        if (res == 40) {
            println("mixed-unit-ok")
        }
    }
    "#;
    let ((program, entry), routed) = super::super::refine::observe_routing(|| {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{mixed_unit_source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_env, type_env) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_env, &type_env);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(&sources, &inputs, &names, &type_env, &typed, &owned, entry)
            .expect("mixed unit lowers and verifies")
    });
    // main routes to refine; helper does not require refine constructs and stays in legacy
    assert!(routed.iter().any(|name| name.starts_with("koven.p.main.d")));
    assert!(
        !routed
            .iter()
            .any(|name| name.starts_with("koven.p.legacy_math_helper.d")),
        "legacy_math_helper should remain in per-family lowering: {routed:?}"
    );
    let _llvm =
        crate::llvm::render_verified_program_with_entry(&program, entry).expect("verified LLVM");
    let (stdout, code) = run(mixed_unit_source);
    assert_eq!(code, Some(0));
    assert_eq!(stdout, "mixed-unit-ok\n");
}

#[test]
fn retired_ownership_primitive_boundary_tests() {
    use crate::ssa::LoweringErrorKind;

    // Case 1: Natural routing under default entry. A function containing replace(&x, ...)
    // alongside an unmigrated construct (nullable subject when) naturally falls out of the
    // refine subset into legacy lowering, which rejects ownership_primitive at the exact call span.
    let natural_legacy_source = r#"
    fun natural_unmigrated(): Int {
        var x = 10
        val old = replace(&x, 20)
        val opt: Int? = 1
        val res = when (opt) {
            null -> 0
            else -> 1
        }
        return old + res
    }
    fun main(): Unit {
        natural_unmigrated()
    }
    "#;
    let mut sources = SourceMap::new();
    let text = format!("package p\n{natural_legacy_source}");
    let (id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
    let (name_env, type_env) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_env, &type_env);
    let entry = declaration(&names, "p", "main");
    let err =
        lower_scalar_unit_with_entry(&sources, &inputs, &names, &type_env, &typed, &owned, entry)
            .err()
            .expect("natural legacy routing must reject ownership_primitive at unit lowering");
    assert_eq!(err.kind, LoweringErrorKind::UnsupportedNode);
    let span = err.span.expect("exact span of replace call");
    assert_eq!(sources.slice(span).unwrap(), "replace(&x, 20)");

    // Case 2: Unit lowering with forced per-family lowering also rejects ownership_primitive at the exact call span.
    let primitive_source = r#"
    fun main(): Unit {
        var x = 10
        val old = replace(&x, 20)
        if (old == 10) {}
    }
    "#;
    let mut sources = SourceMap::new();
    let text = format!("package p\n{primitive_source}");
    let (id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
    let (name_env, type_env) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_env, &type_env);
    let entry = declaration(&names, "p", "main");
    let err = crate::ssa::with_per_family_lowering(|| {
        lower_scalar_unit_with_entry(&sources, &inputs, &names, &type_env, &typed, &owned, entry)
    })
    .err()
    .expect("retired ownership_primitive in legacy lowering must reject");
    assert_eq!(err.kind, LoweringErrorKind::UnsupportedNode);
    let span = err.span.expect("exact span of replace call");
    assert_eq!(sources.slice(span).unwrap(), "replace(&x, 20)");

    // Case 3: Positive verification: refine correctly handles replace with RHS early control transfer
    // (return, break, continue) and performs deterministic cleanup of enclosing MoveOnly resources.
    let rhs_move_only_cleanup_source = r#"
    class Res(val name: String) {
        deinit() {
            println(this.name)
        }
    }

    fun test_return_cleanup(flag: Boolean): Int {
        val res = Res("drop-ret")
        var a = 1
        var b = 2
        val old = replace(&a, if (flag) { return 42 } else { b + 10 })
        return a + old
    }

    fun test_loop_cleanup(): Int {
        var count = 0
        while (count < 2) {
            val loop_res = Res("drop-loop")
            var a = 10
            count = count + 1
            val old = replace(&a, if (count == 1) { continue } else { 20 })
            if (old == 10 && a == 20) {
                println("loop-normal-end")
            }
        }
        return count
    }

    fun test_break_cleanup(): Int {
        var count = 0
        while (count < 3) {
            val break_res = Res("drop-break")
            var a = 10
            count = count + 1
            val old = replace(&a, if (count == 2) { break } else { 30 })
        }
        return count
    }

    fun main(): Unit {
        val r1 = test_return_cleanup(true)
        val r2 = test_return_cleanup(false)
        val l1 = test_loop_cleanup()
        val l2 = test_break_cleanup()
        if (r1 == 42 && r2 == 13 && l1 == 2 && l2 == 2) {
            println("rhs-cleanup-all-ok")
        }
    }
    "#;
    let (stdout, code) = run(rhs_move_only_cleanup_source);
    assert_eq!(code, Some(0));
    assert_eq!(
        stdout,
        concat!(
            "drop-ret\n",
            "drop-ret\n",
            "drop-loop\n",
            "loop-normal-end\n",
            "drop-loop\n",
            "drop-break\n",
            "drop-break\n",
            "rhs-cleanup-all-ok\n",
        )
    );
}

#[test]
fn retired_enum_when_non_null_assertion_and_mutable_list_boundary_tests() {
    use crate::ssa::LoweringErrorKind;

    for (source, expected_slice) in [
        (
            "enum class Shape { Circle(r: Int), Point }\nfun main(): Unit { val s = Shape.Circle(2) }",
            "Shape.Circle(2)",
        ),
        (
            "enum class Shape { Circle(r: Int), Point }\nfun main(s: Shape): Boolean = s is Shape.Circle",
            "s is Shape.Circle",
        ),
        ("class Node()\nfun main(own n: Node?): Node = n!!", "n!!"),
        (
            "fun main(): Unit { var xs = mutableListOf<Int>(); xs.add(1) }",
            "xs.add(1)",
        ),
        (
            "fun main(): Unit { var xs = mutableListOf<Int>(); xs.clear() }",
            "xs.clear()",
        ),
        (
            "fun main(): Int { var xs = mutableListOf<Int>(); return xs.removeAt(0) }",
            "xs.removeAt(0)",
        ),
        (
            "fun main(): Int { var xs = mutableListOf<Int>(); return xs.removeFirst() }",
            "xs.removeFirst()",
        ),
        (
            "fun main(): Int { var xs = mutableListOf<Int>(); return xs.removeLast() }",
            "xs.removeLast()",
        ),
        (
            "fun main(): Unit { var xs = mutableListOf<Int>(); xs.insertAt(0, 1) }",
            "xs.insertAt(0, 1)",
        ),
    ] {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_env, type_env) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_env, &type_env);
        let entry = declaration(&names, "p", "main");
        let err = crate::ssa::with_per_family_lowering(|| {
            lower_scalar_unit_with_entry(
                &sources, &inputs, &names, &type_env, &typed, &owned, entry,
            )
        })
        .err()
        .unwrap_or_else(|| {
            panic!("expected UnsupportedNode under per-family lowering for: {source}")
        });
        assert_eq!(err.kind, LoweringErrorKind::UnsupportedNode, "{source}");
        let span = err.span.expect("exact rejection span");
        assert_eq!(sources.slice(span).unwrap(), expected_slice, "{source}");
    }
}

#[test]
fn retired_deinit_borrow_and_heap_nullable_if_boundary_tests() {
    use crate::ssa::LoweringErrorKind;

    for (source, expected_slice) in [
        (
            "class Inner(val text: String)\nclass Outer(val inner: Inner) { deinit() { println(this.inner.text) } }\nfun main(): Unit { val o = Outer(Inner(\"x\")) }",
            "this.inner",
        ),
        (
            "class Inner(val code: Int)\nclass Outer(val inner: Inner) { deinit() { if (this.inner.code == 1) {} } }\nfun main(): Unit { val o = Outer(Inner(1)) }",
            "this.inner",
        ),
        (
            "class Node(val code: Int)\nfun inspect(n: Node): Unit {}\nfun main(own n: Node?): Unit { if (n != null) { inspect(n) } }",
            "n",
        ),
    ] {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_env, type_env) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_env, &type_env);
        let entry = declaration(&names, "p", "main");
        let err = crate::ssa::with_per_family_lowering(|| {
            lower_scalar_unit_with_entry(
                &sources, &inputs, &names, &type_env, &typed, &owned, entry,
            )
        })
        .err()
        .unwrap_or_else(|| panic!("expected rejection under per-family lowering for: {source}"));
        assert!(
            matches!(
                err.kind,
                LoweringErrorKind::UnsupportedNode | LoweringErrorKind::MissingFact
            ),
            "{source}: {err:?}"
        );
        let span = err.span.expect("exact rejection span");
        assert_eq!(sources.slice(span).unwrap(), expected_slice, "{source}");
    }
}

#[test]
fn retired_inout_receiver_delegation_object_and_destructuring_boundary_tests() {
    use crate::ssa::LoweringErrorKind;

    for (source, expected_slice) in [
        (
            "class Counter(var item: Int) {\n    inout fun set(own next: Int): Unit { this.item = next }\n}\nfun main(): Unit { var c = Counter(1); c.set(2) }",
            "set",
        ),
        (
            "interface Readable { fun read(): Int }\nclass Reader: Readable { override fun read(): Int = 7 }\nclass Host(val delegate: Reader): Readable by delegate {}\nfun main(): Int { val h = Host(Reader()); return h.read() }",
            "h.read()",
        ),
        (
            "object Registry { fun ping(): Int = 7 }\nfun main(): Int = Registry.ping()",
            "Registry",
        ),
        (
            "value class Pair(val a: Int, val b: Int)\nfun main(): Int { val (x, y) = Pair(1, 2); return x + y }",
            "val (x, y) = Pair(1, 2)",
        ),
    ] {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_env, type_env) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_env, &type_env);
        let entry = declaration(&names, "p", "main");
        let err = crate::ssa::with_per_family_lowering(|| {
            lower_scalar_unit_with_entry(
                &sources, &inputs, &names, &type_env, &typed, &owned, entry,
            )
        })
        .err()
        .unwrap_or_else(|| panic!("expected rejection under per-family lowering for: {source}"));
        assert!(
            matches!(
                err.kind,
                LoweringErrorKind::UnsupportedNode | LoweringErrorKind::MissingFact
            ),
            "{source}: {err:?}"
        );
        let span = err.span.expect("exact rejection span");
        assert_eq!(sources.slice(span).unwrap(), expected_slice, "{source}");
    }
}

#[test]
fn retired_string_clone_box_value_field_borrow_and_when_boundary_tests() {
    use crate::ssa::LoweringErrorKind;

    for (source, expected_slice) in [
        (
            "fun main(): String = \"hello\".clone()",
            "\"hello\".clone()",
        ),
        (
            "value class Token(val item: Int)\nfun main(): Box<Token> = Box(Token(9))",
            "Box(Token(9))",
        ),
        (
            "value class Holder(val text: String)\nfun inspect(text: String): Unit {}\nfun observe(holder: Holder): Unit { inspect(holder.text) }\nfun main(): Unit { val h = Holder(\"a\"); observe(h) }",
            "holder.text",
        ),
        (
            "fun main(flag: Boolean): Int = when {\n    flag -> 1\n    else -> 2\n}",
            "when {\n    flag -> 1\n    else -> 2\n}",
        ),
    ] {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_env, type_env) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_env, &type_env);
        let entry = declaration(&names, "p", "main");
        let err = crate::ssa::with_per_family_lowering(|| {
            lower_scalar_unit_with_entry(
                &sources, &inputs, &names, &type_env, &typed, &owned, entry,
            )
        })
        .err()
        .unwrap_or_else(|| panic!("expected rejection under per-family lowering for: {source}"));
        assert!(
            matches!(
                err.kind,
                LoweringErrorKind::UnsupportedNode | LoweringErrorKind::MissingFact
            ),
            "{source}: {err:?}"
        );
        let span = err.span.expect("exact rejection span");
        assert_eq!(sources.slice(span).unwrap(), expected_slice, "{source}");
    }
}

#[test]
fn retired_numeric_for_runtime_container_map_get_and_consume_boundary_tests() {
    use crate::ssa::LoweringErrorKind;

    for (source, expected_slice) in [
        (
            "fun main(r: IntRange): Unit { for (i in r) { val copy = i } }",
            "for (i in r) { val copy = i }",
        ),
        (
            "fun main(): Array<Int> = Array<Int>(2, { index -> index })",
            "Array<Int>(2, { index -> index })",
        ),
        (
            "fun main(): Unit { val m = mapOf<Int, Int>(); val v = m.get(1) }",
            "m.get(1)",
        ),
        (
            "fun main(): Map<Int, Int> { val m = mutableMapOf<Int, Int>(); return m.consume() }",
            "m.consume()",
        ),
    ] {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_env, type_env) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_env, &type_env);
        let entry = declaration(&names, "p", "main");
        let err = crate::ssa::with_per_family_lowering(|| {
            lower_scalar_unit_with_entry(
                &sources, &inputs, &names, &type_env, &typed, &owned, entry,
            )
        })
        .err()
        .unwrap_or_else(|| panic!("expected rejection under per-family lowering for: {source}"));
        assert_eq!(err.kind, LoweringErrorKind::UnsupportedNode, "{source}");
        let span = err.span.expect("exact rejection span");
        assert_eq!(sources.slice(span).unwrap(), expected_slice, "{source}");
    }
}
