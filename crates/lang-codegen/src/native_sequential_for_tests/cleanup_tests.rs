//! SPEC-0182: native temporary Array<String> cleanup across four exit paths.
use super::super::{SymbolKind, UnitSource, boxed_enum_tests, emit_link_and_run};
use crate::test_support::diagnostics::diagnostic;

fn run_cleanup(case: &str, body: &str, expected: &[u8], allocations: usize) {
    let text = format!(
        r#"
        class Guard(val name: String) {{ deinit() {{ println(this.name) }} }}
        fun source(): Array<String> = arrayOf("fi" + "rst", "sec" + "ond")
        fun operand(item: String): Int {{ println(item); return 7 }}
        fun run(): Int {{
            for (item in source()) {{
                val first = Guard("earlier")
                val second = Guard("later")
                {body}
            }}
            println("after")
            return 0
        }}
        fun entry(): Unit {{
            val result = run()
            if (result == 7) {{ println("returned") }}
            println("done")
        }}
    "#
    );
    let source_name = format!("temporary_cleanup_{case}.ko");
    diagnostic!("{source_name}:\n{text}");
    let run = emit_link_and_run(&source_name, &text, "entry");
    boxed_enum_tests::assert_success(&run, expected);

    let analysis = UnitSource::analyze(&source_name, &text)
        .expect("original source must pass strict Unit admission");
    analysis.assert_frontend_clean();
    let (program, entry) = analysis
        .lower_entry(analysis.declaration("entry", SymbolKind::Function))
        .expect("supported String provider with body-local resource guards");
    let ir = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let counted = boxed_enum_tests::run_counted_allocations(&ir, allocations);
    boxed_enum_tests::assert_success(&counted, expected);
}

// Fixed stdout and allocation counts come from these source-level cases, not emitted IR:
// two nonempty concat buffers + one container buffer + two Guard payloads per visited body.
// String literals do not allocate; libc stdout allocations are outside the existing hook.
#[test]
fn normal_exit_drops_body_guards_and_temporary_buffers_once() {
    run_cleanup(
        "normal",
        "println(item)",
        b"first\nlater\nearlier\nsecond\nlater\nearlier\nafter\ndone\n",
        7,
    );
}

#[test]
fn continue_exit_preserves_temporary_buffers_until_exhaustion() {
    run_cleanup(
        "continue",
        "println(item); continue",
        b"first\nlater\nearlier\nsecond\nlater\nearlier\nafter\ndone\n",
        7,
    );
}

#[test]
fn break_exit_drops_unvisited_element_buffer_too() {
    run_cleanup(
        "break",
        "println(item); break",
        b"first\nlater\nearlier\nafter\ndone\n",
        5,
    );
}

#[test]
fn return_operand_runs_before_guards_and_source_cleanup() {
    run_cleanup(
        "return",
        "return operand(item)",
        b"first\nlater\nearlier\nreturned\ndone\n",
        5,
    );
}

#[test]
fn local_destructuring_and_temporary_list_release_each_owner_on_all_exits() {
    let source = r#"
        value class Pair(val first: Int, val second: Int)
        value class Entry(val index: Int, val label: String)
        fun source(): List<Entry> {
            println("source")
            return listOf(Entry(1, "fi" + "rst"), Entry(2, "sec" + "ond"), Entry(3, "th" + "ird"))
        }
        fun run(mode: Int): Int {
            val (second, first) = Pair(1, 2)
            for ((index, label) in source()) {
                println(label)
                if (index == first) { continue }
                if (mode == 1) { break }
                if (mode == 2) { return second }
            }
            println("after")
            return 0
        }
        fun entry(): Unit {
            run(0)
            run(1)
            if (run(2) == 2) { println("returned") }
        }
    "#;
    let name = "destructuring_temporary_list_cleanup.ko";
    let analysis = UnitSource::analyze(name, source).expect("source passes Unit admission");
    analysis.assert_frontend_clean();
    let (program, entry) = analysis
        .lower_entry(analysis.declaration("entry", SymbolKind::Function))
        .expect("destructuring and temporary iteration lower together");
    let ir = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    // Each invocation has three concatenated strings and one list buffer, even on early exit.
    let run = boxed_enum_tests::run_counted_allocations(&ir, 12);
    boxed_enum_tests::assert_success(
        &run,
        b"source\nfirst\nsecond\nthird\nafter\nsource\nfirst\nsecond\nafter\nsource\nfirst\nsecond\nreturned\n",
    );
}

#[test]
fn conditional_break_rebinds_source_and_drops_unvisited_buffer_once() {
    run_cleanup(
        "conditional_break",
        "println(item); if (item == \"first\") { break } else { continue }",
        b"first\nlater\nearlier\nafter\ndone\n",
        5,
    );
}

#[test]
fn nested_conditional_break_keeps_source_across_normal_sibling() {
    run_cleanup(
        "nested_conditional_break",
        "println(item); if (item == \"second\") { if (item == \"second\") { break } }",
        b"first\nlater\nearlier\nsecond\nlater\nearlier\nafter\ndone\n",
        7,
    );
}
