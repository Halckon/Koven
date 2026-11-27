//! SPEC-0330 ordinary while flow; bounded successful source only.
use super::bounded_iteration_run as bounded_run;

use super::{OsStr, TestDirectory, assert_both_native_entries, fs, run};

#[test]
fn single_while_flow_executes_bounded_success_and_cleanup() {
    let directory = TestDirectory::create();
    let source = directory.join("while.ko");
    let executable = directory.join("while");
    fs::write(
        &source,
        include_str!("../fixtures/while-flow-0330/success.ko"),
    )
    .unwrap();
    let built = run([
        OsStr::new("build"),
        source.as_os_str(),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(0), "{built:?}");
    assert!(
        built.stdout.is_empty() && built.stderr.is_empty(),
        "{built:?}"
    );
    let launched = bounded_run::run(&executable);
    assert_eq!(launched.status.code(), Some(0), "{launched:?}");
    assert_eq!(
        launched.stdout,
        include_bytes!("../fixtures/while-flow-0330/success.stdout")
    );
    assert!(launched.stderr.is_empty(), "{launched:?}");
    super::assert_build_entry_count(&directory.0, &executable, 2);
}

#[test]
fn single_pending_field_child_executes_both_branches_and_empty_input() {
    let directory = TestDirectory::create();
    let source = directory.join("while.ko");
    let executable = directory.join("while");
    fs::write(
        &source,
        include_str!("../fixtures/while-flow-0330/pending.ko"),
    )
    .unwrap();
    let built = run([
        OsStr::new("build"),
        source.as_os_str(),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(0), "{built:?}");
    assert!(
        built.stdout.is_empty() && built.stderr.is_empty(),
        "{built:?}"
    );
    let launched = bounded_run::run(&executable);
    assert_eq!(launched.status.code(), Some(0), "{launched:?}");
    assert_eq!(
        launched.stdout,
        include_bytes!("../fixtures/while-flow-0330/pending.stdout")
    );
    assert!(launched.stderr.is_empty(), "{launched:?}");
    super::assert_build_entry_count(&directory.0, &executable, 2);
}

#[test]
fn single_if_retest_executes_both_branches_and_empty_input() {
    let directory = TestDirectory::create();
    let source = directory.join("while.ko");
    let executable = directory.join("while");
    fs::write(
        &source,
        include_str!("../fixtures/while-flow-0330/retest.ko"),
    )
    .unwrap();
    let built = run([
        OsStr::new("build"),
        source.as_os_str(),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(0), "{built:?}");
    assert!(
        built.stdout.is_empty() && built.stderr.is_empty(),
        "{built:?}"
    );
    let launched = bounded_run::run(&executable);
    assert_eq!(launched.status.code(), Some(0), "{launched:?}");
    assert_eq!(
        launched.stdout,
        include_bytes!("../fixtures/while-flow-0330/retest.stdout")
    );
    assert!(launched.stderr.is_empty(), "{launched:?}");
    super::assert_build_entry_count(&directory.0, &executable, 2);
}

#[test]
fn single_numeric_for_preserves_outer_proof_and_empty_input() {
    let directory = TestDirectory::create();
    let source = directory.join("while.ko");
    let executable = directory.join("while");
    fs::write(
        &source,
        include_str!("../fixtures/while-flow-0330/numeric.ko"),
    )
    .unwrap();
    let built = run([
        OsStr::new("build"),
        source.as_os_str(),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(0), "{built:?}");
    assert!(
        built.stdout.is_empty() && built.stderr.is_empty(),
        "{built:?}"
    );
    let launched = bounded_run::run(&executable);
    assert_eq!(launched.status.code(), Some(0), "{launched:?}");
    assert_eq!(
        launched.stdout,
        include_bytes!("../fixtures/while-flow-0330/numeric.stdout")
    );
    assert!(launched.stderr.is_empty(), "{launched:?}");
    super::assert_build_entry_count(&directory.0, &executable, 2);
}

#[test]
fn single_borrowed_for_preserves_outer_proof_and_empty_input() {
    let directory = TestDirectory::create();
    let source = directory.join("while.ko");
    let executable = directory.join("while");
    fs::write(
        &source,
        include_str!("../fixtures/while-flow-0330/inner-borrowed.ko"),
    )
    .unwrap();
    let built = run([
        OsStr::new("build"),
        source.as_os_str(),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(0), "{built:?}");
    assert!(
        built.stdout.is_empty() && built.stderr.is_empty(),
        "{built:?}"
    );
    let launched = bounded_run::run(&executable);
    assert_eq!(launched.status.code(), Some(0), "{launched:?}");
    assert_eq!(
        launched.stdout,
        include_bytes!("../fixtures/while-flow-0330/inner-borrowed.stdout")
    );
    assert!(launched.stderr.is_empty(), "{launched:?}");
    super::assert_build_entry_count(&directory.0, &executable, 2);
}

#[test]
fn single_nullable_while_preserves_outer_borrowed_for_access() {
    let directory = TestDirectory::create();
    let source = directory.join("while.ko");
    let executable = directory.join("while");
    fs::write(
        &source,
        include_str!("../fixtures/while-flow-0330/outer-borrowed.ko"),
    )
    .unwrap();
    let built = run([
        OsStr::new("build"),
        source.as_os_str(),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(0), "{built:?}");
    assert!(
        built.stdout.is_empty() && built.stderr.is_empty(),
        "{built:?}"
    );
    let launched = bounded_run::run(&executable);
    assert_eq!(launched.status.code(), Some(0), "{launched:?}");
    assert_eq!(
        launched.stdout,
        include_bytes!("../fixtures/while-flow-0330/outer-borrowed.stdout")
    );
    assert!(launched.stderr.is_empty(), "{launched:?}");
    super::assert_build_entry_count(&directory.0, &executable, 2);
}

#[test]
fn local_destructuring_routes_while_and_temporary_for_in_both_entries() {
    assert_both_native_entries(
        include_str!("../fixtures/while-flow-0330/local-destructuring.ko"),
        b"kept\nloop\nkept\nloop\nchanged\nfor-changed\nwhile-changed\n",
    );
}

#[test]
fn constant_while_preserves_condition_effects_and_terminating_operands_in_both_entries() {
    assert_both_native_entries(
        include_str!("../fixtures/while-flow-0330/constant-edges.ko"),
        b"kept\ncondition\nnormal exit\nprefix released\nprefix released\nfirst argument\nfirst argument\nafter break\n",
    );
}

#[test]
fn long_short_circuit_chain_runs_both_results_in_both_entries() {
    let condition = format!("flag{}", " || flag".repeat(512));
    let source = format!(
        r#"
fun probe(flag: Boolean): Int {{
    if ({condition}) {{ return 1 }}
    return 0
}}
fun main(): Unit {{
    if (probe(false) == 0) {{ println("false-ok") }}
    if (probe(true) == 1) {{ println("true-ok") }}
}}
"#
    );
    assert_both_native_entries(&source, b"false-ok\ntrue-ok\n");
}

#[test]
fn long_short_circuit_chain_keeps_rhs_and_outer_cleanup_in_both_entries() {
    let condition = format!(
        "flag{} || tested(Guard(\"drop:rhs\"))",
        " || flag".repeat(511)
    );
    assert_both_native_entries(
        &short_circuit_resource_source(&condition),
        SHORT_CIRCUIT_RESOURCE_OUTPUT,
    );
}

const SHORT_CIRCUIT_RESOURCE_OUTPUT: &[u8] =
    b"case:true\ntaken\nkeep\nafter\ndrop:outer\ncase:false\nrhs\ndrop:rhs\ntaken\nkeep\nafter\ndrop:outer\n";

#[test]
fn grouped_short_circuit_chains_keep_cleanup_order_in_both_entries() {
    let rhs = "tested(Guard(\"drop:rhs\"))";
    let left = format!(
        "{}flag{} || {rhs}",
        "(".repeat(128),
        " || flag)".repeat(128)
    );
    let right = format!("{}{rhs}{}", "flag || (".repeat(192), ")".repeat(192));
    for condition in [left, right] {
        assert_both_native_entries(
            &short_circuit_resource_source(&condition),
            SHORT_CIRCUIT_RESOURCE_OUTPUT,
        );
    }
}

#[test]
fn long_known_short_circuit_chain_keeps_bypassed_rhs_and_outer_cleanup() {
    let condition = format!(
        "true{} || tested(Guard(\"drop:rhs\"))",
        " || flag".repeat(511)
    );
    assert_both_native_entries(
        &short_circuit_resource_source(&condition),
        b"case:true\ntaken\nkeep\nafter\ndrop:outer\ncase:false\ntaken\nkeep\nafter\ndrop:outer\n",
    );
}

#[test]
fn terminating_when_selectors_evaluate_unit_argument_prefixes() {
    let plain = r#"fun quiet(): Unit { println("prefix") }
fun keep(stop: Boolean, effect: Unit, later: Int): Boolean {
    println("dispatch")
    return stop
}
fun later(): Int { println("later"); return 1 }
fun main(): Unit {
    while (true) {
        when {
            keep(effect = quiet(), stop = break, later = later()) -> { println("matched") }
            else -> { println("missed") }
        }
    }
    println("done")
}"#;
    assert_both_native_entries(plain, b"prefix\ndone\n");

    let resources = r#"class Guard(val label: String) { deinit() { println(label) } }
fun quiet(): Unit { println("prefix") }
fun keep(own item: Guard, effect: Unit, stop: Boolean): Boolean {
    println("dispatch")
    return stop
}
fun main(): Unit {
    val outer = Guard("outer")
    while (true) {
        val body = Guard("body")
        when {
            keep(Guard("argument"), quiet(), break) -> { println("matched") }
            else -> { println("missed") }
        }
    }
    println("done")
}"#;
    assert_both_native_entries(resources, b"prefix\nargument\nbody\ndone\nouter\n");
}

#[test]
fn terminating_if_conditions_evaluate_unit_argument_prefixes() {
    let source = r#"fun quiet(): Unit { println("prefix") }
fun keep(stop: Boolean, effect: Unit, later: Int): Boolean {
    println("dispatch")
    return stop
}
fun later(): Int { println("later"); return 1 }
fun main(): Unit {
    while (true) {
        if (keep(effect = quiet(), stop = break, later = later())) { println("matched") }
    }
    println("done")
}"#;
    assert_both_native_entries(source, b"prefix\ndone\n");
}

#[test]
fn terminating_when_selectors_end_temporary_argument_borrows_before_drops() {
    let source = r#"class Guard(val label: String) { deinit() { println(label) } }
fun trace(): String { println("text"); return "text" }
fun keep(stop: Boolean, first: Guard, text: String, second: Guard): Boolean {
    println("dispatch")
    return stop
}
fun main(): Unit {
    val outer = Guard("outer")
    while (true) {
        val body = Guard("body")
        when {
            keep(first = Guard("first"), text = trace(), second = Guard("second"), stop = break) -> {
                println("matched")
            }
            else -> { println("missed") }
        }
    }
    println("done")
}"#;
    assert_both_native_entries(source, b"text\nsecond\nfirst\nbody\ndone\nouter\n");
}

#[test]
fn when_selectors_clean_up_borrowed_temporaries_on_both_call_edges() {
    let source = r#"class Guard(val label: String) { deinit() { println(label) } }
fun trace(): String { println("text"); return "text" }
fun keep(stop: Boolean, first: Guard, text: String, second: Guard): Boolean {
    println("dispatch")
    return stop
}
fun probe(skip: Boolean): Unit {
    val outer = Guard("outer")
    while (true) {
        val body = Guard("body")
        when {
            keep(first = Guard("first"), text = trace(), second = Guard("second"),
                 stop = if (skip) { break } else { true }) -> {
                println("matched")
                break
            }
            else -> { println("missed") }
        }
    }
    println("done")
}
fun main(): Unit { probe(true); probe(false) }"#;
    let expected = concat!(
        "text\nsecond\nfirst\nbody\ndone\nouter\n",
        "text\ndispatch\nsecond\nfirst\nmatched\nbody\ndone\nouter\n",
    );
    assert_both_native_entries(source, expected.as_bytes());
}

#[test]
fn when_selectors_dispatch_direct_calls_with_unit_arguments() {
    let source = r#"fun quiet(): Unit { println("prefix") }
fun owned(own effect: Unit): Unit { println("own") }
fun relay(borrow effect: Unit): Unit { owned(effect); println("borrow") }
fun choose(effect: Unit, flag: Boolean): Boolean { relay(effect); return flag }
fun plain(): Unit { var effect = quiet(); effect = quiet(); relay(effect) }
fun main(): Unit {
    plain()
    val initial = quiet()
    var effect = (initial)
    effect = quiet()
    when {
        choose(effect, true) -> { println("matched") }
        else -> { println("missed") }
    }
}"#;
    assert_both_native_entries(
        source,
        b"prefix\nprefix\nown\nborrow\nprefix\nprefix\nown\nborrow\nmatched\n",
    );
}

#[test]
fn temporary_sequential_iteration_drops_before_leaving_a_move_only_enum_arm() {
    let source = r#"class Marker(val label: String) { deinit() { println(label) } }
enum class Choice { Yes(payload: Marker), Other }
fun element(own label: String): Marker { println("element-made"); return Marker(label) }
fun visit(run: Boolean, mode: Int): String {
    val subject: Choice = if (run) { Choice.Yes(Marker("enum-drop")) } else { Choice.Other }
    return when (subject) {
        is Choice.Yes -> {
            for (_ in listOf(element("first-drop"), element("second-drop"))) {
                val current = Marker("iteration-drop")
                println("body")
                if (mode == 1) { continue }
                if (mode == 2) { break }
                if (mode == 3) { return "early" }
                println("body-end")
            }
            println("arm-end")
            "done"
        }
        else -> "other"
    }
}
fun main(): Unit {
    println(visit(true, 0)); println(visit(true, 1)); println(visit(true, 2))
    println(visit(true, 3)); println(visit(false, 0))
}
"#;
    let expected = b"element-made\nelement-made\nbody\nbody-end\niteration-drop\nbody\nbody-end\niteration-drop\nsecond-drop\nfirst-drop\narm-end\nenum-drop\ndone\n\
element-made\nelement-made\nbody\niteration-drop\nbody\niteration-drop\nsecond-drop\nfirst-drop\narm-end\nenum-drop\ndone\n\
element-made\nelement-made\nbody\niteration-drop\nsecond-drop\nfirst-drop\narm-end\nenum-drop\ndone\n\
element-made\nelement-made\nbody\niteration-drop\nsecond-drop\nfirst-drop\nenum-drop\nearly\nother\n";
    for provider in ["arrayOf", "listOf", "mutableListOf"] {
        assert_both_native_entries(&source.replace("listOf", provider), expected);
    }
}

#[test]
fn map_contains_stops_after_its_temporary_receiver_returns() {
    let source = r#"fun key(): String { println("key"); return "x".clone() }
fun table(value: Int): MutableMap<String, Int> {
    println("map")
    return mutableMapOf<String, Int>()
}
fun probe(): Int {
    if (table(return 9).contains(key())) { return 1 }
    return 0
}
fun main(): Unit { if (probe() == 9) { println("nine") } }"#;
    assert_both_native_entries(source, b"nine\n");
}

#[test]
fn map_put_stops_after_its_temporary_receiver_returns() {
    let source = r#"fun key(): Int { println("key"); return 1 }
fun value(): Int { println("value"); return 7 }
fun table(input: Int): MutableMap<Int, Int> {
    println("map")
    return mutableMapOf<Int, Int>()
}
fun probe(): Int {
    table(return 9).put(key(), value())
    return 0
}
fun main(): Unit { if (probe() == 9) { println("nine") } }"#;
    assert_both_native_entries(source, b"nine\n");
}

#[test]
fn map_get_stops_after_its_temporary_receiver_returns_and_drops_the_outer_owner() {
    let source = r#"class Guard(val label: String) { deinit() { println(label) } }
fun key(): Int { println("key"); return 1 }
fun table(input: Int): MutableMap<Int, Int> {
    println("map")
    return mutableMapOf<Int, Int>()
}
fun probe(): Int {
    val guard = Guard("drop:outer")
    table(return 9).get(key())
    return 0
}
fun main(): Unit { if (probe() == 9) { println("nine") } }"#;
    assert_both_native_entries(source, b"drop:outer\nnine\n");
}

#[test]
fn map_remove_and_with_value_skip_operands_after_receiver_returns() {
    let source = r#"class Guard(val label: String) { deinit() { println(label) } }
fun key(): Int { println("key"); return 1 }
fun table(input: Int): MutableMap<Int, Int> {
    println("map")
    return mutableMapOf<Int, Int>()
}
fun removeProbe(): Int {
    val guard = Guard("drop:remove")
    table(return 9).remove(key())
    return 0
}
fun withValueProbe(): Int {
    val guard = Guard("drop:withValue")
    table(return 7).withValue(key(), { value -> println("callback") })
    return 0
}
fun main(): Unit {
    if (removeProbe() == 9) { println("nine") }
    if (withValueProbe() == 7) { println("seven") }
}"#;
    assert_both_native_entries(source, b"drop:remove\nnine\ndrop:withValue\nseven\n");
}

#[test]
fn map_consume_stops_after_its_temporary_receiver_returns() {
    let source = r#"class Guard(val label: String) { deinit() { println(label) } }
fun table(input: Int): MutableMap<Int, Guard> {
    println("map")
    var map = mutableMapOf<Int, Guard>()
    map.put(1, Guard("drop:entry"))
    return map
}
fun probe(): Int {
    val guard = Guard("drop:outer")
    table(return 9).consume()
    return 0
}
fun main(): Unit { if (probe() == 9) { println("nine") } }"#;
    assert_both_native_entries(source, b"drop:outer\nnine\n");
}

#[test]
fn map_with_value_ends_pending_loans_when_an_argument_exits() {
    let source = r#"class Guard(val label: String) { deinit() { println(label) } }
fun key(): String { println("key"); return "hit".clone() }
fun keyExit(stop: Boolean): Int {
    var map = mutableMapOf<String, Guard>()
    map.put("hit".clone(), Guard("drop:key-map"))
    map.withValue(if (stop) return 9 else key(), { item -> println("callback") })
    return 7
}
fun actionExit(): Int {
    var map = mutableMapOf<String, Guard>()
    map.put("hit".clone(), Guard("drop:action-map"))
    map.withValue(key(), return 9)
    return 0
}
fun loopExit(): Unit {
    var map = mutableMapOf<String, Guard>()
    map.put("hit".clone(), Guard("drop:loop-map"))
    var step = 0
    while (step < 2) {
        step = step + 1
        if (step == 1) { map.withValue(continue, { item -> println("bad:continue") }) }
        map.withValue(break, { item -> println("bad:break") })
    }
    if (map.contains("hit")) { println("loop:reused") }
}
fun main(): Unit {
    if (keyExit(true) == 9) { println("key:return") }
    if (keyExit(false) == 7) { println("key:complete") }
    if (actionExit() == 9) { println("action:return") }
    loopExit()
}"#;
    let expected = concat!(
        "drop:key-map\nkey:return\n",
        "key\ncallback\ndrop:key-map\nkey:complete\n",
        "key\ndrop:action-map\naction:return\n",
        "loop:reused\ndrop:loop-map\n",
    );
    assert_both_native_entries(source, expected.as_bytes());
}

#[test]
fn map_require_value_only_binds_a_result_when_the_query_is_reached() {
    let source = r#"class Guard(val label: String) { deinit() { println(label) } }
fun key(): String { println("key"); return "hit".clone() }
fun make(input: Int): MutableMap<String, String> {
    println("map")
    var map = mutableMapOf<String, String>()
    map.put("hit".clone(), "value".clone())
    return map
}
fun observe(item: String): Unit { println(item) }
fun receiverExit(): Int {
    val guard = Guard("drop:receiver")
    block { borrow val item = make(return 9).requireValue(key()); observe(item) }
    return 0
}
fun keyExit(): Int {
    val guard = Guard("drop:key")
    val map = make(0)
    block { borrow val item = map.requireValue(return 9); observe(item) }
    return 0
}
fun branch(stop: Boolean): Int {
    val guard = Guard("drop:branch")
    val map = make(0)
    block { borrow val item = map.requireValue(if (stop) return 9 else key()); observe(item) }
    if (map.contains("hit")) { println("reused") }
    return 7
}
fun main(): Unit {
    if (receiverExit() == 9) { println("receiver:return") }
    if (keyExit() == 9) { println("key:return") }
    if (branch(true) == 9) { println("branch:return") }
    if (branch(false) == 7) { println("branch:complete") }
}"#;
    let expected = concat!(
        "drop:receiver\nreceiver:return\n",
        "map\ndrop:key\nkey:return\n",
        "map\ndrop:branch\nbranch:return\n",
        "map\nkey\nvalue\nreused\ndrop:branch\nbranch:complete\n",
    );
    assert_both_native_entries(source, expected.as_bytes());
}

#[test]
fn short_circuit_loop_exits_end_borrows_and_drop_resources_in_both_entries() {
    let source = include_str!("../../../lang-codegen/tests/fixtures/short-circuit-loop-exits.ko");
    let expected = concat!(
        "case:continue\nread:first\nobserve\ndrop:continue-arg\ndrop:second\ndrop:first\n",
        "read:first\nbody\ndrop:second\ndrop:first\nread:outer\ndrop:outer\nzero\n",
        "case:break\nread:first\nobserve\ndrop:break-arg\ndrop:second\ndrop:first\n",
        "read:outer\ndrop:outer\nzero\n",
        "case:return\nread:first\nobserve\ndrop:return-arg\nresult\ndrop:result-arg\n",
        "drop:second\ndrop:first\ndrop:outer\nseven\n",
    );
    assert_both_native_entries(source, expected.as_bytes());
}

fn short_circuit_resource_source(condition: &str) -> String {
    format!(
        r#"
class Guard(val label: String) {{ deinit() {{ println(label) }} }}
fun tested(guard: Guard): Boolean {{
    println("rhs")
    return true
}}
fun keep(guard: Guard): Unit {{ println("keep") }}
fun probe(flag: Boolean): Unit {{
    val outer = Guard("drop:outer")
    if ({condition}) {{ println("taken") }}
    keep(outer)
    println("after")
}}
fun main(): Unit {{
    println("case:true")
    probe(true)
    println("case:false")
    probe(false)
}}
"#
    )
}

#[test]
fn native_string_interpolation_preserves_values_and_cleans_exits() {
    let text = r#"
fun piece(label: String): String {
    println("eval " + label)
    return label.clone()
}
fun stop(early: Boolean): Unit {
    println("[${piece("A")}|${if (early) { return } else { "middle" }}|${piece("B")}]")
}
fun main(): Unit {
    val text = "k"
    println("hello ${((text))} ${text}")
    println(text)
    stop(false)
    stop(true)
    loop { println("${piece("C")}${break}${piece("unreachable")}") }
    println("done")
}
"#;
    assert_both_native_entries(
        text,
        b"hello k k\nk\neval A\neval B\n[A|middle|B]\neval A\neval C\ndone\n",
    );
}

#[test]
fn native_display_resource_direct_call_keeps_reverse_scope_drops() {
    assert_both_native_entries(
        r#"class Trace(val label: String) : Display {
    override fun toString(): String = label.clone()
    deinit() { println(label) }
}
fun main(): Unit {
    val first = Trace("A")
    val second = Trace("B")
    println(first.toString() + second.toString())
    println("body")
}
"#,
        b"AB\nbody\nB\nA\n",
    );
}

#[test]
fn native_display_resource_delegation_keeps_wrapper_and_field_drops() {
    assert_both_native_entries(
        r#"class Resource(val label: String) : Display {
    override fun toString(): String = label.clone()
    deinit() { println("resource " + label) }
}
class Wrapper(val resource: Resource) : Display by resource {
    deinit() { println("wrapper") }
}
fun main(): Unit {
    val wrapper = Wrapper(Resource("A"))
    println(wrapper.toString())
    println("body")
}
"#,
        b"A\nbody\nwrapper\nresource A\n",
    );
}

#[test]
fn native_nominal_interpolation_preserves_dispatch_and_resource_cleanup() {
    assert_both_native_entries(
        r#"class Trace(val label: String) : Display {
    override fun toString(): String {
        println("format " + label)
        return label.clone()
    }
    deinit() { println("drop " + label) }
}
fun make(label: String): Trace {
    println("eval " + label)
    return Trace(label.clone())
}
fun normal(): Unit {
    println("[${make("A")}|${make("B")}]")
}
fun early(flag: Boolean): Unit {
    println("[${make("C")}|${if (flag) { return } else { "ok" }}]")
}
fun main(): Unit {
    val named = Trace("named")
    println("${named}")
    normal()
    early(true)
    early(false)
    println("done")
}
"#,
        b"format named\nnamed\neval A\nformat A\neval B\nformat B\ndrop B\ndrop A\n[A|B]\neval C\nformat C\ndrop C\neval C\nformat C\ndrop C\n[C|ok]\ndone\ndrop named\n",
    );
}

#[test]
fn native_nominal_interpolation_specializes_and_delegates_only_reachable_calls() {
    assert_both_native_entries(
        r#"class Trace(val label: String) : Display {
    override fun toString(): String {
        println("format " + label)
        return label.clone()
    }
    deinit() { println("drop " + label) }
}
class Wrapper(val trace: Trace) : Display by trace {
    deinit() { println("drop wrapper") }
}
class Unneeded() : Display {
    override fun toString(): String = "${1.0}"
}
fun renderDead(): Unit {
    return
    println("${Unneeded()}")
}
fun <T : Display> render(value: T): String = "<${value}>"
fun main(): Unit {
    val delegated = Wrapper(Trace("delegate"))
    renderDead()
    println(render(delegated))
    println(render("builtin"))
    val renderLocal: () -> String = { "${Trace("lambda")}" }
    println(renderLocal())
    println("done")
}
"#,
        b"format delegate\n<delegate>\n<builtin>\nformat lambda\ndrop lambda\nlambda\ndone\ndrop wrapper\ndrop delegate\n",
    );
}

#[test]
fn native_nominal_interpolation_uses_non_null_proofs_and_field_loans() {
    assert_both_native_entries(
        r#"class Trace(val label: String) : Display {
    override fun toString(): String = label.clone()
    deinit() { println("drop " + label) }
}
class Holder(val trace: Trace) { deinit() { println("holder") } }
fun render(own candidate: Trace?): String =
    if (candidate != null) { "<${candidate}>" } else { "nil" }
fun main(): Unit {
    println(render(Trace("present")))
    println(render(null))
    val value = Holder(Trace("field"))
    println("<${value.trace}>")
    println("body")
}
"#,
        b"drop present\n<present>\nnil\n<field>\nbody\nholder\ndrop field\n",
    );
}

#[test]
fn native_nominal_interpolation_stops_at_constant_ownership_exit() {
    assert_both_native_entries(
        r#"const val ALWAYS = true
class Unneeded() : Display { override fun toString(): String = "${1.0}" }
fun prefix(): Int {
    println("[${if (ALWAYS && ((return 7) == 0)) { "yes" } else { "no" }}|${Unneeded()}]")
    return 8
}
fun main(): Unit { if (prefix() == 7) { println("stopped") } }
"#,
        b"stopped\n",
    );
}

#[test]
fn native_nominal_interpolation_dispatches_on_a_refined_enum_case() {
    assert_both_native_entries(
        r#"enum class Label : Display {
    Item(value: Int), Other;
    override fun toString(): String = "enum"
}
fun main(): Unit {
    val label: Label = Label.Item(7)
    when (label) {
        is Label.Item -> println("${label}")
        else -> println("other")
    }
}
"#,
        b"enum\n",
    );
}

