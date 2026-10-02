//! SPEC-0244：跨文件 root 原语的真实 native 值流、控制前缀与资源回收。
use super::super::analyze_sources;
use super::run_constant_sources;
use crate::ssa::unit_lower::lower_scalar_unit_with_entry;

#[test]
fn unit_root_primitive_native_cross_file_order_and_dynamic_owners() {
    let analysis = analyze_sources(
        "package p\nfun replacement(): String { println(\"make\")\nreturn \"new\" + \"!\" }",
        "package q\nfun entry(): Unit {\nvar left = \"left\" + \"!\"\nvar right = \"right\" + \"!\"\nval old = replace(&left, p.replacement())\nswap(&left, &right)\nprintln(old)\nprintln(left)\nprintln(right)\n}",
    );
    let inputs = analysis.inputs();
    let lower = |inputs: &[_]| {
        lower_scalar_unit_with_entry(
            &analysis.sources,
            inputs,
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
        )
        .expect("cross-file ownership primitive SSA")
    };
    let (program, entry) = lower(&inputs);
    let (reversed, reversed_entry) = lower(&[inputs[1], inputs[0]]);
    assert_eq!(
        crate::ssa::render_program(&program),
        crate::ssa::render_program(&reversed)
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    assert_eq!(
        llvm,
        crate::llvm::render_verified_program_with_entry(&reversed, reversed_entry).unwrap()
    );
    let output = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 3);
    crate::native_tests::boxed_enum_tests::assert_success(&output, b"make\nleft!\nright!\nnew!\n");
}

#[test]
fn unit_root_primitive_native_control_transfer_preserves_old_root() {
    for exit in ["return", "break", "continue"] {
        let provider = format!(
            "package p\nfun exercise(own flag: Boolean): Unit {{\nvar target = \"old\" + \"!\"\nvar once = true\nloop {{\nif (!once) {{ break }}\nonce = false\nval old = replace(&target, if (flag) {{ {exit} }} else {{ \"new\" + \"!\" }})\nprintln(old)\nbreak\n}}\nprintln(target)\n}}"
        );
        for flag in ["true", "false"] {
            let consumer =
                format!("package q\nfun entry(): Unit {{ p.exercise({flag})\nprintln(\"done\") }}");
            let run = run_constant_sources(&provider, &consumer);
            assert!(run.status.success(), "{exit}/{flag}: {run:?}");
            let expected = if flag == "false" {
                "old!\nnew!\ndone\n"
            } else if exit == "return" {
                "done\n"
            } else {
                "old!\ndone\n"
            };
            assert_eq!(run.stdout, expected.as_bytes(), "{exit}/{flag}: {run:?}");
            assert!(run.stderr.is_empty(), "{exit}/{flag}: {run:?}");
        }
    }
}

#[test]
fn unit_root_primitive_native_copyable_aliases_and_unit_roots() {
    let run = run_constant_sources(
        "package p\nfun make(): Unit {}",
        "package q\nfun entry(): Unit {\nvar left = 7\nvar right = left\nval source = right\nval old = replace(&left, source)\nswap(&left, &right)\nif (old + left + right != 21) { error(\"bad scalar exchange\") }\nvar first = p.make()\nvar second = p.make()\nval previous = replace(&first, p.make())\nswap(&first, &second)\nprintln(\"done\")\n}",
    );
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"done\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn unit_root_primitive_native_abort_never_reaches_commit_continuation() {
    let run = run_constant_sources(
        "package p\nconst val MESSAGE = \"stop\"",
        "package q\nfun entry(): Unit {\nvar target = \"old\" + \"!\"\nval unused = replace(&target, error(p.MESSAGE))\nprintln(\"unexpected continuation\")\n}",
    );
    assert!(!run.status.success(), "Nothing must abort: {run:?}");
    assert!(run.stdout.is_empty(), "{run:?}");
}

#[test]
fn unit_root_primitive_native_nested_copyable_and_unit_loop_prefixes() {
    let provider = "package p\nfun make(): Unit {}\nfun scalar(own flag: Boolean): Int {\nvar target = 7\nval old = replace(&target, if (flag) { loop { break }\n9 } else { 11 })\nreturn old + target\n}\nfun unitRoot(own flag: Boolean): Unit {\nvar target = make()\nloop {\nval old = replace(&target, if (flag) { break } else { make() })\nbreak\n}\ntarget\nprintln(\"unit\")\n}";
    for (flag, result) in [("true", 16), ("false", 18)] {
        let consumer = format!(
            "package q\nfun entry(): Unit {{\nif (p.scalar({flag}) != {result}) {{ error(\"bad pending root\") }}\np.unitRoot({flag})\nprintln(\"done\")\n}}"
        );
        let run = run_constant_sources(provider, &consumer);
        assert!(run.status.success(), "{flag}: {run:?}");
        assert_eq!(run.stdout, b"unit\ndone\n", "{flag}: {run:?}");
        assert!(run.stderr.is_empty(), "{flag}: {run:?}");
    }
}
