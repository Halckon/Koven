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
