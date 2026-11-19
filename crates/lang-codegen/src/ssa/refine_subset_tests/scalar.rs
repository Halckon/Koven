//! SPEC-0454: the new trigger changes routing, not scalar semantics or caller authority.
use super::*;
use crate::{ssa::model::FunctionId, test_support::diagnostics::assert_eq_diagnostic};

#[test]
fn linear_scalar_functions_refine_without_another_family_trigger() {
    let mut sources = vec![
        "fun f(): Int = 1".to_owned(),
        "fun f(own x: Int): Int = x + 1".to_owned(),
        "fun f(x: Long): Long = -x".to_owned(),
        "fun f(b: Boolean): Boolean = !b".to_owned(),
        "fun f(x: Int): Boolean = x >= 1".to_owned(),
        "fun f(): Unit {}".to_owned(),
        "fun f(): Unit { return }".to_owned(),
        "fun f(x: Int): Int { var total = x; val next = total + 1; total = next; return total }"
            .to_owned(),
    ];
    for ty in [
        "Byte", "Short", "Int", "Long", "UByte", "UShort", "UInt", "ULong",
    ] {
        let suffix = if ty.starts_with('U') { "u" } else { "" };
        sources.push(format!(
            "fun f(x: {ty}): {ty} {{ val one: {ty} = 1{suffix}; return (x + one) }}"
        ));
    }
    for source in sources {
        let (result, routed) = route(&source);
        let program = result.unwrap_or_else(|error| panic!("{source}: {error:?}"));
        assert!(
            routed,
            "a supported linear scalar body must refine: {source}"
        );
        crate::ssa::verify::verify_program(&program).expect("complete SSA/ownership verifier");
        crate::llvm::render_verified_program(&program).expect("scalar LLVM verifies");
    }
}

#[test]
fn scalar_calls_and_control_flow_refine_without_another_trigger() {
    for source in [
        "fun g(): Int = 1\nfun f(): Int = g()",
        "fun f(c: Boolean): Int = if (c) 1 else 0",
        "fun f(a: Boolean, b: Boolean): Boolean = a && b",
        "fun f(): Int { var i = 0; while (i < 2) { i = i + 1 }; return i }",
    ] {
        let (result, routed) = route(source);
        let program =
            result.unwrap_or_else(|error| panic!("scalar source failed: {source}: {error:?}"));
        assert!(routed, "scalar call/control must refine: {source}");
        crate::ssa::verify::verify_program(&program).expect("complete SSA/ownership verifier");
        crate::llvm::render_verified_program(&program).expect("scalar LLVM verifies");
    }
}

#[test]
fn scalar_trigger_preserves_non_scalar_signature_boundaries() {
    for source in [
        "class Token()\nfun f(t: Token): Int = 1",
        "fun f(s: String): Int = 1",
        "fun f(own s: String): Int = 1",
    ] {
        let (result, routed) = route(source);
        let program =
            result.unwrap_or_else(|error| panic!("existing source failed: {source}: {error:?}"));
        assert!(
            !routed,
            "the scalar-only trigger must not widen non-scalar signatures: {source}"
        );
        crate::llvm::render_verified_program(&program).expect("existing path still verifies");
    }
}

#[test]
fn non_scalar_signatures_project_routing_and_native_output_are_stable() {
    let helper = "class Token(val id: Int)\n\
                  fun borrow_token(t: Token): Int = 1\n\
                  fun borrow_str(s: String): Int = 2\n\
                  fun own_str(own s: String): Int = 3";
    let consumer = "fun main(): Unit { if (borrow_token(Token(1)) + borrow_str(\"a\") + own_str(\"b\".clone()) == 6) { println(\"non-scalar-sig-ok\") } }";
    let mut expected = None;
    for reverse in [false, true] {
        let (program, _, routed) = lower_project(helper, consumer, reverse);
        for name in [
            "koven.p.borrow_token.d",
            "koven.p.borrow_str.d",
            "koven.p.own_str.d",
        ] {
            assert!(
                !routed.iter().any(|r| r.starts_with(name)),
                "{name}: {routed:?}"
            );
        }
        crate::ssa::verify::verify_program(&program).expect("complete SSA/ownership verifier");
        let llvm = crate::llvm::render_verified_program(&program).expect("non-scalar project LLVM");
        let actual = (render_program(&program), llvm);
        if let Some(expected) = &expected {
            assert_eq_diagnostic!(expected, &actual, "input order must not change output");
        }
        expected = Some(actual);
    }
    let source = format!("{helper}\n{consumer}");
    let run = crate::native_tests::emit_link_and_run("non_scalar_sig.ko", &source, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"non-scalar-sig-ok\n");
}

