//! SPEC-0487 (F4c expansion): Exhaustive AST-level Formatter round-trip differential
//! tests covering all complete-file pass fixtures plus SPEC-0479 / F3c / F3d / F3e
//! constructs and semantic-mutation fault controls.

use std::fs;

use lang_frontend::formatting::format_source;

use super::{
    collect_ko_files, fixture_root,
    full_projector::{first_difference, project_full},
    is_complete_file_pass, loaded_source,
};

fn assert_roundtrip_preserves_full_ast(label: &str, source: &str) {
    let (sources, id) = loaded_source(label, source);
    let formatted = format_source(&sources, id)
        .unwrap_or_else(|e| panic!("{label}: format_source failed: {e:?}"));
    let orig_proj =
        project_full(source).unwrap_or_else(|e| panic!("{label} original projection failed: {e}"));
    let fmt_proj = project_full(&formatted)
        .unwrap_or_else(|e| panic!("{label} formatted projection failed: {e}\n{formatted}"));
    if let Some((before, after)) = first_difference(&orig_proj, &fmt_proj) {
        panic!("{label}: AST round-trip drift:\n  original:  {before:?}\n  formatted: {after:?}");
    }
    let (fmt_sources, fmt_id) = loaded_source("second.ko", &formatted);
    let second = format_source(&fmt_sources, fmt_id).expect("second format");
    assert_eq!(second, formatted, "{label}: formatter not idempotent");
}

#[test]
fn full_ast_roundtrip_covers_every_complete_file_pass_fixture() {
    let mut paths = Vec::new();
    collect_ko_files(&fixture_root(), &mut paths);
    paths.retain(|p| is_complete_file_pass(p));
    paths.sort();
    assert!(
        paths.len() >= 15,
        "expected >= 15 complete-file pass fixtures, found {}",
        paths.len()
    );
    for path in paths {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let label = path.to_string_lossy();
        assert_roundtrip_preserves_full_ast(&label, &source);
    }
}

#[test]
fn full_ast_roundtrip_preserves_spec_0479_and_f3c_f3d_f3e_constructs() {
    let cases = [
        (
            "spec-0479-explicit-block-and-trailing-lambdas",
            "\
fun   run(flag: Boolean,   other: Boolean) {
    block   {
        val x = 1
        html   {   render(x)   }
    }
    val a = f()
    {   x()   }
    val b = f() // keep comment
    {   work()   }
    val c = f() /* keep block comment */
    {   work()   }
    when {
        flag -> f()
        { -> true }() && flag -> g()
        else -> ({   fallback()   })
    }
    val cb: move (own Int) -> Int = move { n -> n + 1 }
}
",
        ),
        (
            "f3c-operators-strings-and-calls",
            "\
fun   ops(a: Int, b: Int, opt: Int?, xs: List<Int>): Int {
    val bits = a   shl   1   and   b   xor   2   or   3
    val rng = a..b   to   (opt ?: 0)
    val msg = \"v=${bits} ok\"
    val refFn = ::ops
    return   compute(name = a, &b, xs[0]?.next!!, refFn)
}
",
        ),
        (
            "f3e-classifiers-deinit-and-borrow-return",
            "\
package   acme.demo
import   std.io.*
import   std.text.format   as   fmt

public class   Box(val inner: Shape) : Shape by inner {
    public override inout fun draw(): Unit {}
    fun   peek(borrow x: Int): borrow Int from x = x
    deinit()   {}
    private companion   {
        const val VERSION: Int = 1
    }
}
enum class   Color : Named {
    Red,
    Rgb(r: Int, g: Int, b: Int);
    fun   code(): Int = 1
}
",
        ),
    ];

    for (label, source) in cases {
        assert_roundtrip_preserves_full_ast(label, source);
    }
}

#[test]
fn full_projection_fault_controls_detect_semantic_mutations_and_ignore_offsets() {
    let baseline_src = "\
package demo.core
import std.io.println as emit
public class Box(val inner: Shape) : Shape by inner {
    public override borrow fun peek(x: Int): borrow Int from x = x
    fun run(flag: Boolean, opt: Int?): Int {
        if (flag) {
            block { emit(opt?.next!! as Int) }
        }
        val r = f() { a -> a + 1 }
        return when {
            flag -> 1
            else -> 0
        }
    }
}
";
    let baseline = project_full(baseline_src).expect("baseline projects cleanly");
    assert_eq!(
        project_full(&format!("// leading offset shift\n\n{baseline_src}")).unwrap(),
        baseline
    );

    let mutations = [
        ("public class", "private class", "root[0].mods.visibility"),
        ("by inner", "by other", "root[0].decl.super[0].by"),
        (
            "borrow fun peek",
            "inout fun peek",
            "root[0].decl.member[0].mods.receiver_mode",
        ),
        (
            "from x = x",
            "from y = x",
            "root[0].decl.member[0].decl.form.source.target",
        ),
        (
            "opt: Int?",
            "opt: Int",
            "root[0].decl.member[1].param[1].type.nullable",
        ),
        (
            "opt?.next",
            "opt.next",
            "root[0].decl.member[1].form.body.block.el[0].expr.then.el[0].el[0].expr.arg[0].val.expr.operand.member",
        ),
        (
            "as Int",
            "as? Int",
            "root[0].decl.member[1].form.body.block.el[0].expr.then.el[0].el[0].expr.arg[0].val.cast",
        ),
        (
            "a + 1",
            "a - 1",
            "root[0].decl.member[1].form.body.block.el[1].decl.init.arg[0].val.body.el[0].expr.bin",
        ),
    ];

    for (from, to, expected_path) in mutations {
        let mutated_src = baseline_src.replacen(from, to, 1);
        assert_ne!(mutated_src, baseline_src, "mutation `{from}` must match");
        let mutated = project_full(&mutated_src)
            .unwrap_or_else(|e| panic!("mutated `{from}` -> `{to}` failed to project: {e}"));
        let (left, _) = first_difference(&baseline, &mutated)
            .unwrap_or_else(|| panic!("full projection missed mutation `{from}` -> `{to}`"));
        assert_eq!(left.unwrap().0, expected_path);
    }

    // Detached trailing lambda vs attached trailing lambda must differ.
    let attached = project_full("fun f() { g()\n{ x() } }\n").unwrap();
    let grouped_separate = project_full("fun f() { g()\n({ x() }) }\n").unwrap();
    assert!(first_difference(&attached, &grouped_separate).is_some());

    // Error/recovery input must be rejected by project_full.
    assert!(project_full("fun f() { { x() } }\n").is_err());
}
