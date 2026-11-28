use crate::native_tests::{boxed_enum_tests::assert_success, emit_link_and_run};

#[test]
fn borrowed_nullable_string_result_supports_presence_and_narrowed_reads() {
    let source = r#"fun view(borrow a: String?): borrow String? from a = a
fun selected(borrow original: String?, enabled: Boolean): Boolean {
    borrow val a = view(original)
    return ((null != (a)) && enabled)
}
fun show(borrow original: String?): Unit {
    borrow val a = view(original)
    if (a != null) { println(a) } else { println("null") }
}
fun entry(): Unit {
    var a: String? = null
    show(a)
    if (selected(a, true)) { println("wrong") }
    a = "".clone()
    show(a)
    if (selected(a, true)) { println("empty selected") }
    a = "text".clone()
    show(a)
    if (selected(a, false)) { println("wrong") }
    if (selected(a, true)) { println("text selected") }
}
"#;
    let result = emit_link_and_run("borrowed-nullable-string-result.ko", source, "entry");
    assert_success(&result, b"null\n\nempty selected\ntext\ntext selected\n");
}

#[test]
fn borrowed_nullable_string_result_narrows_inside_a_nested_short_circuit() {
    let source = r#"fun view(borrow source: String?): borrow String? from source = source
fun inspect(borrow source: String?, enabled: Boolean): Unit {
    if (enabled) {
        borrow val result = view(source)
        if (result != null && result == "T") { println(result) } else { println("miss") }
    }
}
fun entry(): Unit {
    var source: String? = "T".clone()
    inspect(source, true)
    inspect(source, false)
    source = "U".clone()
    inspect(source, true)
    source = null
    inspect(source, true)
}
"#;
    let result = emit_link_and_run("nullable-result-short-circuit.ko", source, "entry");
    assert_success(&result, b"T\nmiss\nmiss\n");
}

#[test]
fn borrowed_nullable_string_result_releases_child_and_source_before_writeback() {
    let source = r#"fun view(borrow source: String?): borrow String? from source = source
fun inspect(inout slot: String?): Unit {
    block {
        borrow val parent = view(slot)
        borrow val child = parent
        if (child == null) { println("null") } else { println(child) }
    }
    slot = null
    if (slot == null) { println("cleared") }
}
fun entry(): Unit {
    var slot: String? = "first".clone()
    inspect(&slot)
    inspect(&slot)
}
"#;
    let result = emit_link_and_run("nullable-result-writeback.ko", source, "entry");
    assert_success(&result, b"first\ncleared\nnull\ncleared\n");
}

#[test]
fn borrowed_nullable_string_result_lends_its_narrowed_payload() {
    let source = r#"fun view(borrow source: String?): borrow String? from source = source
fun inspect(inout slot: String?): Unit {
    block {
        borrow val result = view(slot)
        if (result != null) {
            borrow val child = (result)
            println(child)
        } else { println("null") }
    }
    slot = null
}
fun entry(): Unit {
    var slot: String? = "payload".clone()
    inspect(&slot)
    inspect(&slot)
}
"#;
    let result = emit_link_and_run("nullable-result-payload-borrow.ko", source, "entry");
    assert_success(&result, b"payload\nnull\n");
}

#[test]
fn borrowed_nullable_string_result_reads_the_removed_map_value() {
    let source = r#"fun view(borrow source: String?): borrow String? from source = source
fun inspect(borrow original: String?): Unit {
    borrow val result = view(original)
    if (result != null) {
        borrow val child = result
        println(child)
    } else { println("absent") }
    when (result) {
        null -> println("missing")
        else -> println(result)
    }
}
fun entry(): Unit {
    var map = mutableMapOf<Int, String>()
    map.put(1, "hit".clone())
    map.put(2, "".clone())
    val hit = map.remove(1)
    val empty = map.remove(2)
    val miss = map.remove(3)
    inspect(hit)
    inspect(empty)
    inspect(miss)
}
"#;
    let result = emit_link_and_run("nullable-result-map.ko", source, "entry");
    assert_success(&result, b"hit\nhit\n\n\nabsent\nmissing\n");
}