/// Keeps the real validated Unit boundary and ordinary routing for two source files.
fn lower_project(
    helper: &str,
    consumer: &str,
    reverse: bool,
) -> (Program, FunctionId, Vec<String>) {
    let mut sources = SourceMap::new();
    let (helper_id, helper_file) =
        parsed(&mut sources, "p/helper.ko", &format!("package p\n{helper}"));
    let (main_id, main_file) = parsed(&mut sources, "p/main.ko", &format!("package p\n{consumer}"));
    let mut inputs = [
        SourceUnitInput::new("root", "p/helper.ko", helper_id, &helper_file),
        SourceUnitInput::new("root", "p/main.ko", main_id, &main_file),
    ];
    if reverse {
        inputs.reverse();
    }
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "p", "main");
    let (result, routed) = observe_routing(|| {
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
    });
    let (program, root) = result.expect("original project must lower through the normal Unit gate");
    (program, root, routed)
}

#[test]
fn generic_scalar_instance_and_caller_refine_without_another_trigger() {
    for sibling in [
        "",
        "\nfun uncalled(own x: Int?): Int = if (x == null) 0 else 1",
    ] {
        let mut expected = None;
        for reverse in [false, true] {
            let (program, _, routed) = lower_project(
                &format!("fun <T> identity(own x: T): T = x{sibling}"),
                "fun main(): Int = identity<Int>(1)",
                reverse,
            );
            assert!(
                routed.iter().any(|name| name.contains(".identity.")),
                "generic scalar instance must refine: {routed:?}"
            );
            assert!(
                routed.iter().any(|name| name.starts_with("koven.p.main.d")),
                "generic scalar caller must refine: {routed:?}"
            );
            assert!(
                !routed
                    .iter()
                    .any(|name| name.starts_with("koven.p.uncalled.d"))
            );
            crate::ssa::verify::verify_program(&program).expect("complete SSA/ownership verifier");
            let llvm = crate::llvm::render_verified_program(&program)
                .expect("generic scalar LLVM verifies");
            let actual = (render_program(&program), llvm);
            if let Some(expected) = &expected {
                assert_eq_diagnostic!(
                    expected,
                    &actual,
                    "input order must not change generic scalar output"
                );
            }
            expected = Some(actual);
        }
    }
}

