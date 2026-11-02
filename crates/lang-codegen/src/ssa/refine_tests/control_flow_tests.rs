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
