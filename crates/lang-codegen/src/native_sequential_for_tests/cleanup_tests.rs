//! SPEC-0182: native temporary Array<String> cleanup across four exit paths.
use super::super::{SymbolKind, analyze, boxed_enum_tests, emit_link_and_run, symbol};

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
    eprintln!("{source_name}:\n{text}");
    let run = emit_link_and_run(&source_name, &text, "entry");
    boxed_enum_tests::assert_success(&run, expected);

    let analysis = analyze(&source_name, &text);
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let (program, entry) = crate::ssa::lower_scalar_file_with_entry(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
        symbol(&analysis, "entry", SymbolKind::Function),
    )
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