#[test]
fn scalar_calls_control_and_generic_execute_through_the_native_entry() {
    let source = "fun <T> identity(own x: T): T = x\n                  fun step(n: Int): Int = if (n > 0 && n < 10) identity<Int>(n + 1) else 0\n                  fun sum_to(limit: Int): Int { var i = 0; var total = 0; while (i < limit) { i = i + 1; total = total + step(i) }; return total }\n                  fun main(): Unit { if (sum_to(3) == 9) { println(\"scalar-control-generic-ok\") } }";
    let (run, routed) = observe_routing(|| {
        crate::native_tests::emit_link_and_run("scalar_control_generic.ko", source, "main")
    });
    for name in [".identity.", ".step.d", ".sum_to.d"] {
        assert!(
            routed.iter().any(|entry| entry.contains(name)),
            "expected {name} in routed: {routed:?}"
        );
    }
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"scalar-control-generic-ok\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn scalar_project_routing_native_output_and_sibling_isolation_are_stable() {
    use std::process::Command;

    let helper = "fun f(x: Int): Int { var total = x; val next = total + 1; total = next; return total }\n\
                  fun flipped(b: Boolean): Boolean = !b";
    let consumer = "fun main(): Unit { if (f(40) == 41 && flipped(false)) { println(\"linear-project-ok\") } }";
    for sibling in [
        "",
        "\nfun uncalled(own x: Int?): Int = if (x == null) 0 else 1",
    ] {
        // Adding a declaration may renumber main's source symbol. Compare full IR for
        // identical source bytes; sibling isolation requires the same route and native result.
        let mut expected = None;
        for reverse in [false, true] {
            let (program, root, routed) =
                lower_project(&format!("{helper}{sibling}"), consumer, reverse);
            for name in ["koven.p.f.d", "koven.p.flipped.d"] {
                assert!(
                    routed.iter().any(|routed| routed.starts_with(name)),
                    "{name}: {routed:?}"
                );
            }
            assert!(
                !routed
                    .iter()
                    .any(|name| name.starts_with("koven.p.uncalled.d"))
            );
            let rendered = render_program(&program);
            let llvm = crate::llvm::render_verified_program_with_entry(&program, root)
                .expect("project SSA/ownership and LLVM verify");
            let actual = (rendered, llvm.clone());
            if let Some(expected) = &expected {
                assert_eq_diagnostic!(
                    expected,
                    &actual,
                    "input order must not change output for identical source bytes"
                );
            }
            expected = Some(actual);

            let directory = crate::native_tests::TestDirectory::create();
            let ir = directory.join("program.ll");
            let executable = directory.join("program");
            std::fs::write(&ir, llvm).expect("write verified LLVM");
            let linked = Command::new(crate::test_support::clang())
                .arg(&ir)
                .arg("-o")
                .arg(&executable)
                .output()
                .expect("clang launches");
            assert!(linked.status.success(), "{linked:?}");
            let run = Command::new(&executable)
                .output()
                .expect("project executable launches");
            assert!(run.status.success(), "{run:?}");
            assert_eq!(run.stdout, b"linear-project-ok\n");
            assert!(run.stderr.is_empty(), "{run:?}");
        }
    }
}