#[test]
fn native_char_interpolation_encodes_one_unicode_scalar() {
    let mut source = String::from(
        r#"fun read(value: Char): Char { println("read"); return value }
fun <T : Display> render(value: T): String = "<${value}>"
fun show(value: Char): Unit {
    println("[${read(value)}]")
    println(render(value))
}
fun main(): Unit {
    show('\0')
"#,
    );
    let mut expected = String::from("read\n[\0]\n<\0>\n");
    for value in [
        'a',
        '\u{7f}',
        '\u{80}',
        '\u{7ff}',
        '\u{800}',
        '\u{d7ff}',
        '\u{e000}',
        '\u{ffff}',
        '\u{10000}',
        '\u{10ffff}',
    ] {
        source.push_str(&format!("    show('{value}')\n"));
        expected.push_str(&format!("read\n[{value}]\n<{value}>\n"));
    }
    source.push_str("}\n");
    assert_both_native_entries(&source, expected.as_bytes());
}

#[test]
fn native_integer_interpolation_preserves_width_sign_and_extrema() {
    let mut source = String::from("fun <T : Display> render(value: T): String = \"<${value}>\"\n");
    let mut main = String::from("fun main(): Unit {\n");
    let mut expected = String::new();
    for (ty, values, suffix) in [
        ("Byte", ["-128", "0", "127"], ""),
        ("Short", ["-32768", "0", "32767"], ""),
        ("Int", ["-2147483648", "0", "2147483647"], ""),
        (
            "Long",
            ["-9223372036854775808", "0", "9223372036854775807"],
            "L",
        ),
        ("UByte", ["1", "0", "255"], "u"),
        ("UShort", ["1", "0", "65535"], "u"),
        ("UInt", ["1", "0", "4294967295"], "u"),
        ("ULong", ["1", "0", "18446744073709551615"], "uL"),
    ] {
        source.push_str(&format!(
            "fun show{ty}(value: {ty}): Unit {{ println(\"[${{value}}]\"); println(render(value)) }}\n"
        ));
        for value in values {
            main.push_str(&format!("    show{ty}({value}{suffix})\n"));
            expected.push_str(&format!("[{value}]\n<{value}>\n"));
        }
    }
    source.push_str(&main);
    source.push_str("}\n");
    assert_both_native_entries(&source, expected.as_bytes());
}

#[test]
fn native_char_order_matches_unicode_scalar_order() {
    assert_both_native_entries(
        r#"fun ordered(a: Char, b: Char): Boolean =
    a < b && a <= b && b > a && b >= a && a != b && !(b < a) && !(a > b)
fun describe(value: Char): String {
    if (value <= '中' && value >= '中' && value == '中') { return "${value}" }
    return "other"
}
fun main(): Unit {
    if (!ordered('\0', 'é') || !ordered('é', '中') || !ordered('中', '🦀')) {
        error("incorrect Char ordering")
    }
    println(describe('中'))
    println(describe('🦀'))
}
"#,
        "中\nother\n".as_bytes(),
    );
}
#[path = "float_flow.rs"]
mod float_flow;
