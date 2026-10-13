//! SPEC-0312 ordinary unmodified object/link/run. Output is not a free-count oracle.
use super::super::{boxed_enum_tests::assert_success, emit_link_and_run};

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
