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

pub(crate) mod conditional_owned_source_tests {
    //! SPEC-0312 ordinary unmodified object/link/run. Output is not a free-count oracle.
    use super::super::super::{boxed_enum_tests::assert_success, emit_link_and_run};

    #[test]
    fn owned_source_continue_native_provider_size_path_liveness_matrix() {
        for (container, factory) in [
            ("Array", "arrayOf"),
            ("List", "listOf"),
            ("MutableList", "mutableListOf"),
        ] {
            for size in [0, 1, 3] {
                for stop in [0, 1, 2] {
                    for live in [false, true] {
                        let values = (1..=size)
                            .map(|n| n.to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        // stop=0 never, stop=1 every iteration, stop=2 mixed for size=3.
                        let condition = if stop == 1 {
                            "key > 0".to_owned()
                        } else {
                            format!("key == {stop}")
                        };
                        let after = if live {
                            format!("if (keys.size == {size}) {{ println(\"live\") }}")
                        } else {
                            String::new()
                        };
                        let source = format!(
                            "fun source(): {container}<Int> {{ println(\"seed\"); return {factory}<Int>({values}) }}\nfun main(): Unit {{ var keys = source(); for (key in keys) {{ println(\"visit\"); if ({condition}) {{ continue }}; println(\"tail\") }}; {after}; println(\"done\") }}"
                        );
                        let mut expected = "seed\n".to_owned();
                        for key in 1..=size {
                            expected.push_str("visit\n");
                            if !(stop == 1 || key == stop) {
                                expected.push_str("tail\n");
                            }
                        }
                        if live {
                            expected.push_str("live\n");
                        }
                        expected.push_str("done\n");
                        eprintln!("{container} size={size} policy={stop} live={live}");
                        assert_success(
                            &emit_link_and_run("owned-source-continue.ko", &source, "main"),
                            expected.as_bytes(),
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn owned_source_continue_native_body_cleanup_and_post_loop_permission() {
        let text = r#"
class Token(val name: String) { deinit() { println(this.name) } }
fun consume(own source: Array<Int>): Unit { if (source.size == 3) { println("moved") } }
fun main(): Unit {
    var keys = arrayOf(1, 2, 3)
    for (key in keys) {
        val first = Token("first")
        val second = Token("second")
        if (key == 2) { continue }
        println("tail")
    }
    consume(keys)
    var list = mutableListOf(1, 2, 3)
    for (key in list) { if (key == 2) { continue } }
    list.add(4)
    list.removeLast()
    if (list.size == 3) { println("mutated") }
}
"#;
        assert_success(
            &emit_link_and_run("owned-source-cleanup.ko", text, "main"),
            b"tail\nsecond\nfirst\nsecond\nfirst\ntail\nsecond\nfirst\nmoved\nmutated\n",
        );
    }

    #[test]
    fn owned_source_continue_native_owned_parameter_string_and_borrow_controls() {
        let text = r#"
fun owned(own keys: List<String>): Unit {
    for (key in keys) { println(key); if (key == "two") { continue }; println("tail") }
    if (keys.size == 3) { println("owned") }
}
fun borrowed(keys: Array<String>): Unit {
    for (key in keys) { println(key); if (key == "two") { continue }; println("tail") }
}
fun caller(): Unit {
    val keys = arrayOf("one", "two", "three")
    borrowed(keys)
    if (keys.size == 3) { println("borrowed") }
}
fun temporary(): Unit {
    for (key in arrayOf("one", "two", "three")) {
        println(key)
        if (key == "two") { continue }
        println("tail")
    }
    println("temporary")
}
fun main(): Unit { owned(listOf("one", "two", "three")); caller(); temporary() }
"#;
        assert_success(&emit_link_and_run("owned-source-modes.ko", text, "main"),
        b"one\ntail\ntwo\nthree\ntail\nowned\none\ntail\ntwo\nthree\ntail\nborrowed\none\ntail\ntwo\nthree\ntail\ntemporary\n");
    }

    #[test]
    fn owned_source_continue_native_moveonly_destructure_and_discard() {
        let text = r#"
value class Item(val id: Int, val name: String)
fun destructured(): Unit {
    val items = listOf(Item(1, "one"), Item(2, "two"), Item(3, "three"))
    for ((id, name) in items) { println(name); if (id == 2) { continue }; println("tail") }
    if (items.size == 3) { println("retained") }
}
fun discarded(): Unit {
    val keys = arrayOf(1, 2, 3)
    for (_ in keys) { if (keys.size == 3) { continue }; println("wrong") }
    println("done")
}
fun main(): Unit { destructured(); discarded() }
"#;
        assert_success(
            &emit_link_and_run("owned-source-destructure.ko", text, "main"),
            b"one\ntail\ntwo\nthree\ntail\nretained\ndone\n",
        );
    }

    #[test]
    fn owned_source_continue_native_fresh_inner_dynamic_instances() {
        assert_success(
        &emit_link_and_run("fresh-inner.ko", include_str!("../../tests/fixtures/owned-source-continue-0312/fresh-inner.ko"), "main"),
        b"first\nbody\ntail\nlive\nsecond\nbody\ntail\nlive\nthird\nbody\ntail\nfourth\nbody\ntail\n",
    );
    }

    #[test]
    fn owned_source_continue_native_original_map_fixture() {
        assert_success(
        &emit_link_and_run("providers-cleanup.ko", include_str!("../../tests/fixtures/map-conditional-continue-0311/providers-cleanup.ko"), "main"),
        b"tail\ntail\nnamedArray\nsource kept\ntail\ntail\ntemporaryArray\ntail\ntail\nnamedList\nsource kept\ntail\ntail\ntemporaryList\ntail\ntail\nnamedMutableList\nsource kept\ntail\ntail\ntemporaryMutableList\n",
    );
    }

    #[test]
    fn owned_source_continue_native_map_and_source_liveness() {
        for map_live in [false, true] {
            for source_live in [false, true] {
                eprintln!("Map/source liveness map={map_live}, source={source_live}");
                let text = format!(
                    "fun main(): Unit {{ val keys = arrayOf(1, 2, 3); var first = mutableMapOf<Int, String>(); var second = mutableMapOf<Int, String>(); for (key in keys) {{ first.put(key, \"first\".clone()); second.put(key, \"second\".clone()); if (key == 2) {{ continue }}; first.put(9, \"tail one\".clone()); second.put(9, \"tail two\".clone()); println(\"tail\") }}; {}; {} }}",
                    if map_live {
                        "if (first.contains(3)) { println(\"first map\") }; if (second.contains(3)) { println(\"second map\") }"
                    } else {
                        ""
                    },
                    if source_live {
                        "if (keys.size == 3) { println(\"source\") }"
                    } else {
                        ""
                    },
                );
                let expected = format!(
                    "tail\ntail\n{}{}",
                    if map_live {
                        "first map\nsecond map\n"
                    } else {
                        ""
                    },
                    if source_live { "source\n" } else { "" }
                );
                assert_success(
                    &emit_link_and_run("map-source-liveness.ko", &text, "main"),
                    expected.as_bytes(),
                );
            }
        }
    }

    #[test]
    fn owned_source_continue_native_resource_source_old_unconditional_path() {
        let source = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
fun main(): Unit {
    val keys = arrayOf(Leaf("first"), Leaf("second"))
    for (key in keys) { println("visit"); continue }
    println("done")
}
"#;
        assert_success(
            &emit_link_and_run("resource-source-old-path.ko", source, "main"),
            b"visit\nvisit\ndone\nsecond\nfirst\n",
        );
    }
}