#[test]
fn scalar_widths_and_boolean_results_execute_through_the_native_entry() {
    let mut source = "fun flipped(b: Boolean): Boolean = !b\n".to_owned();
    let mut conditions = Vec::new();
    let mut arguments = String::new();
    for ty in [
        "Byte", "Short", "Int", "Long", "UByte", "UShort", "UInt", "ULong",
    ] {
        let suffix = if ty.starts_with('U') { "u" } else { "" };
        source.push_str(&format!(
            "fun step_{ty}(x: {ty}): {ty} {{ val one: {ty} = 1{suffix}; return x + one }}\n"
        ));
        arguments.push_str(&format!(
            "val input_{ty}: {ty} = 40{suffix}; val expected_{ty}: {ty} = 41{suffix}\n"
        ));
        conditions.push(format!("step_{ty}(input_{ty}) == expected_{ty}"));
    }
    source.push_str(&format!(
        "fun main(): Unit {{ {arguments}if ({} && flipped(false)) {{ println(\"linear-scalars-ok\") }} }}",
        conditions.join(" && "),
    ));
    let run = crate::native_tests::emit_link_and_run("linear_scalars.ko", &source, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"linear-scalars-ok\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
#[cfg(unix)]
fn scalar_checked_arithmetic_still_aborts_instead_of_wrapping_or_dividing_by_zero() {
    use std::os::unix::process::ExitStatusExt;

    for (expression, argument) in [
        ("x + 1", "2147483647"),
        ("1 / x", "0"),
        ("x / -1", "-2147483648"),
    ] {
        let source = format!(
            "fun f(x: Int): Int = {expression}\n\
             fun main(): Unit {{ val result = f({argument}); println(\"unexpected-normal-exit\") }}"
        );
        let (result, routed) = route(&source);
        result.expect("checked scalar source verifies");
        assert!(routed, "the test must exercise refined checked arithmetic");
        let run = crate::native_tests::emit_link_and_run("linear_abort.ko", &source, "main");
        assert_eq!(
            run.status.signal(),
            Some(6),
            "checked arithmetic must SIGABRT: {run:?}"
        );
        assert!(
            run.stdout.is_empty(),
            "normal successor must not execute: {run:?}"
        );
    }
}

#[test]
fn refined_linear_and_legacy_branch_helpers_coexist_in_the_native_program() {
    let source = "fun linear(x: Int): Int = x + 1\n\
                  fun compat_tag(s: String): Int = 1\n\
                  fun main(): Unit {\n\
                      var marker: Int = 0\n\
                      val previous = replace(&marker, 1)\n\
                      if (previous == 0 && marker == 1 && linear(40) == 41 && compat_tag(\"ok\") == 1) { println(\"mixed-linear-ok\") }\n\
                  }";
    let (run, routed) = observe_routing(|| {
        crate::native_tests::emit_link_and_run("mixed_linear.ko", source, "main")
    });
    assert!(
        routed.iter().any(|name| name.contains(".linear.d")),
        "{routed:?}"
    );
    assert!(
        !routed.iter().any(|name| name.contains(".compat_tag.d")),
        "{routed:?}"
    );
    assert!(
        routed.iter().any(|name| name.contains(".main.d")),
        "main must refine to cover refined-to-compatibility calls: {routed:?}"
    );
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"mixed-linear-ok\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn uncalled_sibling_locals_do_not_pollute_linear_scalar_type_demands() {
    let source = "fun f(): Int = 1";
    let (baseline, routed) = route(source);
    assert!(routed);
    let baseline = baseline.expect("baseline scalar source lowers");
    let expected = render_program(&baseline);
    for sibling in [
        "fun dead(): Unit { val text = \"unused\" }",
        "fun dead(): Unit { val wide: Long = 1 }",
    ] {
        let (program, routed) = route(&format!("{source}\n{sibling}"));
        assert!(routed);
        let program = program.expect("unused local cannot affect the reachable scalar body");
        assert_eq!(program.modules[0].functions.len(), 1);
        assert_eq_diagnostic!(
            expected,
            render_program(&program),
            "unused String/Long locals must not register runtime types in the reachable program"
        );
        crate::llvm::render_verified_program(&program).expect("unpolluted scalar LLVM verifies");
    }
}

#[test]
fn test_only_forced_per_family_control_is_preserved() {
    let source = "fun f(x: Int): Int = x + 1";
    let (result, routed) = crate::ssa::refine::with_per_family_lowering(|| route(source));
    assert!(
        !routed,
        "test-only override still selects the legacy control"
    );
    let program = result.expect("the scalar legacy control remains executable");
    crate::llvm::render_verified_program(&program).expect("forced control LLVM verifies");
}

#[test]
fn nested_primitive_range_view_consumption_routes_and_verifies_in_refine() {
    let source = r#"
    fun head_view(xs: List<Int>, n: Int): View<Int> from xs = rangeView(xs, 0, n)
    fun nested_slice(parent: View<Int>): View<Int> from parent = rangeView(parent, 0, 2)
    fun sum_view(v: View<Int>): Int {
        var total = 0
        for (x in v) { total = total + x }
        return total
    }
    fun main(): Int {
        val xs = listOf(10, 20, 30, 40, 50)
        borrow val a = rangeView(rangeView(xs, 1, 4), 0, 2)
        borrow val b = rangeView(head_view(xs, 4), 1, 3)
        var acc = sum_view(a) + sum_view(b)
        for (x in rangeView(rangeView(xs, 1, 5), 1, 3)) {
            acc = acc + x
        }
        acc = acc + sum_view(rangeView(rangeView(xs, 0, 4), 1, 3))
        borrow val c = nested_slice(a)
        acc = acc + sum_view(c)
        return acc
    }"#;
    let mut sources = SourceMap::new();
    let text = format!("package p\n{source}");
    let (id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
    let (name_environment, mut type_environment) = standard_environments();
    type_environment
        .authorize_range_source(&sources, id)
        .unwrap();
    type_environment
        .authorize_range_extension_source(&sources, id)
        .unwrap();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "p", "main");
    let (res, routed) = observe_routing(|| {
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
    });
    let (program, _) = res.expect("nested primitive rangeView must lower and verify");
    for name in [
        "koven.p.main.d",
        "koven.p.nested_slice.d",
        "koven.p.head_view.d",
    ] {
        assert!(
            routed.iter().any(|r| r.starts_with(name)),
            "{name} must route to refine: {routed:?}"
        );
    }
    let llvm = crate::llvm::render_verified_program(&program)
        .expect("verified LLVM for nested primitive rangeView");
    assert!(!llvm.is_empty());
}
