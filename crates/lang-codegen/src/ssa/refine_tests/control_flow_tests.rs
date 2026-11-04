use super::*;

#[test]
fn checked_division_by_zero_with_sibling_slot_reassignment() {
    let source = r#"fun test_div(opt: Int?, flag: Boolean, input: Int): Int {
        var d = input
        val result: Int = if (flag) {
            if (opt != null) {
                val q = 100 / d
                q
            } else {
                0
            }
        } else {
            d = 50
            val q = 200 / d
            q
        }
        return result
    }
    fun main(): Unit {
        val normal = test_div(1, true, 2)
        if (normal == 50) { println("normal-ok") }
        val branch_b = test_div(null, false, 0)
        if (branch_b == 4) { println("branch-b-ok") }
        val res = test_div(1, true, 0)
        if (res == 0) { println("unreachable") }
    }"#;
    let output = crate::native_tests::emit_link_and_run("div_zero.ko", source, "main");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "normal-ok\nbranch-b-ok\n"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("abort: division by zero\n  at "),
        "{stderr:?}"
    );
}

#[test]
fn if_single_sided_reassignment_ends_view_on_correct_edge_and_preserves_resources() {
    // Branch 1 reassigns `a`, ending its view early; branch 0 leaves it unchanged.
    // At join, branch 0 must emit BorrowEnd on its edge, leaving active_views empty
    // so `a` can be safely reassigned or used afterwards without resource leaks.
    let source_true = r#"fun test_branch(flag: Boolean): Unit {
        var a: String? = "initial".clone()
        if (a != null) {
            if (flag) {
                a = "reassigned".clone()
            } else {
                println("else-branch")
            }
            if (a != null) {
                println(a)
            }
        }
    }
    fun main(): Unit {
        test_branch(true)
    }"#;
    counted(source_true, 2, b"reassigned\n");

    let source_false = r#"fun test_branch(flag: Boolean): Unit {
        var a: String? = "initial".clone()
        if (a != null) {
            if (flag) {
                a = "reassigned".clone()
            } else {
                println("else-branch")
            }
            if (a != null) {
                println(a)
            }
        }
    }
    fun main(): Unit {
        test_branch(false)
    }"#;
    counted(source_false, 1, b"else-branch\ninitial\n");
}

#[test]
fn if_both_branches_preserve_view_retains_smart_cast() {
    let source = r#"fun main(): Unit {
        val a: String? = "common_view".clone()
        val flag = true
        if (a != null) {
            if (flag) {
                println("then")
            } else {
                println("else")
            }
            // Common view survives both branches: smart-cast holds without another `a != null` check
            println(a)
        }
    }"#;
    counted(source, 1, b"then\ncommon_view\n");
}

#[test]
fn two_arm_when_single_sided_reassignment_preserves_resources() {
    let source_true = r#"fun test_when(flag: Boolean): Unit {
        var a: String? = "when_init".clone()
        if (a != null) {
            when (flag) {
                true -> {
                    a = "when_reassigned".clone()
                }
                false -> {
                    println("when_false")
                }
            }
            if (a != null) {
                println(a)
            }
        }
    }
    fun main(): Unit {
        test_when(true)
    }"#;
    counted(source_true, 2, b"when_reassigned\n");

    let source_false = r#"fun test_when(flag: Boolean): Unit {
        var a: String? = "when_init".clone()
        if (a != null) {
            when (flag) {
                true -> {
                    a = "when_reassigned".clone()
                }
                false -> {
                    println("when_false")
                }
            }
            if (a != null) {
                println(a)
            }
        }
    }
    fun main(): Unit {
        test_when(false)
    }"#;
    counted(source_false, 1, b"when_false\nwhen_init\n");
}