#[test]
fn map_nullable_string_inout_can_replace_after_ending_the_payload_loan() {
    let source = r#"fun observe(inout slot: String?): Unit {
    if (slot != null) { println(slot) } else { println("absent") }
    slot = null
    if (slot == null) { println("cleared") }
}
fun entry(): Unit {
    var map = mutableMapOf<Int, String>()
    map.put(1, "hit".clone())
    var slot = map.remove(1)
    observe(&slot)
    observe(&slot)
    map.put(2, "again".clone())
    slot = map.remove(2)
    observe(&slot)
}
"#;
    let result = emit_link_and_run("nullable-result-map-writeback.ko", source, "entry");
    assert_success(&result, b"hit\ncleared\nabsent\ncleared\nagain\ncleared\n");
}

#[test]
fn borrowed_nullable_string_result_while_releases_each_iterations_payload() {
    let source = r#"class Marker(val label: String) {
    deinit() { println(label) }
}

fun view(borrow source: String?): borrow String? from source = source

fun inspect(inout source: String?): Unit {
    block {
        borrow val result = view(source)
        var remaining = 2
        while (result != null && remaining > 0) {
            val marker = Marker("iteration drop".clone())
            println(result)
            if (remaining > 1) {
                remaining = 1
                continue
            }
            break
        }
        println("loop exit")
    }
    source = null
}

fun entry(): Unit {
    var source: String? = "".clone()
    inspect(&source)
    inspect(&source)
    source = "hit".clone()
    inspect(&source)
}
"#;
    let result = emit_link_and_run("nullable-result-while.ko", source, "entry");
    assert_success(
        &result,
        b"\niteration drop\n\niteration drop\nloop exit\nloop exit\nhit\niteration drop\nhit\niteration drop\nloop exit\n",
    );
}

#[test]
fn named_string_can_widen_for_a_borrow_in_a_unit_that_removes_map_values() {
    let source = r#"fun inspect(borrow value: String?): Unit {
    if (value != null) { println(value) } else { println("missing") }
}
fun relay(borrow value: String): Unit { inspect(value) }
fun entry(): Unit {
    var map = mutableMapOf<Int, String>()
    map.put(1, "".clone())
    val removed = map.remove(1)
    inspect(removed)
    inspect(map.remove(2))
    val plain = "plain".clone()
    inspect(plain)
    relay(plain)
    println(plain)
}
"#;
    let result = emit_link_and_run("map-string-nullable-borrow-widening.ko", source, "entry");
    assert_success(&result, b"\nmissing\nplain\nplain\nplain\n");
}

#[test]
fn borrowed_nullable_when_forwards_the_same_value_to_both_parameter_types() {
    let source = r#"fun read(borrow value: String): Unit { println(value) }
fun readNullable(borrow value: String?): Unit {
    if (value != null) { println(value) } else { println("unexpected null") }
}
fun inspect(borrow value: String?): Unit {
    when (value) {
        null -> println("absent")
        else -> {
            read(value)
            readNullable((value))
            read(value)
        }
    }
}
fun entry(): Unit {
    var slot: String? = "text".clone()
    inspect(slot)
    slot = "".clone()
    inspect(slot)
    slot = null
    inspect(slot)
}
"#;
    let result = emit_link_and_run("nullable-when-forward-string.ko", source, "entry");
    assert_success(&result, b"text\ntext\ntext\n\n\n\nabsent\n");
}

#[test]
fn borrowed_nullable_when_releases_forwarded_loan_when_a_later_argument_returns() {
    let source = r#"class Marker(val label: String) { deinit() { println(label) } }
fun read(borrow value: String?, own side: Marker): Unit {
    if (value != null) { println(value) }
}
fun inspect(borrow value: String?, stop: Boolean): Unit {
    val outer = Marker("outer".clone())
    when (value) {
        null -> println("absent")
        else -> read(value, if (stop) { return } else { Marker("argument".clone()) })
    }
    println("after")
}
fun entry(): Unit {
    val present: String? = "text".clone()
    inspect(present, true)
    inspect(present, false)
    val absent: String? = null
    inspect(absent, true)
}
"#;
    let result = emit_link_and_run("nullable-when-forward-prefix-return.ko", source, "entry");
    assert_success(
        &result,
        b"outer\ntext\nargument\nafter\nouter\nabsent\nafter\nouter\n",
    );
}

