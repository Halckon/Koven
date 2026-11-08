//! Unreachable typed Map iteration must not affect lowering a reachable callable.

use super::emit_link_and_run;

const UNUSED_MAP_ITERATION: &str = r#"
fun unrelated(map: Map<Int, Int>): Unit {
    for ((key, value) in map) { val unused = value }
}
"#;

fn check(source: &str, stdout: &[u8]) {
    // Exercise the historical success case before the otherwise identical source.
    for (path, suffix) in [
        ("with-map-sibling.ko", UNUSED_MAP_ITERATION),
        ("without-map-sibling.ko", ""),
    ] {
        let run = emit_link_and_run(path, &format!("{source}{suffix}"), "main");
        assert!(run.status.success(), "{path}: {run:?}");
        assert_eq!(run.stdout, stdout, "{path}");
        assert!(run.stderr.is_empty(), "{path}: {run:?}");
    }
}

#[test]
fn numeric_range_with_nullable_borrow_parameter_is_independent_of_map_sibling() {
    check(
        r#"
fun scan(s: String?, r: IntRange): Unit {
    for (i in r) { if (i == 1) { println("one") } else { println("two") } }
}
public fun main(): Unit { val none: String? = null; scan(none, 1..2) }
"#,
        b"one\ntwo\n",
    );
}

#[test]
fn map_consume_with_nullable_borrow_parameter_is_independent_of_map_sibling() {
    check(
        r#"
fun scan(s: String?): Unit {
    var map = mutableMapOf<Int, Int>()
    map.put(1, 2)
    val frozen = map.consume()
    if (frozen.size == 1) { println("size") }
}
public fun main(): Unit { val none: String? = null; scan(none) }
"#,
        b"size\n",
    );
}

#[test]
fn delegated_receiver_with_nullable_borrow_parameter_is_independent_of_map_sibling() {
    check(
        r#"
interface Read { fun value(): Int }
class Reader: Read { override fun value(): Int = 7 }
class Wrapper(val reader: Reader): Read by reader
fun scan(s: String?, wrapper: Wrapper): Unit {
    if (wrapper.value() == 7) { println("delegate") }
}
public fun main(): Unit { val none: String? = null; scan(none, Wrapper(Reader())) }
"#,
        b"delegate\n",
    );
}

#[test]
fn numeric_range_with_loop_reassigns_nullable_owner_is_independent_of_map_sibling() {
    check(
        r#"
fun scan(r: IntRange): Unit {
    var outer: String? = "first"
    if (outer != null) {
        var i = 0
        while (i < 2) {
            outer = "second"
            break
        }
    }
    for (i in r) { if (i == 1) { println("one") } else { println("two") } }
}
public fun main(): Unit { scan(1..2) }
"#,
        b"one\ntwo\n",
    );
}

#[test]
fn map_consume_with_loop_reassigns_nullable_owner_is_independent_of_map_sibling() {
    check(
        r#"
fun scan(): Unit {
    var outer: String? = "first"
    if (outer != null) {
        var i = 0
        while (i < 2) {
            outer = "second"
            break
        }
    }
    var map = mutableMapOf<Int, Int>()
    map.put(1, 2)
    val frozen = map.consume()
    if (frozen.size == 1) { println("size") }
}
public fun main(): Unit { scan() }
"#,
        b"size\n",
    );
}

#[test]
fn delegated_receiver_with_loop_reassigns_nullable_owner_is_independent_of_map_sibling() {
    check(
        r#"
interface Read { fun value(): Int }
class Reader: Read { override fun value(): Int = 7 }
class Wrapper(val reader: Reader): Read by reader
fun scan(wrapper: Wrapper): Unit {
    var outer: String? = "first"
    if (outer != null) {
        var i = 0
        while (i < 2) {
            outer = "second"
            break
        }
    }
    if (wrapper.value() == 7) { println("delegate") }
}
public fun main(): Unit { scan(Wrapper(Reader())) }
"#,
        b"delegate\n",
    );
}