#[test]
fn subjectless_when_single_sided_reassignment_preserves_resources() {
    let source_branch1 = r#"fun test_subjectless(tag: Int): Unit {
        var a: String? = "subj_init".clone()
        if (a != null) {
            when {
                tag == 1 -> {
                    a = "subj_one".clone()
                }
                tag == 2 -> {
                    println("subj_two")
                }
                else -> {
                    println("subj_other")
                }
            }
            if (a != null) {
                println(a)
            }
        }
    }
    fun main(): Unit {
        test_subjectless(1)
    }"#;
    counted(source_branch1, 2, b"subj_one\n");

    let source_branch2 = r#"fun test_subjectless(tag: Int): Unit {
        var a: String? = "subj_init".clone()
        if (a != null) {
            when {
                tag == 1 -> {
                    a = "subj_one".clone()
                }
                tag == 2 -> {
                    println("subj_two")
                }
                else -> {
                    println("subj_other")
                }
            }
            if (a != null) {
                println(a)
            }
        }
    }
    fun main(): Unit {
        test_subjectless(2)
    }"#;
    counted(source_branch2, 1, b"subj_two\nsubj_init\n");

    let source_else = r#"fun test_subjectless(tag: Int): Unit {
        var a: String? = "subj_init".clone()
        if (a != null) {
            when {
                tag == 1 -> {
                    a = "subj_one".clone()
                }
                tag == 2 -> {
                    println("subj_two")
                }
                else -> {
                    println("subj_other")
                }
            }
            if (a != null) {
                println(a)
            }
        }
    }
    fun main(): Unit {
        test_subjectless(3)
    }"#;
    counted(source_else, 1, b"subj_other\nsubj_init\n");
}

#[test]
fn when_early_return_preserves_surviving_branch_view() {
    let source = r#"fun test_early_ret(tag: Int): Unit {
        val a: String? = "surviving".clone()
        if (a != null) {
            when (tag) {
                1 -> {
                    println("early-exit")
                    return
                }
                else -> {
                    println("stay")
                }
            }
            // Tag 2 reaches here; since branch 1 returned, only branch 2 reached join.
            // Branch 2's view of `a` is preserved.
            println(a)
        }
    }
    fun main(): Unit {
        test_early_ret(2)
    }"#;
    counted(source, 1, b"stay\nsurviving\n");
}

#[test]
fn when_early_break_preserves_surviving_branch_view() {
    let source = r#"fun main(): Unit {
        val a: String? = "loop_view".clone()
        var i = 0
        if (a != null) {
            while (i < 2) {
                i = i + 1
                when (i) {
                    1 -> {
                        break
                    }
                    else -> {}
                }
            }
            println(a)
        }
    }"#;
    counted(source, 1, b"loop_view\n");
}

#[test]
fn when_fallthrough_preserves_view_and_handles_asymmetric_reassignment() {
    // When tag != 1, falls through without matching any arm.
    // Branch 1 reassigns `a`; fallthrough keeps `a`.
    // At join, common is empty. Fallthrough emits BorrowEnd on its incoming edge.
    let source_fallthrough = r#"fun test_fallthrough(tag: Int): Unit {
        var a: String? = "fall_init".clone()
        if (a != null) {
            when (tag) {
                1 -> {
                    a = "matched_reassigned".clone()
                }
            }
            if (a != null) {
                println(a)
            }
        }
    }
    fun main(): Unit {
        test_fallthrough(2)
    }"#;
    counted(source_fallthrough, 1, b"fall_init\n");

    let source_matched = r#"fun test_fallthrough(tag: Int): Unit {
        var a: String? = "fall_init".clone()
        if (a != null) {
            when (tag) {
                1 -> {
                    a = "matched_reassigned".clone()
                }
            }
            if (a != null) {
                println(a)
            }
        }
    }
    fun main(): Unit {
        test_fallthrough(1)
    }"#;
    counted(source_matched, 2, b"matched_reassigned\n");
}

#[test]
fn outer_proof_view_with_loop_reassignment_is_rejected_by_subset_admission() {
    // Static candidate (b): Reassigning a symbol with an outer proof view inside a loop
    // is strictly rejected by the refine subset admission gate:
    // `subset/field.rs: (self.loops > 0 && self.view_symbols.contains(&symbol))`
    // This test verifies that admission stops it from entering refine lowering,
    // rather than lowering incorrectly.
    let source = r#"fun main(): Unit {
        var outer: String? = "initial_loop_proof".clone()
        if (outer != null) {
            var i = 0
            while (i < 2) {
                outer = "reassigned_in_loop".clone()
                break
            }
            if (outer != null) {
                println(outer)
            }
        }
    }"#;
    let mut sources = SourceMap::new();
    let text = format!("package p\n{source}");
    let (id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "p", "main");
    let (_, routed) = super::super::refine::observe_routing(|| {
        let _ = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        );
    });
    // Verified: main is NOT routed to refine because subset rejected loop reassignment of an active view.
    assert!(
        !routed.iter().any(|name| name.contains("main")),
        "symbol with outer proof view reassigned inside loop must not be admitted to refine: {routed:?}"
    );
}

