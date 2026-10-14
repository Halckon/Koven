//! Ordinary source only: original LLVM, object, link and native execution.
use crate::native_tests::{boxed_enum_tests::assert_success, emit_link_and_run};

fn matrix(factory: &str, ty: &str, own: bool, live: bool, map: bool) {
    let mut source = String::new();
    let mut calls = String::new();
    let mut expected = String::new();
    for count in [0, 1, 3] {
        let items = (1..=count)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        source.push_str(&format!("fun make{count}(): {ty}<Int> {{ println(\"factory\"); return {factory}<Int>({items}) }}\n"));
        let parameter = if own {
            format!("own keys: {ty}<Int>, ")
        } else {
            String::new()
        };
        let seed = if own {
            String::new()
        } else {
            format!("val keys = make{count}();")
        };
        let before = if map {
            "var cache = mutableMapOf<Int, String>();"
        } else {
            ""
        };
        let update = if map {
            "cache.put(key, \"latest\".clone());"
        } else {
            ""
        };
        let tail = if live {
            format!(
                "if (keys.size == {count}) {{ println(\"live\") }} else {{ println(\"wrong\") }};"
            )
        } else {
            String::new()
        };
        let map_tail = if map {
            "if (cache.contains(1)) { println(\"present\") } else { println(\"empty\") };"
        } else {
            ""
        };
        source.push_str(&format!("fun work{count}({parameter}stop: Int): Unit {{ {seed} {before} for (key in keys) {{ {update} println(\"head\"); if (key == stop) {{ println(\"return\"); return }}; println(\"tail\") }}; {tail} {map_tail} println(\"done\") }}\n"));
        for stop in [0, 1, 3] {
            let argument = if own {
                format!("make{count}(), ")
            } else {
                String::new()
            };
            calls.push_str(&format!("work{count}({argument}{stop});"));
            expected.push_str("factory\n");
            let mut returned = false;
            for key in 1..=count {
                expected.push_str("head\n");
                if key == stop {
                    expected.push_str("return\n");
                    returned = true;
                    break;
                }
                expected.push_str("tail\n");
            }
            if !returned {
                if live {
                    expected.push_str("live\n");
                }
                if map {
                    expected.push_str(if count == 0 { "empty\n" } else { "present\n" });
                }
                expected.push_str("done\n");
            }
        }
    }
    source.push_str(&format!("fun main(): Unit {{ {calls} }}"));
    let run = emit_link_and_run("owned-source-return-cardinality.ko", &source, "main");
    assert_success(&run, expected.as_bytes());
}

#[test]
fn owned_source_return_native_array_local_dead_no_map() {
    matrix("arrayOf", "Array", false, false, false);
}

#[test]
fn owned_source_return_native_array_local_dead_map() {
    matrix("arrayOf", "Array", false, false, true);
}

#[test]
fn owned_source_return_native_array_local_live_no_map() {
    matrix("arrayOf", "Array", false, true, false);
}

#[test]
fn owned_source_return_native_array_local_live_map() {
    matrix("arrayOf", "Array", false, true, true);
}

#[test]
fn owned_source_return_native_array_own_dead_no_map() {
    matrix("arrayOf", "Array", true, false, false);
}

#[test]
fn owned_source_return_native_array_own_dead_map() {
    matrix("arrayOf", "Array", true, false, true);
}

#[test]
fn owned_source_return_native_array_own_live_no_map() {
    matrix("arrayOf", "Array", true, true, false);
}

#[test]
fn owned_source_return_native_array_own_live_map() {
    matrix("arrayOf", "Array", true, true, true);
}

#[test]
fn owned_source_return_native_list_local_dead_no_map() {
    matrix("listOf", "List", false, false, false);
}

#[test]
fn owned_source_return_native_list_local_dead_map() {
    matrix("listOf", "List", false, false, true);
}

#[test]
fn owned_source_return_native_list_local_live_no_map() {
    matrix("listOf", "List", false, true, false);
}

#[test]
fn owned_source_return_native_list_local_live_map() {
    matrix("listOf", "List", false, true, true);
}

#[test]
fn owned_source_return_native_list_own_dead_no_map() {
    matrix("listOf", "List", true, false, false);
}

#[test]
fn owned_source_return_native_list_own_dead_map() {
    matrix("listOf", "List", true, false, true);
}

#[test]
fn owned_source_return_native_list_own_live_no_map() {
    matrix("listOf", "List", true, true, false);
}

#[test]
fn owned_source_return_native_list_own_live_map() {
    matrix("listOf", "List", true, true, true);
}

#[test]
fn owned_source_return_native_mutable_list_local_dead_no_map() {
    matrix("mutableListOf", "MutableList", false, false, false);
}

#[test]
fn owned_source_return_native_mutable_list_local_dead_map() {
    matrix("mutableListOf", "MutableList", false, false, true);
}

#[test]
fn owned_source_return_native_mutable_list_local_live_no_map() {
    matrix("mutableListOf", "MutableList", false, true, false);
}

#[test]
fn owned_source_return_native_mutable_list_local_live_map() {
    matrix("mutableListOf", "MutableList", false, true, true);
}

