//! Guide 10 双轨析构：容器/可空包裹中的资源类型按词法作用域结束析构，而非 ASAP。
use super::{OsStr, TestDirectory, assert_both_native_entries, fs, run};

/// 把值绑定到局部变量后不再使用，先打印标记行；deinit 输出必须在标记之后。
fn assert_scope_drop(name: &str, binding: &str) {
    let directory = TestDirectory::create();
    let source = directory.join(&format!("{name}.ko"));
    let program = format!(
        "class R(val name: String) {{ deinit() {{ println(\"deinit\") }} }}\nfun main(): Unit {{ {binding}; println(\"marker\") }}\n"
    );
    fs::write(&source, program).expect("write source");
    let executed = run([OsStr::new("run"), source.as_os_str()]);
    assert_eq!(executed.status.code(), Some(0), "{name}: {executed:?}");
    assert_eq!(executed.stdout, b"marker\ndeinit\n", "{name}: {executed:?}");
    assert!(executed.stderr.is_empty(), "{name}: {executed:?}");
}

#[test]
fn resource_in_list_drops_at_scope_end() {
    assert_scope_drop("list", "val items = listOf(R(\"x\"))");
}

#[test]
fn resource_in_nullable_drops_at_scope_end() {
    assert_scope_drop("nullable", "val item: R? = R(\"x\")");
}