#[test]
fn loop_reassignment_without_outer_view_enters_refine_and_preserves_resources() {
    // When there is no outer proof view before the loop, reassignment inside loop
    // cleanly passes refine admission and executes with zero leaks.
    let source = r#"fun main(): Unit {
        var outer: String? = "initial_no_proof".clone()
        var i = 0
        while (i < 2) {
            outer = "reassigned_in_loop".clone()
            break
        }
        if (outer != null) {
            println(outer)
        }
    }"#;
    counted(source, 2, b"reassigned_in_loop\n");
}

#[test]
fn join_views_named_dead_insert_preserves_resources_without_double_drop() {
    // Direction A (P1 static candidate): When a MoveOnly local variable is consumed (deliver)
    // inside one branch leading to dead.insert, the join_views exit snapshot must propagate
    // this dead fact to the join block so outer scope exits will not double-drop it.
    let source_true = r#"fun consume(text: String): Unit { println(text) }
    fun test_branch(flag: Boolean): Unit {
        val item: String? = "move_me".clone()
        if (item != null) {
            if (flag) {
                consume(item)
            } else {
                println("did_not_consume")
            }
        }
    }
    fun main(): Unit {
        test_branch(true)
    }"#;
    counted(source_true, 1, b"move_me\n");

    let source_false = r#"fun consume(text: String): Unit { println(text) }
    fun test_branch(flag: Boolean): Unit {
        val item: String? = "move_me".clone()
        if (item != null) {
            if (flag) {
                consume(item)
            } else {
                println("did_not_consume")
            }
        }
    }
    fun main(): Unit {
        test_branch(false)
    }"#;
    counted(source_false, 1, b"did_not_consume\n");
}

#[test]
fn join_views_named_dead_remove_on_reassignment_preserves_live_state() {
    // Direction B (P1 static candidate): When a local variable is moved and then reassigned
    // within branches (dead.remove), the join_views exit snapshot must coordinate with dead_block
    // so the live state of the newly assigned values survives at the join block and is cleanly dropped.
    let source_true = r#"fun consume(text: String): Unit { println(text) }
    fun test_reassign(flag: Boolean): Unit {
        var item: String? = "init_item".clone()
        if (item != null) {
            if (flag) {
                consume(item)
                item = "then_reassigned".clone()
            } else {
                consume(item)
                item = "else_reassigned".clone()
            }
            if (item != null) {
                println(item)
            }
        }
    }
    fun main(): Unit {
        test_reassign(true)
    }"#;
    counted(source_true, 2, b"init_item\nthen_reassigned\n");

    let source_false = r#"fun consume(text: String): Unit { println(text) }
    fun test_reassign(flag: Boolean): Unit {
        var item: String? = "init_item".clone()
        if (item != null) {
            if (flag) {
                consume(item)
                item = "then_reassigned".clone()
            } else {
                consume(item)
                item = "else_reassigned".clone()
            }
            if (item != null) {
                println(item)
            }
        }
    }
    fun main(): Unit {
        test_reassign(false)
    }"#;
    counted(source_false, 2, b"init_item\nelse_reassigned\n");
}

#[test]
fn join_views_single_arrival_with_divergent_predecessor_preserves_resources() {
    // When one branch diverges (early return), join_views receives only a single predecessor.
    // The exit snapshot must accurately reflect the surviving branch's dead set.
    let source = r#"fun consume(text: String): Unit { println(text) }
    fun test_diverge(flag: Boolean): Unit {
        var item: String? = "diverge_test".clone()
        if (item != null) {
            if (flag) {
                consume(item)
                return
            } else {
                consume(item)
                item = "surviving_branch".clone()
            }
            if (item != null) {
                println(item)
            }
        }
    }
    fun main(): Unit {
        test_diverge(false)
        test_diverge(true)
    }"#;
    counted(source, 3, b"diverge_test\nsurviving_branch\ndiverge_test\n");
}

#[test]
fn checked_remainder_by_zero_with_sibling_slot_reassignment() {
    // Verifies `%` checked remainder divisor dominance in refine lowering.
    let source = r#"fun test_rem(opt: Int?, flag: Boolean, input: Int): Int {
        var d = input
        val result: Int = if (flag) {
            if (opt != null) {
                val q = 100 % d
                q
            } else {
                0
            }
        } else {
            d = 50
            val q = 200 % d
            q
        }
        return result
    }
    fun main(): Unit {
        val normal = test_rem(1, true, 3)
        if (normal == 1) { println("rem-normal-ok") }
        val branch_b = test_rem(null, false, 0)
        if (branch_b == 0) { println("rem-branch-b-ok") }
        val res = test_rem(1, true, 0)
        if (res == 0) { println("unreachable") }
    }"#;
    let output = crate::native_tests::emit_link_and_run("rem_zero.ko", source, "main");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "rem-normal-ok\nrem-branch-b-ok\n"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("abort: division by zero\n  at "),
        "{stderr:?}"
    );
}