#[test]
fn owned_source_return_native_mutable_list_own_dead_no_map() {
    matrix("mutableListOf", "MutableList", true, false, false);
}

#[test]
fn owned_source_return_native_mutable_list_own_dead_map() {
    matrix("mutableListOf", "MutableList", true, false, true);
}

#[test]
fn owned_source_return_native_mutable_list_own_live_no_map() {
    matrix("mutableListOf", "MutableList", true, true, false);
}

#[test]
fn owned_source_return_native_mutable_list_own_live_map() {
    matrix("mutableListOf", "MutableList", true, true, true);
}

#[test]
fn owned_source_return_native_operand_cleanup() {
    let run = emit_link_and_run(
        "owned-source-return-cleanup.ko",
        include_str!("../../tests/fixtures/owned-source-return-0314/operand-cleanup.ko"),
        "main",
    );
    assert_success(&run, concat!("first\noperand\nlater\nearlier\nreturned\n", "fallthrough\nlater\nearlier\nfallthrough\nlater\nearlier\ntail kept\nexhausted\ncompleted\n", "copy returned\ncopy exhausted\n").as_bytes());
}

#[test]
fn owned_source_return_native_distinct_maps() {
    let run = emit_link_and_run(
        "owned-source-return-distinct.ko",
        include_str!("../../tests/fixtures/owned-source-return-0314/distinct-owners.ko"),
        "main",
    );
    assert_success(
        &run,
        concat!(
            "parameter empty\nparameter no tail\nparameter missing\n",
            "return\n",
            "return\n",
            "first no last\nfirst no tail\nsecond tail\nfirst after\nsecond after removed\n",
            "second removed\nreturn\n",
            "second removed\nsecond removed\nsecond removed\nreturn\n"
        )
        .as_bytes(),
    );
}

#[test]
fn owned_source_return_native_independent_results() {
    let run = emit_link_and_run(
        "owned-source-return-results.ko",
        include_str!("../../tests/fixtures/owned-source-return-0314/owned-results.ko"),
        "main",
    );
    let one = "returned\ntail\ntail\nreturned\ntail\ntail\ntail\nfallback\n";
    assert_success(&run, one.repeat(4).as_bytes());
}

#[test]
fn owned_source_return_native_post_loop_permissions() {
    let source = r#"
fun consume(own source: Array<Int>): Unit { if (source.size == 3) { println("moved") } }
fun handoff(stop: Int): Unit {
    var keys = arrayOf(1, 2, 3)
    for (key in keys) { if (key == stop) { return } }
    consume(keys)
}
fun mutate(stop: Int): Unit {
    var keys = mutableListOf(1, 2, 3)
    for (key in keys) { if (key == stop) { return } }
    keys.add(4)
    keys.removeLast()
    if (keys.size == 3) { println("mutated") }
}
fun main(): Unit { handoff(2); handoff(9); mutate(2); mutate(9) }
"#;
    assert_success(
        &emit_link_and_run("owned-source-return-permissions.ko", source, "main"),
        b"moved\nmutated\n",
    );
}

#[test]
fn owned_source_return_native_moveonly_patterns() {
    let source = r#"
value class Item(val id: Int, val name: String)
fun destructured(stop: Int): Unit {
    val items = listOf(Item(1, "one"), Item(2, "two"), Item(3, "three"))
    for ((id, name) in items) { println(name); if (id == stop) { return }; println("tail") }
    if (items.size == 3) { println("retained") }
}
fun discarded(stop: Boolean): Unit {
    val keys = arrayOf("one", "two", "three")
    for (_ in keys) { if (stop) { return }; println("tail") }
    println("done")
}
fun main(): Unit { destructured(2); destructured(9); discarded(true); discarded(false) }
"#;
    assert_success(
        &emit_link_and_run("owned-source-return-patterns.ko", source, "main"),
        b"one\ntail\ntwo\none\ntail\ntwo\ntail\nthree\ntail\nretained\ntail\ntail\ntail\ndone\n",
    );
}

#[test]
fn owned_source_return_native_fresh_inner_instances() {
    let mut source = String::new();
    for live in [false, true] {
        let tail = if live {
            "if (keys.size == 2) { println(\"live\") }"
        } else {
            ""
        };
        source.push_str(&format!("fun scan{live}(stop: String): Unit {{ for (outer in arrayOf(\"first\", \"second\")) {{ val keys = arrayOf(outer.clone(), \"tail\"); for (key in keys) {{ println(key); if (key == stop) {{ return }}; println(\"body\") }}; {tail} }} }}\n"));
    }
    source.push_str("fun main(): Unit { scanfalse(\"tail\"); scanfalse(\"never\"); scantrue(\"tail\"); scantrue(\"never\") }");
    assert_success(
        &emit_link_and_run("owned-source-return-fresh.ko", &source, "main"),
        concat!(
            "first\nbody\ntail\n",
            "first\nbody\ntail\nbody\nsecond\nbody\ntail\nbody\n",
            "first\nbody\ntail\n",
            "first\nbody\ntail\nbody\nlive\nsecond\nbody\ntail\nbody\nlive\n"
        )
        .as_bytes(),
    );
}