#[test]
fn borrowed_nullable_if_forwards_the_same_value_to_both_parameter_types() {
    let source = r#"fun read(borrow value: String): Unit { println(value) }
fun readNullable(borrow value: String?): Unit {
    if (value != null) { println(value) } else { println("unexpected null") }
}
fun inspect(borrow value: String?, reverse: Boolean): Unit {
    if (value != null) {
        if (reverse) {
            readNullable((value))
            read(value)
        } else {
            read(value)
            readNullable((value))
        }
        println(value)
    } else { println("absent") }
}
fun entry(): Unit {
    var slot: String? = "text".clone()
    inspect(slot, false)
    inspect(slot, true)
    slot = "".clone()
    inspect(slot, false)
    inspect(slot, true)
    slot = null
    inspect(slot, false)
}
"#;
    let result = emit_link_and_run("nullable-if-forward-string.ko", source, "entry");
    assert_success(
        &result,
        b"text\ntext\ntext\ntext\ntext\ntext\n\n\n\n\n\n\nabsent\n",
    );
}

#[test]
fn lexical_nullable_when_retires_the_binding_on_a_later_argument_return() {
    let source = r#"class Marker(val label: String) { deinit() { println(label) } }
fun alias(borrow value: String?): borrow String? from value = value
fun read(borrow plain: String, borrow wider: String, own count: Int): Unit {
    println(plain)
    println(wider)
    println("called")
}
fun inspect(inout value: String?, stop: Boolean): Unit {
    val marker = Marker("outer".clone())
    borrow val local = alias(value)
    when (local) {
        null -> println("absent")
        else -> read(local, (local), if (stop) { return } else { 2 })
    }
    println("after")
}
fun entry(): Unit {
    var present: String? = "text".clone()
    inspect(&present, false)
    inspect(&present, true)
    present = "rewritten".clone()
    inspect(&present, false)
    present = "".clone()
    inspect(&present, false)
    inspect(&present, true)
    present = null
    inspect(&present, true)
}
"#;
    let widened = source
        .replace("borrow wider: String", "borrow wider: String?")
        .replace("println(wider)", "if (wider != null) { println(wider) }");
    let expected = concat!(
        "text\ntext\ncalled\nafter\nouter\nouter\n",
        "rewritten\nrewritten\ncalled\nafter\nouter\n",
        "\n\ncalled\nafter\nouter\nouter\nabsent\nafter\nouter\n",
    );
    for (path, source) in [
        ("lexical-when-prefix-return.ko", source),
        ("lexical-when-widened-prefix-return.ko", &widened),
    ] {
        let result = emit_link_and_run(path, source, "entry");
        assert_success(&result, expected.as_bytes());
    }
}

#[test]
fn narrowed_string_can_lend_its_nullable_storage_as_the_call_result_source() {
    let source = r#"fun view(borrow source: String?): borrow String? from source = source
fun read(borrow value: String?): Unit {
    if (value != null) { println(value) } else { println("unexpected null") }
}
fun inspect(inout value: String?, stop: Boolean): Unit {
    if (value != null) {
        borrow val result = view((value))
        read(result)
        if (stop) { return }
        println(value)
    } else { println("absent") }
    value = null
    if (value == null) { println("cleared") }
}
fun entry(): Unit {
    var slot: String? = "text".clone()
    inspect(&slot, true)
    inspect(&slot, false)
    slot = "".clone()
    inspect(&slot, false)
    inspect(&slot, true)
}
"#;
    let result = emit_link_and_run("narrowed-nullable-borrow-source.ko", source, "entry");
    assert_success(
        &result,
        b"text\ntext\ntext\ncleared\n\n\ncleared\nabsent\ncleared\n",
    );
}