#[test]
fn checked_arithmetic_overflow_without_divisor_aborts_cleanly() {
    // Negative control: Add/Multiply overflow aborts do not consume or require a divisor.
    let source = r#"fun main(): Unit {
        var x: Int? = 2147483647
        if (x != null) {
            val y = x + 1
            if (y == 0) { println("unreachable") }
        }
    }"#;
    let output = crate::native_tests::emit_link_and_run("add_overflow.ko", source, "main");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("abort: integer overflow in +\n  at "),
        "{stderr:?}"
    );
}

#[test]
fn checked_multiply_overflow_without_divisor_aborts_cleanly() {
    let source = r#"fun main(): Unit {
        var x: Int? = 1000000
        if (x != null) {
            val y = x * 1000000
            if (y == 0) { println("unreachable") }
        }
    }"#;
    let output = crate::native_tests::emit_link_and_run("mul_overflow.ko", source, "main");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("abort: integer overflow in *\n  at "),
        "{stderr:?}"
    );
}

#[test]
fn named_arguments_evaluation_order_and_slot_mapping() {
    // Named arguments must evaluate in declaration argument order (side-effects)
    // while filling target parameter slots by parameter_index.
    let source = r#"
    fun trace(tag: String, value: Int): Int {
        println(tag)
        return value
    }
    fun pair(first: Int, second: Int): Unit {
        if (first == 1 && second == 2) {
            println("pair-ok")
        }
    }
    fun main(): Unit {
        val dummy: String? = "refine_tag".clone()
        if (dummy != null) {
            // Evaluation order: second arg evaluated first, then first arg
            pair(second = trace("eval-second", 2), first = trace("eval-first", 1))
        }
    }"#;
    counted(source, 1, b"eval-second\neval-first\npair-ok\n");
}

#[test]
fn logical_short_circuit_dead_isolation_and_early_exit() {
    // Short-circuit `||` / `&&` must isolate dead state when RHS moves a MoveOnly binding.
    let source = r#"
    fun consume(text: String): Boolean {
        println(text)
        return true
    }
    fun test_short_circuit(dummy: Int?, skip_rhs: Boolean): Unit {
        if (dummy != null) {
            val text = "payload".clone()
            if (skip_rhs || consume(text)) {
                println("condition-true")
            }
        }
    }
    fun main(): Unit {
        var flag: Int? = 1
        if (flag != null) {
            test_short_circuit(flag, true)
            test_short_circuit(flag, false)
        }
    }"#;
    counted(source, 2, b"condition-true\npayload\ncondition-true\n");
}

#[test]
fn abort_predecessor_string_consume_then_both_abort() {
    let source = r#"
    fun consume(text: String): Unit {
        println(text)
    }
    fun test_case(opt: Int?, flag: Boolean, text: String): Unit {
        if (opt != null) {
            consume(text)
            if (flag) {
                error("abort-branch-a")
            } else {
                error("abort-branch-b")
            }
        }
    }
    fun main(): Unit {
        test_case(1, true, "hello".clone())
    }"#;
    let output = crate::native_tests::emit_link_and_run("consume_then_abort.ko", source, "main");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "hello\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("abort: abort-branch-a\n  at "),
        "{stderr:?}"
    );
}

#[test]
fn abort_predecessor_aggregate_construct_then_both_abort() {
    let source = r#"
    class BoxedPayload(val text: String)
    fun test_case(flag: Boolean): Unit {
        val text = "boxed-payload".clone()
        val boxed = BoxedPayload(text)
        println(boxed.text)
        if (flag) {
            error("abort-aggregate-a")
        } else {
            error("abort-aggregate-b")
        }
    }
    fun main(): Unit {
        test_case(false)
    }"#;
    let output = crate::native_tests::emit_link_and_run("aggregate_then_abort.ko", source, "main");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "boxed-payload\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("abort: abort-aggregate-b\n  at "),
        "{stderr:?}"
    );
}