#[test]
fn nullable_owner_if_joins_before_a_temporary_container_loop() {
    for (ty, factory) in [
        ("Resource", r#"Resource("drop")"#),
        ("Box<Payload>", r#"Box(Payload.Ready(Resource("drop")))"#),
        ("Rc<Resource>", r#"Rc(Resource("drop"))"#),
    ] {
        let source = format!(
            r#"class Resource(val label: String) {{ deinit() {{ println(label) }} }}
enum class Payload {{ Ready(value: Resource), Empty }}
fun make(): {ty} {{ println("make"); return {factory} }}
fun visit(present: Boolean, alternate: Boolean): Unit {{
    val value: {ty}? = if (present) {{ val created = make(); (created) }} else {{
        ((when (alternate) {{
            true -> {{ println("first-null"); null }}
            else -> {{ println("second-null"); null }}
        }}))
    }}
    for (_ in listOf("item")) {{ println("loop") }}
    println("end")
}}
fun main(): Unit {{ visit(true, false); visit(false, true); visit(false, false) }}
"#
        );
        assert_both_native_entries(
            &source,
            b"make\nloop\nend\ndrop\nfirst-null\nloop\nend\nsecond-null\nloop\nend\n",
        );
    }
}

#[test]
fn nullable_owner_if_joins_null_only_values_after_their_effects() {
    assert_both_native_entries(
        r#"class Resource(val label: String) { deinit() { println(label) } }
fun absent(): Nothing? { println("call"); return null }
fun visit(own source: Resource?, mode: Int): Unit {
    val saved: Nothing? = null
    val result: Resource? = if (mode == 0) { Resource("drop:result") } else {
        if (mode == 1) { absent() } else {
            if (mode == 2) { (saved) } else {
                when (source) {
                    null -> { println("null-arm"); null }
                    else -> {
                        if (mode == 4) { println("early"); return }
                        if (mode == 5) { println("nested-first"); null }
                        else { println("nested-second"); null }
                    }
                }
            }
        }
    }
    for (_ in listOf("item")) { println("loop") }
    println("end")
}

fun main(): Unit {
    visit(null, 0); visit(null, 1); visit(null, 2); visit(null, 3)
    visit(Resource("drop:subject"), 3)
    visit(Resource("drop:subject"), 4)
    visit(Resource("drop:subject"), 5)
}
"#,
        b"loop\nend\ndrop:result\ncall\nloop\nend\nloop\nend\nnull-arm\nloop\nend\n\
nested-second\nloop\nend\ndrop:subject\nearly\ndrop:subject\nnested-first\nloop\nend\ndrop:subject\n",
    );
}

#[test]
fn nullable_local_uses_its_declared_layout_after_one_arm_returns() {
    for (ty, make, selected_drop) in [
        (
            "Resource",
            r#"Resource("drop:selected")"#,
            "drop:selected\n",
        ),
        (
            "Box<Payload>",
            r#"Box(Payload.Ready(Resource("drop:selected")))"#,
            "drop:selected\n",
        ),
        (
            "Rc<Resource>",
            r#"Rc(Resource("drop:selected"))"#,
            "drop:selected\n",
        ),
        ("String", r#""owner" + """#, ""),
        ("Float", "1.25f", ""),
        ("Double", "1.25", ""),
        ("InlineValue", "InlineValue(3)", ""),
    ] {
        let source = format!(
            r#"class Resource(val label: String) {{ deinit() {{ println(label) }} }}
enum class Payload {{ Ready(value: Resource), Empty }}
value class InlineValue(val value: Int)
fun make(): {ty} {{ println("make"); return {make} }}
fun visit(stop: Boolean): Unit {{
    val outer = Resource("drop:outer")
    val value: {ty}? = if (stop) {{
        val early = Resource("drop:early")
        println("return")
        return
    }} else {{
        val selected = make()
        (selected)
    }}
    for (_ in listOf("item")) {{ println("loop") }}
    println("end")
}}
fun main(): Unit {{ visit(true); visit(false) }}
"#
        );
        assert_both_native_entries(
            &source,
            format!("return\ndrop:early\ndrop:outer\nmake\nloop\nend\n{selected_drop}drop:outer\n")
                .as_bytes(),
        );
    }
}

#[test]
fn generic_nullable_local_uses_its_declared_layout_after_one_arm_returns() {
    assert_both_native_entries(
        r#"class Resource(val label: String) { deinit() { println(label) } }
fun <T> visit(own resource: T, stop: Boolean): Unit {
    val saved: T? = if (stop) { println("early"); return } else { resource }
    for (_ in listOf("item")) { println("loop") }
    println("end")
}
fun main(): Unit { visit(Resource("drop:early"), true); visit(Resource("drop:normal"), false) }
"#,
        b"early\ndrop:early\nloop\nend\ndrop:normal\n",
    );
}

#[test]
fn nullable_map_if_joins_in_the_planned_map_layout() {
    for (ty, handoff) in [("MutableMap", "table"), ("Map", "table.consume()")] {
        let source = format!(
            r#"class Resource(val label: String) {{ deinit() {{ println(label) }} }}
fun make(): {ty}<Int, Resource> {{
    println("make")
    var table = mutableMapOf<Int, Resource>()
    table.put(1, Resource("drop"))
    return {handoff}
}}
fun absent(): Nothing? {{ println("absent"); return null }}
fun visit(present: Boolean): Unit {{
    val selected: {ty}<Int, Resource>? = if (present) {{ make() }} else {{ absent() }}
    for (_ in listOf("item")) {{ println("loop") }}
    val isAbsent = selected == null
    if (isAbsent) {{ println("null") }} else {{ println("has-map") }}
    println("end")
}}
fun main(): Unit {{ visit(true); visit(false) }}
"#
        );
        assert_both_native_entries(
            &source,
            b"make\nloop\nhas-map\nend\ndrop\nabsent\nloop\nnull\nend\n",
        );
    }
}

#[test]
fn temporary_nullable_char_compares_with_null_on_either_side() {
    assert_both_native_entries(
        r#"fun pick(present: Boolean): Unit {
    val missing = (if (present) { '\0' } else { null }) == null
    val found = null != (if (present) { '界' } else { null })
    if (missing) { println("absent") }
    if (found) { println("present") }
}
fun main(): Unit { pick(false); pick(true) }
"#,
        b"absent\npresent\n",
    );
}

#[test]
fn nullable_char_narrowing_reads_payload_only_on_the_present_edge() {
    assert_both_native_entries(
        r#"class Marker(val label: String) { deinit() { println(label) } }
fun visit(stop: Boolean, present: Boolean): Unit {
    val marker = Marker("drop")
    val selected: Char? = if (stop) { println("early"); return }
        else if (present) { '\0' }
        else { null }
    if (selected != null) {
        if (selected == '\0') { println("nul") }
        println("payload=${selected}")
    } else { println("absent") }
}
fun main(): Unit { visit(true, true); visit(false, true); visit(false, false) }
"#,
        b"early\ndrop\nnul\npayload=\0\ndrop\nabsent\ndrop\n",
    );
}

#[test]
fn nullable_char_if_preserves_tag_payload_and_cleanup() {
    assert_both_native_entries(
        r#"class Marker(val label: String) { deinit() { println(label) } }
fun absent(): Nothing? { println("absent"); return null }
fun visit(stop: Boolean, present: Boolean): Unit {
    val marker = Marker("drop")
    val selected: Char? = if (stop) { println("early"); return } else { '界' }
    val joined: Char? = if (present) { '\0' } else { absent() }
    for (_ in listOf("item")) { println("loop") }
    if (selected != null) { println("char=${selected}") }
    if (joined == null) { println("null") }
    else if (joined == '\0') { println("nul") }
}
fun main(): Unit { visit(true, true); visit(false, true); visit(false, false) }
"#,
        b"early\ndrop\nloop\nchar=\xe7\x95\x8c\nnul\ndrop\nabsent\nloop\nchar=\xe7\x95\x8c\nnull\ndrop\n",
    );
}

#[test]
fn nullable_when_ignores_dead_owner_results_and_preserves_live_effects() {
    assert_both_native_entries(
        r#"class Resource(val label: String) { deinit() { println(label) } }
fun condition(): Boolean { println("condition"); return true }
fun items(argument: Int): List<String> { println("dead-call"); return listOf("never") }
fun visit(own source: Resource?, mode: Int, stop: Boolean): Unit {
    val result: Resource? = when (source) {
        null -> null
        else -> {
            if (mode == 0) {
                if (false) { Resource("dead-if") } else {
                    val arm = Resource("drop:arm")
                    println("live")
                    null
                }
            } else if (mode == 1) {
                if (stop) {
                    println("early")
                    return
                    Resource("dead-tail")
                } else { println("live"); null }
            } else if (mode == 2) {
                if (condition() && false) { Resource("dead-condition") }
                else { println("live"); null }
            } else {
                for (_ in items(if (stop) { println("for-return"); return } else { return })) {
                    println("dead-body")
                }
                Resource("dead-for-tail")
            }
        }
    }
    for (_ in listOf("item")) { println("loop") }
    println("end")
}
fun main(): Unit {
    visit(Resource("drop:source"), 0, false)
    visit(Resource("drop:source"), 1, true)
    visit(Resource("drop:source"), 1, false)
    visit(Resource("drop:source"), 2, false)
    visit(Resource("drop:source"), 3, true)
}
"#,
        b"live\ndrop:arm\nloop\nend\ndrop:source\nearly\ndrop:source\nlive\nloop\nend\ndrop:source\n\
condition\nlive\nloop\nend\ndrop:source\nfor-return\ndrop:source\n",
    );
}

#[test]
fn nullable_when_still_rejects_a_reachable_non_null_owner_result() {
    let directory = TestDirectory::create();
    let source = directory.join("nonnull.ko");
    fs::write(
        &source,
        r#"class Resource(val label: String) { deinit() { println(label) } }
fun visit(own source: Resource?, flag: Boolean): Unit {
    val result: Resource? = when (source) {
        null -> null
        else -> if (flag) { Resource("non-null") } else { null }
    }
    for (_ in listOf("item")) { println("loop") }
}
fun main(): Unit { visit(Resource("drop:source"), true) }
"#,
    )
    .unwrap();
    let checked = run([OsStr::new("check"), source.as_os_str()]);
    assert!(checked.status.success(), "{checked:?}");
    let executed = run([OsStr::new("run"), source.as_os_str()]);
    assert_eq!(executed.status.code(), Some(2), "{executed:?}");
    assert!(executed.stdout.is_empty(), "{executed:?}");
    assert!(
        String::from_utf8_lossy(&executed.stderr).contains("UnsupportedNode"),
        "{executed:?}"
    );
}