#[test]
fn owned_nullable_if_moves_its_string_into_a_call_or_cleans_it_on_argument_return() {
    let source = r#"fun observe(borrow value: String): Unit { println(value) }
fun consume(own value: String, own marker: Int): Unit {
    println(value)
    println("called")
}
fun inspect(own value: String?, stop: Boolean): Unit {
    if (value != null) {
        observe(value)
        consume((value), if (stop) { return } else { 1 })
        println("after")
    } else { println("absent") }
}
fun entry(): Unit {
    inspect("text".clone(), false)
    inspect("early".clone(), true)
    inspect("".clone(), false)
    inspect(null, false)
}
"#;
    let llvm = crate::native_tests::boxed_enum_tests::lower_to_llvm("if-own-string.ko", source);
    let result = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 2);
    assert_success(
        &result,
        b"text\ntext\ncalled\nafter\nearly\n\n\ncalled\nafter\nabsent\n",
    );
}

#[test]
fn owned_nullable_if_cleans_the_payload_and_its_replacement_independently() {
    let source = r#"fun consume(own value: String, own marker: Int): Unit { println(value) }
fun inspect(stop: Boolean): Unit {
    var value: String? = "old".clone()
    if (value != null) {
        if (value != null) {
            consume(value, if (stop) {
                value = "new".clone()
                return
            } else {
                value = "new".clone()
                1
            })
        }
    }
    if (value != null) { println(value) }
}
fun entry(): Unit {
    inspect(true)
    inspect(false)
}
"#;
    let llvm =
        crate::native_tests::boxed_enum_tests::lower_to_llvm("if-own-string-replace.ko", source);
    let result = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 4);
    assert_success(&result, b"old\nnew\n");
}

#[test]
fn borrowed_nullable_string_result_while_with_closed_sibling() {
    let source = r#"class Marker(val label: String) {
    deinit() { println(label) }
}

fun view(borrow source: String?): borrow String? from source = source

fun observe(borrow value: String?): Unit {
    if (value != null) { println(value) }
}
fun inspect(inout source: String?): Unit {
    block {
        block {
            borrow val other = view(source)
            observe(other)
        }
        borrow val result = view(source)
        var first = true
        while (result != null) {
            val marker = Marker("iteration drop".clone())
            println(result)
            if (first) {
                first = false
                continue
            }
            break
        }
        if (result == null) { println("missing") }
        println("loop exit")
    }
    source = null
}

fun entry(): Unit {
    var source: String? = "".clone()
    inspect(&source)
    inspect(&source)
    source = "hit".clone()
    inspect(&source)
}
"#;
    let result = emit_link_and_run("nullable-result-while-closed-sibling.ko", source, "entry");
    assert_success(&result, b"\n\niteration drop\n\niteration drop\nloop exit\nmissing\nloop exit\nhit\nhit\niteration drop\nhit\niteration drop\nloop exit\n");
}

#[test]
fn borrowed_nullable_string_result_while_in_sequential_block_with_live_sibling() {
    let source = r#"class Marker(val label: String) {
    deinit() { println(label) }
}

fun view(borrow source: String?): borrow String? from source = source

fun inspect(inout source: String?): Unit {
    block {
        borrow val other = view(source)
        borrow val result = view(source)
        var first = true
        block {
            while (result != null) {
                val marker = Marker("iteration drop".clone())
                println(result)
                if (first) {
                    first = false
                    continue
                }
                break
            }
        }
        if (other == null) { println("missing") }
        println("loop exit")
    }
    source = null
}

fun entry(): Unit {
    var source: String? = "".clone()
    inspect(&source)
    inspect(&source)
    source = "hit".clone()
    inspect(&source)
}
"#;
    let result = emit_link_and_run(
        "nullable-result-while-nested-live-sibling.ko",
        source,
        "entry",
    );
    assert_success(&result, b"\niteration drop\n\niteration drop\nloop exit\nmissing\nloop exit\nhit\niteration drop\nhit\niteration drop\nloop exit\n");
}

