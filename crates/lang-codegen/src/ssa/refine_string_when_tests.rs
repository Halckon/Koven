//! SPEC-0360: real Unit sources consume the SPEC-0357 comparison/edge facts.
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    model::{Operation, Program},
    refine::observe_routing,
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

/// Same source in a single unit and a two-source unit, both source orders. Each product
/// passes the public Unit qualification, complete SSA verifier and LLVM verifier.
fn lower(source: &str) -> (Program, String) {
    let mut sources = SourceMap::new();
    let text = format!("package p\n{source}");
    let (id, file) = parsed(&mut sources, "p/main.ko", &text);
    let (other_id, other) = parsed(
        &mut sources,
        "q/other.ko",
        "package q\nfun unrelated(): Int = 7",
    );
    let main = SourceUnitInput::new("root", "p/main.ko", id, &file);
    let other = SourceUnitInput::new("root", "q/other.ko", other_id, &other);
    let mut last = None;
    let mut ordered = None;
    for inputs in [vec![main], vec![main, other], vec![other, main]] {
        let (ne, te) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
        let entry = declaration(&names, "p", "main");
        let (result, routed) = observe_routing(|| {
            lower_scalar_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, entry)
        });
        let (program, entry) = result.unwrap_or_else(|error| panic!("{source}: {error:?}"));
        assert!(
            !routed.is_empty(),
            "String when must use refinement: {source}"
        );
        let rendered = render_program(&program);
        if inputs.len() == 2 {
            if let Some(expected) = &ordered {
                assert_eq!(expected, &rendered, "source-order determinism");
            }
            ordered = Some(rendered);
        }
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
            .expect("String when LLVM verifies");
        last = Some((program, llvm));
    }
    last.expect("three input shapes")
}

fn counted(source: &str, allocations: usize, stdout: &[u8]) {
    let (_, llvm) = lower(source);
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, allocations);
    crate::native_tests::boxed_enum_tests::assert_success(&run, stdout);
}

#[test]
fn p41_original_source_runs_through_refinement() {
    let source = include_str!("../../../../conformance/probes/p41_when_string.ko");
    lower(source);
    let run = crate::native_tests::emit_link_and_run("p41.ko", source, "main");
    crate::native_tests::boxed_enum_tests::assert_success(&run, b"when-str\n");
}

#[test]
fn temporary_subject_early_late_miss_and_grouped_candidates_free_once() {
    for (subject, expected) in [("a", "early"), ("b", "late"), ("z", "miss")] {
        let source = format!(
            r#"fun make(): String {{ println("subject"); return "{subject}" + "!" }}
        fun main(): Unit {{
            val n = when ((make())) {{ (("a!")), "c!" -> 1; "b!" -> 2; else -> 0 }}
            if (n == 1) {{ println("early") }}
            if (n == 2) {{ println("late") }}
            if (n == 0) {{ println("miss") }}
        }}"#
        );
        counted(&source, 1, format!("subject\n{expected}\n").as_bytes());
    }
}

#[test]
fn named_subject_stays_live_for_body_and_after_when() {
    for value in ["a", "z"] {
        let source = format!(
            r#"fun main(): Unit {{
            val s = "{value}" + "!"
            when ((s)) {{ "a!" -> println(s); else -> println(s) }}
            println(s)
        }}"#
        );
        counted(&source, 1, format!("{value}!\n{value}!\n").as_bytes());
    }
}

#[test]
fn no_else_and_zero_comparison_paths_free_temporary_once() {
    for body in [
        r#"when ("z" + "!") { "a!", "b!" -> println("unreachable") }; println("done")"#,
        r#"when (("z" + "!")) { else -> println("done") }"#,
        r#"when ("z" + "!") {}; println("done")"#,
    ] {
        counted(&format!("fun main(): Unit {{ {body} }}"), 1, b"done\n");
    }
}

#[test]
fn borrowed_subject_uses_callers_loan_without_owner_drop() {
    let source = r#"fun select(s: String): Int = when ((s)) { "a!", "b!" -> 1; else -> 0 }
    fun main(): Unit {
        val s = "b" + "!"
        if (select(s) == 1) { println("match") }
        println(s)
        if (select("z" + "!") == 0) { println("miss") }
    }"#;
    let (program, _) = lower(source);
    let select = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.starts_with("koven.p.select.d"))
        .expect("select");
    assert_eq!(
        select
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        2,
        "only the two literal candidates are dropped; the subject is a loan"
    );
    assert!(
        select
            .instructions
            .iter()
            .all(|instruction| !matches!(instruction.operation, Operation::Read { .. })),
        "a String loan cannot be read into an owned value"
    );
    counted(source, 2, b"match\nb!\nmiss\n");
}

#[test]
fn body_return_break_and_continue_do_not_repeat_subject_cleanup() {
    let source = r#"fun choose(): Int {
        when ("b" + "!") { "a!" -> return 0; "b!" -> return 1 }; return 2
    }
    fun main(): Unit {
        if (choose() == 1) { println("return") }
        var i = 0
        loop {
            i = i + 1
            when ("b" + "!") { "b!" -> { if (i == 1) continue else break } }
        }
        println("loop")
    }"#;
    counted(source, 3, b"return\nloop\n");
}