#[test]
fn borrowed_nullable_string_result_while_with_plain_peer() {
    let source = r#"fun viewPlain(borrow source: String): borrow String from source = source
class Marker(val label: String) {
    deinit() { println(label) }
}

fun view(borrow source: String?): borrow String? from source = source

fun inspect(inout source: String?): Unit {
    block {
        val second = "independent".clone()
        borrow val other = viewPlain(second)
        borrow val result = view(source)
        var first = true
        while (result != null) {
            val marker = Marker("iteration drop".clone())
            println(result)
            if (first) {
                first = false
                continue
            }
            break
        }
        if (other == "independent") { println(other) }
        println("loop exit")
    }
    source = null
}

fun entry(): Unit {
    var source: String? = "".clone()
    inspect(&source)
    inspect(&source)
    source = "hit".clone()
    inspect(&source)
}
"#;
    let result = emit_link_and_run("nullable-result-while-plain-peer.ko", source, "entry");
    assert_success(&result, b"\niteration drop\n\niteration drop\nindependent\nloop exit\nindependent\nloop exit\nhit\niteration drop\nhit\niteration drop\nindependent\nloop exit\n");
}

#[test]
fn borrowed_nullable_string_result_while_with_plain_closed() {
    let source = r#"fun viewPlain(borrow source: String): borrow String from source = source
class Marker(val label: String) {
    deinit() { println(label) }
}

fun view(borrow source: String?): borrow String? from source = source

fun inspect(inout source: String?): Unit {
    block {
        block {
            val second = "independent".clone()
            borrow val other = viewPlain(second)
            println(other)
        }
        borrow val result = view(source)
        var first = true
        while (result != null) {
            val marker = Marker("iteration drop".clone())
            println(result)
            if (first) {
                first = false
                continue
            }
            break
        }
        if (result == null) { println("missing") }
        println("loop exit")
    }
    source = null
}

fun entry(): Unit {
    var source: String? = "".clone()
    inspect(&source)
    inspect(&source)
    source = "hit".clone()
    inspect(&source)
}
"#;
    let result = emit_link_and_run("nullable-result-while-plain-closed.ko", source, "entry");
    assert_success(&result, b"independent\n\niteration drop\n\niteration drop\nloop exit\nindependent\nmissing\nloop exit\nindependent\nhit\niteration drop\nhit\niteration drop\nloop exit\n");
}

#[test]
fn mixed_string_loans_return_after_loop() {
    let source = r#"fun viewPlain(borrow source: String): borrow String from source = source
class Marker(val label: String) {
    deinit() { println(label) }
}

fun view(borrow source: String?): borrow String? from source = source

fun inspect(inout source: String?): Unit {
    block {
        val cleanup = Marker("scope drop".clone())
        val second = "independent".clone()
        borrow val other = viewPlain(second)
        borrow val result = view(source)
        var first = true
        while (result != null) {
            val marker = Marker("iteration drop".clone())
            println(result)
            if (first) {
                first = false
                continue
            }
            break
        }
        if (other == "independent") { println(other) }
        println("loop exit")
        return
    }
}

fun entry(): Unit {
    var source: String? = "".clone()
    inspect(&source)
    source = null
    inspect(&source)
    source = "hit".clone()
    inspect(&source)
}
"#;
    let result = emit_link_and_run("mixed-loans-return-after-loop.ko", source, "entry");
    assert_success(&result, b"\niteration drop\n\niteration drop\nindependent\nloop exit\nscope drop\nindependent\nloop exit\nscope drop\nhit\niteration drop\nhit\niteration drop\nindependent\nloop exit\nscope drop\n");
}

#[test]
fn mixed_string_loans_return_in_loop() {
    let source = r#"fun viewPlain(borrow source: String): borrow String from source = source
class Marker(val label: String) {
    deinit() { println(label) }
}

fun view(borrow source: String?): borrow String? from source = source

fun inspect(inout source: String?): Unit {
    block {
        val cleanup = Marker("scope drop".clone())
        val second = "independent".clone()
        borrow val other = viewPlain(second)
        borrow val result = view(source)
        var first = true
        while (result != null) {
            val marker = Marker("iteration drop".clone())
            println(result)
            if (first) {
                first = false
                continue
            }
            return
        }
        if (other == "independent") { println(other) }
        println("loop exit")
    }
    source = null
}

fun entry(): Unit {
    var source: String? = "".clone()
    inspect(&source)
    source = null
    inspect(&source)
    source = "hit".clone()
    inspect(&source)
}
"#;
    let result = emit_link_and_run("mixed-loans-return-in-loop.ko", source, "entry");
    assert_success(&result, b"\niteration drop\n\niteration drop\nscope drop\nindependent\nloop exit\nscope drop\nhit\niteration drop\nhit\niteration drop\nscope drop\n");
}