#[test]
fn each_evaluated_candidate_drops_after_comparison_and_suffix_is_not_evaluated() {
    use super::model::{EntityId, SsaTypeKind};
    use std::{fs, process::Command};
    // Three distinct source candidates; early, late and no-match execute 1/3/3.
    for (subject, comparisons, expected) in [("a", 1, 1), ("b", 3, 2), ("z", 3, 0)] {
        let source = format!(
            r#"fun main(): Unit {{
            val n = when ("{subject}" + "!") {{ "a!", "c!" -> 1; "b!" -> 2; else -> 0 }}
            if (n == {expected}) {{ println("ok") }}
        }}"#
        );
        let (program, llvm) = lower(&source);
        let function = program.modules[0]
            .functions
            .iter()
            .find(|function| function.name.starts_with("koven.p.main.d"))
            .unwrap();
        let mut sites = 0;
        for block in &function.blocks {
            for (index, id) in block.instructions.iter().enumerate() {
                let instruction = function.instruction(*id).unwrap();
                if let Operation::StringEqual {
                    right: EntityId::Value(candidate),
                    ..
                } = instruction.operation
                {
                    sites += 1;
                    let next = function.instruction(block.instructions[index + 1]).unwrap();
                    assert!(
                        matches!(next.operation, Operation::Drop { .. }),
                        "Completed drops the candidate immediately after equality"
                    );
                    // The precise owner ID is printed in debug output on failure; the
                    // complete verifier additionally rejects any read after this drop.
                    assert!(
                        matches!(next.operation, Operation::Drop { owner } if owner == candidate)
                    );
                }
            }
        }
        assert_eq!(sites, 3, "one comparison per source candidate");
        let string = program.modules[0]
            .types
            .iter()
            .position(|ty| *ty == SsaTypeKind::StringOwner)
            .unwrap();
        let mut instrumented = String::new();
        let mut drop_sites = 0;
        for line in llvm.lines() {
            instrumented.push_str(line);
            instrumented.push('\n');
            if line.contains(&format!("call void @koven.drop.t{string}(")) {
                instrumented.push_str("  call void @counted_drop()\n");
                drop_sites += 1;
            }
        }
        assert!(drop_sites > 0);
        instrumented.push_str("declare void @counted_drop()\n");
        let directory = crate::native_tests::TestDirectory::create();
        let ir = directory.join("when.ll");
        let counter = directory.join("counter.c");
        let executable = directory.join("when");
        fs::write(&ir, instrumented).unwrap();
        // Two concat operands, one subject, evaluated candidates, and the output literal.
        fs::write(
            &counter,
            format!(
                r#"#include <assert.h>
static int drops;
void counted_drop(void) {{ ++drops; }}
__attribute__((destructor)) static void verify(void) {{ assert(drops == {}); }}
"#,
                comparisons + 4
            ),
        )
        .unwrap();
        let linked = Command::new(crate::test_support::ir_clang())
            .arg(&ir)
            .arg(&counter)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        let run = Command::new(executable).output().unwrap();
        crate::native_tests::boxed_enum_tests::assert_success(&run, b"ok\n");
    }
}

#[test]
fn named_owner_cleanup_follows_selected_body_liveness() {
    for (value, expected) in [("a", "a!"), ("z", "miss")] {
        let source = format!(
            r#"fun main(): Unit {{
            val s = "{value}" + "!"
            when (s) {{ "a!" -> println(s); else -> println("miss") }}
        }}"#
        );
        counted(&source, 1, format!("{expected}\n").as_bytes());
    }
}

#[test]
fn borrowed_subject_cross_source_call_verifies_and_preserves_caller_owner() {
    let mut sources = SourceMap::new();
    let (provider_id, provider) = parsed(
        &mut sources,
        "p/select.ko",
        "package p\nfun select(s: String): Int = when (s) { \"b!\" -> 1; else -> 0 }",
    );
    let (consumer_id, consumer) = parsed(
        &mut sources,
        "q/main.ko",
        r#"package q
    fun main(): Unit {
        val s = "b" + "!"
        if (p.select(s) == 1) { println(s) }
        println(s)
    }"#,
    );
    let inputs = [
        SourceUnitInput::new("root", "p/select.ko", provider_id, &provider),
        SourceUnitInput::new("root", "q/main.ko", consumer_id, &consumer),
    ];
    let mut expected = None;
    for inputs in [inputs, [inputs[1], inputs[0]]] {
        let (ne, te) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
        let entry = declaration(&names, "q", "main");
        let (program, entry) =
            lower_scalar_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, entry)
                .expect("source-qualified borrowed subject lowers");
        let rendered = render_program(&program);
        if let Some(expected) = &expected {
            assert_eq!(&rendered, expected);
        }
        expected = Some(rendered);
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 1);
        crate::native_tests::boxed_enum_tests::assert_success(&run, b"b!\nb!\n");
    }
}