#[test]
fn mixed_string_loans_branch_while() {
    let source = r#"fun viewPlain(borrow source: String): borrow String from source = source
class Marker(val label: String) {
    deinit() { println(label) }
}

fun view(borrow source: String?): borrow String? from source = source

fun inspect(inout source: String?, enabled: Boolean): Unit {
    block {
        val cleanup = Marker("scope drop".clone())
        val second = "independent".clone()
        borrow val other = viewPlain(second)
        borrow val result = view(source)
        var first = true
        if (enabled) {
            while (result != null) {
                val marker = Marker("iteration drop".clone())
                println(result)
                if (first) {
                    first = false
                    continue
                }
                break
            }
        }
        if (other == "independent") { println(other) }
        println("loop exit")
    }
    source = null
}

fun entry(): Unit {
    var source: String? = "".clone()
    inspect(&source, true)
    inspect(&source, true)
    source = "hit".clone()
    inspect(&source, true)
    source = "skip".clone()
    inspect(&source, false)
}
"#;
    let result = emit_link_and_run("mixed-loans-branch-while.ko", source, "entry");
    assert_success(&result, b"\niteration drop\n\niteration drop\nindependent\nloop exit\nscope drop\nindependent\nloop exit\nscope drop\nhit\niteration drop\nhit\niteration drop\nindependent\nloop exit\nscope drop\nindependent\nloop exit\nscope drop\n");
}

#[test]
fn mixed_string_loans_inside_independent_nullable_when() {
    let source = r#"fun viewPlain(borrow source: String): borrow String from source = source
fun view(borrow source: String?): borrow String? from source = source
class Marker(val label: String) {
    deinit() { println(label) }
}
fun inspect(inout source: String?, own discriminator: String?, early: Boolean): Unit {
    block {
        val scope = Marker("scope drop".clone())
        val second = "independent".clone()
        borrow val other = viewPlain(second)
        borrow val result = view(source)
        when (discriminator) {
            null -> println("skipped")
            else -> {
                while (result != null) {
                    val iteration = Marker("iteration drop".clone())
                    println(result)
                    if (early) { return }
                    break
                }
            }
        }
        if (other == "independent") { println(other) }
    }
    source = null
}
fun entry(): Unit {
    var source: String? = "".clone()
    inspect(&source, "".clone(), false)
    inspect(&source, "selected".clone(), false)
    source = "hit".clone()
    inspect(&source, "selected".clone(), false)
    source = "unused".clone()
    inspect(&source, null, false)
    source = "early".clone()
    inspect(&source, "selected".clone(), true)
    source = null
}
"#;
    let result = emit_link_and_run("mixed-loans-nullable-when.ko", source, "entry");
    assert_success(
        &result,
        b"\niteration drop\nindependent\nscope drop\nindependent\nscope drop\nhit\niteration drop\nindependent\nscope drop\nskipped\nindependent\nscope drop\nearly\niteration drop\nscope drop\n",
    );
}

#[test]
fn independent_owned_nullable_when_observes_without_a_loop() {
    let source = r#"fun viewPlain(borrow source: String): borrow String from source = source
fun inspect(own discriminator: String?): Unit {
    val source = "independent".clone()
    borrow val result = viewPlain(source)
    when (discriminator) {
        null -> println("null")
        else -> println("present")
    }
    println(result)
}
fun entry(): Unit {
    inspect("".clone())
    inspect(null)
}
"#;
    let result = emit_link_and_run("independent-when-no-loop.ko", source, "entry");
    assert_success(&result, b"present\nindependent\nnull\nindependent\n");
}
