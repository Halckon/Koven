//! SPEC-0241: exact Guide source and payloadless Copyable enum conditions.
use super::boxed_enum_tests::assert_success;
use super::emit_link_and_run;

pub(crate) fn guide_litmus_04() -> &'static str {
    let guide = include_str!("../../../docs/guide/15-conformance-and-staging.md");
    let section = guide
        .split_once("### Litmus 4: ")
        .unwrap()
        .1
        .split_once("### Litmus 5: ")
        .unwrap()
        .0;
    assert_eq!(section.matches("```kotlin\n").count(), 1);
    section
        .split_once("```kotlin\n")
        .unwrap()
        .1
        .split_once("\n```")
        .unwrap()
        .0
}

#[test]
fn guide_litmus_04_return_when_runs_natively_without_rewriting_the_guide() {
    let source = format!(
        "{}\n{}",
        guide_litmus_04(),
        r#"fun entry(): Unit {
    val circle: Shape = Shape.Circle(2)
    val rectangle: Shape = Shape.Rectangle(4, 5)
    val point: Shape = Shape.Point
    if (area(circle) == 12) println("12") else println("bad-circle")
    if (area(rectangle) == 20) println("20") else println("bad-rectangle")
    if (area(point) == 0) println("0") else println("bad-point")
}"#
    );
    let run = emit_link_and_run("demo/enums/litmus4.ko", &source, "entry");
    assert_success(&run, b"12\n20\n0\n");
}

#[test]
fn payloadless_enum_conditions_use_case_identity_and_declaration_order() {
    let source = r#"
enum class First { On, Off }
enum class Second { Off, On }
fun first(value: First): Int { return when (value) { First.On -> 11; First.Off -> 12 } }
fun second(value: Second): Int { return when (value) { Second.On -> 21; Second.Off -> 22 } }
fun entry(): Unit {
    val a: First = First.On
    val b: First = First.Off
    val c: Second = Second.On
    val d: Second = Second.Off
    if (first(a) == 11) println("11") else println("bad-a")
    if (first(b) == 12) println("12") else println("bad-b")
    if (second(c) == 21) println("21") else println("bad-c")
    if (second(d) == 22) println("22") else println("bad-d")
}
"#;
    assert_success(
        &emit_link_and_run("enum-tag-identity.ko", source, "entry"),
        b"11\n12\n21\n22\n",
    );
}

#[test]
fn return_if_and_return_when_compose_with_native_branch_values() {
    let source = r#"
fun choose(flag: Boolean): Int { return if (flag) when (flag) { true -> 7; false -> 8 } else 9 }
fun entry(): Unit {
    if (choose(true) == 7) println("7") else println("bad-true")
    if (choose(false) == 9) println("9") else println("bad-false")
}
"#;
    assert_success(
        &emit_link_and_run("return-control.ko", source, "entry"),
        b"7\n9\n",
    );
}

#[test]
fn ordinary_short_circuit_return_keeps_the_when_bypass_owner() {
    for (selector, arms, when_true, when_false) in [
        (
            "flag && pred(return 7)",
            "-> 2; else -> { observe(x); 3 }",
            7,
            3,
        ),
        (
            "flag || pred(return 7)",
            "-> { observe(x); 2 }; else -> 3",
            2,
            7,
        ),
    ] {
        let source = format!(
            r#"class Node(val tag: Int)
fun observe(borrow value: Node): Unit {{ if (value.tag == 1) {{ println("observe") }} }}
fun pred(value: Int): Boolean {{ println("pred must not run"); return true }}
fun test(flag: Boolean, own x: Node): Int {{
    return when {{ {selector} {arms} }}
}}
fun main(): Unit {{
    if (test(true, Node(1)) != {when_true} || test(false, Node(1)) != {when_false}) {{
        error("wrong result")
    }}
    println("ok")
}}
"#
        );
        let output = emit_link_and_run("when-short-circuit-owner.ko", &source, "main");
        assert_success(&output, b"observe\nok\n");
    }
}

#[test]
fn ordinary_short_circuit_return_prunes_unreachable_native_edges() {
    for (body, when_true, when_false) in [
        (
            "return if (flag && pred(return 7)) { reads(x); 2 } else 3",
            7,
            3,
        ),
        (
            "while (flag && pred(return 7)) { reads(x) }\nreturn 3",
            7,
            3,
        ),
        (
            "while (flag || pred(return 7)) { return 2 }\nreads(x)\nreturn 3",
            2,
            7,
        ),
        ("while (flag || pred(return 7)) { break }\nreturn 3", 3, 7),
        (
            "if ((flag && pred(return 7)) && reads(x)) { return 2 }\nreturn 3",
            7,
            3,
        ),
        (
            "if ((flag || pred(return 7)) && pred(9)) { return 2 }\nreturn 3",
            2,
            7,
        ),
    ] {
        let source = format!(
            r#"class Node(val tag: Int)
fun reads(borrow value: Node): Boolean = value.tag == 1
fun consume(own value: Node): Unit {{}}
fun pred(value: Int): Boolean {{ println("pred"); return true }}
fun test(flag: Boolean, own x: Node): Int {{
    consume(x)
    {body}
}}
fun main(): Unit {{
    if (test(true, Node(1)) != {when_true} || test(false, Node(1)) != {when_false}) {{
        error("wrong result")
    }}
    println("ok")
}}
"#
        );
        let expected = if body.contains("pred(9)") {
            b"pred\nok\n".as_slice()
        } else {
            b"ok\n".as_slice()
        };
        let run = || emit_link_and_run("ordinary-short-circuit-edges.ko", &source, "main");
        assert_success(&run(), expected);
        assert_success(&crate::ssa::with_per_family_lowering(run), expected);
    }
}

#[test]
fn ordinary_short_circuit_required_rhs_consumes_each_owner_once() {
    let source = r#"
class Node(val tag: Int) {
    deinit() { println("drop") }
}
fun take(own value: Node): Boolean {
    if (value.tag != 1) { error("wrong owner") }
    println("take")
    return true
}
fun test(own x: Node, own y: Node): Unit {
    val a = (true) && take(x)
    val b = !true || take(y)
    if (!(a && b)) { error("wrong result") }
}
fun main(): Unit {
    test(Node(1), Node(1))
    println("ok")
}
"#;
    let run = || emit_link_and_run("ordinary-required-rhs.ko", source, "main");
    let expected = b"take\ndrop\ntake\ndrop\nok\n";
    assert_success(&run(), expected);
    assert_success(&crate::ssa::with_per_family_lowering(run), expected);
}

#[test]
fn constant_short_circuit_when_cleans_the_actual_skipped_owner() {
    for (operator, taken, skipped) in [("||", false, true), ("&&", true, false)] {
        let skipped_result = if skipped { 2 } else { 3 };
        let source = format!(
            r#"const val Answer: Int = 2
fun take(own text: String): Boolean {{ println(text); return true }}
fun takeAfter(own text: String, value: Int): Boolean {{ println("unexpected"); return true }}
fun choose(flag: Boolean, own text: String): Int {{
    return when {{ flag {operator} take(text) -> Answer; else -> 3 }}
}}
fun chooseExit(flag: Boolean, own text: String): Int {{
    return when {{ flag {operator} takeAfter(text, return 7) -> Answer; else -> 3 }}
}}
fun main(): Unit {{
    if (chooseExit({skipped}, "skipped exit") != {skipped_result}) {{ error("wrong skip result") }}
    if (chooseExit({taken}, "RHS exit") != 7) {{ error("wrong early RHS result") }}
    if (choose({skipped}, "skipped") != {skipped_result}) {{ error("wrong skip result") }}
    if (choose({taken}, "taken") != Answer) {{ error("wrong RHS result") }}
    println("ok")
}}
"#
        );
        assert_success(
            &emit_link_and_run("const-when-short-circuit-owner.ko", &source, "main"),
            b"taken\nok\n",
        );
    }
}

#[test]
fn an_entered_call_keeps_the_named_prefix_before_a_dead_callback() {
    let source = r#"
fun tick(value: Int): Int { println("prefix"); return value }
fun apply(first: Int, second: Int, action: () -> Unit): Boolean { action(); return true }
fun choose(): Int {
    apply(second = tick(1), first = return 7, action = { println("dead") })
    return 3
}
fun chooseCallbackExit(): Int {
    return when {
        apply(second = tick(3), first = 1, action = return 9) -> 2
        else -> 3
    }
}
fun applyInout(inout item: Int, first: Int, second: Int, action: () -> Unit): Unit {
    item = 9
    action()
}
fun chooseInout(): Int {
    var item = 1
    applyInout(item = &item, second = tick(4), first = return 11, action = { println("dead inout") })
    return 4
}
fun invokeLive(): Unit {
    apply(second = tick(2), first = 1, action = { println("live") })
}
fun main(): Unit {
    if (choose() != 7 || chooseCallbackExit() != 9 || chooseInout() != 11) {
        error("wrong result")
    }
    invokeLive()
    println("ok")
}
"#;
    assert_success(
        &emit_link_and_run("call-exiting-before-callback.ko", source, "main"),
        b"prefix\nprefix\nprefix\nprefix\nlive\nok\n",
    );
}

#[test]
fn method_receiver_exit_precedes_callback_argument_evaluation() {
    let source = r#"
class Worker {
    fun apply(value: Int, action: () -> Unit): Unit { action() }
    fun ack(value: Int): Unit { println("unexpected ack") }
}
fun receiverPrefix(): Int { println("receiver prefix"); return 1 }
fun argument(): Int { println("argument"); return 2 }
fun make(first: Int, second: Int): Worker { println("receiver"); return Worker() }
fun choose(): Int {
    make(receiverPrefix(), return 7).apply(argument(), { println("dead") })
    return 3
}
fun legacyChoose(): Int {
    make(receiverPrefix(), return 7).ack(argument())
    return 3
}
fun legacy(): Unit {
    if (legacyChoose() != 7) { error("wrong result") }
    println("ok")
}
fun main(): Unit {
    if (choose() != 7) { error("wrong result") }
    make(1, 2).apply(argument(), { println("live") })
    println("ok")
}
"#;
    assert_success(
        &emit_link_and_run("method-receiver-exit.ko", source, "main"),
        b"receiver prefix\nreceiver\nargument\nlive\nok\n",
    );
    let compatibility = crate::ssa::with_per_family_lowering(|| {
        emit_link_and_run("method-receiver-exit.ko", source, "legacy")
    });
    assert_success(&compatibility, b"receiver prefix\nok\n");
}

#[test]
fn an_exiting_call_prefix_does_not_emit_a_dormant_callback_callee() {
    use super::boxed_enum_tests::unit_source::UnitSource;
    use crate::ssa::model::Origin;
    use lang_frontend::name_resolution::SymbolKind;

    let source = r#"
fun tick(): Int { println("prefix"); return 1 }
fun dormant(first: Int, second: Int, callback: () -> Unit): Unit { callback() }
fun choose(): Int {
    dormant(tick(), return 7, { println("dead") })
    return 3
}
fun main(): Unit {
    if (choose() != 7) { error("wrong result") }
    println("ok")
}
"#;
    let unit = UnitSource::analyze("dormant-callback-callee.ko", source).unwrap();
    unit.assert_frontend_clean();
    let dormant = unit.declaration("dormant", SymbolKind::Function);
    let origin = Origin::Source(unit.declaration_span(dormant));
    let (program, _) = unit
        .lower_entry(unit.declaration("main", SymbolKind::Function))
        .unwrap();
    assert!(
        program
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .all(|function| function.origin != origin),
        "an exiting operand prefix must not require the callee body"
    );
    let output = crate::ssa::with_per_family_lowering(|| {
        emit_link_and_run("dormant-callback-callee.ko", source, "main")
    });
    assert_success(&output, b"prefix\nok\n");
}

#[test]
fn an_exiting_call_prefix_can_form_a_callable_view_before_returning() {
    for operand in ["{ println(marker) }", "({ println(marker) })"] {
        let source = format!(
            r#"
fun dormant(callback: () -> Unit, second: Int): Unit {{ callback() }}
fun choose(): Int {{
    val marker = "callback body"
    dormant({operand}, return 7)
    return 3
}}
fun main(): Unit {{
    if (choose() != 7) {{ error("wrong result") }}
    println("ok")
}}
"#
        );
        assert_success(
            &emit_link_and_run("completed-callback-prefix.ko", &source, "main"),
            b"ok\n",
        );
    }
}

#[test]
fn an_exiting_call_prefix_drops_a_consumed_nullable_when_subject() {
    let source = r#"
class Node { deinit() { println("drop") } }
fun dormant(own value: Node, own result: Int): Unit { println("dispatched") }
fun choose(own subject: Node?): Unit {
    when (subject) {
        null -> {};
        else -> { dormant((subject), return) }
    }
    println("after")
}
fun main(): Unit {
    val present: Node? = Node()
    choose(present)
    choose(null)
    println("ok")
}
"#;
    assert_success(
        &emit_link_and_run("nullable-when-operand-exit.ko", source, "main"),
        b"drop\nafter\nok\n",
    );
}

#[test]
fn an_abandoned_borrow_result_call_cleans_up_a_nullable_when_argument() {
    let source = r#"
class Node { deinit() { println("drop") } }
fun dormant(MODE value: Node, source: String, stop: Boolean): borrow String from source {
    println("dispatched")
    return source
}
fun choose(source: String, own subject: Node?): borrow String from source {
    when (subject) {
        null -> { return source };
        else -> { return dormant((subject), source, return source) }
    }
    return source
}
fun main(): Unit {
    val source = "owner".clone()
    val present: Node? = Node()
    block { borrow val result = choose(source, present); println(result) }
    block { borrow val result = choose(source, null); println(result) }
    println(source)
}
"#;
    for (name, mode) in [("observe", ""), ("consume", "own ")] {
        let source = source.replace("MODE ", mode);
        assert_success(
            &emit_link_and_run(&format!("abandoned-borrow-when-{name}.ko"), &source, "main"),
            b"drop\nowner\nowner\nowner\n",
        );
    }
}

#[test]
fn exhaustive_when_inside_loop_has_no_phantom_resource_exit() {
    let source = r#"
class Ticket {
    fun observe(): Unit { println("observe") }
    deinit() { println("drop") }
}
fun discard(own ticket: Ticket): Unit { println("unexpected discard") }
fun choose(flag: Boolean, own ticket: Ticket): Int {
    loop {
        when (flag) {
            (true) -> { ticket.observe(); return 7 };
            false -> { ticket.observe(); return 8 };
            else -> { discard(ticket); discard(ticket); break }
        }
        break
    }
}
fun deadSelector(own ticket: Ticket): Boolean { println("unexpected selector"); return false }
fun covered(flag: Boolean, own ticket: Ticket): Int {
    loop {
        when (flag) {
            true, false, deadSelector(ticket), deadSelector(ticket) -> { ticket.observe(); return 9 }
        }
    }
}
fun main(): Unit {
    if (covered(true, Ticket()) != 9) { error("wrong covered branch") }
    if (choose(true, Ticket()) != 7) { error("wrong true branch") }
    if (choose(false, Ticket()) != 8) { error("wrong false branch") }
    println("ok")
}
"#;
    assert_success(
        &emit_link_and_run("exhaustive-when-loop-resource.ko", source, "main"),
        b"observe\ndrop\nobserve\ndrop\nobserve\ndrop\nok\n",
    );
}

#[test]
fn call_argument_while_return_keeps_prefix_loan_on_each_reachable_edge() {
    let source = r#"
fun consume(text: String, flag: Boolean): Unit { println(text) }
fun work(outer: Boolean, loopFlag: Boolean): Unit {
    consume("outer" + " loan", if (outer) {
        while (loopFlag) { return }
        return
    } else { true })
}
fun main(): Unit {
    work(true, false)
    work(true, true)
    work(false, false)
    println("done")
}
"#;
    assert_success(
        &emit_link_and_run("argument-while-return.ko", source, "main"),
        b"outer loan\ndone\n",
    );
}

#[test]
fn grouped_call_argument_while_return_uses_the_same_prefix_owner() {
    let source = r#"
fun consume(text: String, flag: Boolean): Unit { println(text) }
fun work(outer: Boolean, flag: Boolean): Unit {
    consume(("outer" + " loan"), if (outer) {
        while (flag) { return }
        return
    } else { true })
}
fun main(): Unit {
    work(true, false)
    work(true, true)
    work(false, false)
    println("done")
}
"#;
    crate::ssa::with_per_family_lowering(|| {
        assert_success(
            &emit_link_and_run("grouped-argument-while-return.ko", source, "main"),
            b"outer loan\ndone\n",
        );
    });
}

#[test]
fn call_argument_diverging_loop_skips_dispatch_only_on_the_return_edge() {
    let source = r#"
fun consume(text: String, flag: Boolean): Unit {
    if (!flag) { error("wrong delivered value") }
    println(text)
}
fun work(outer: Boolean): Unit {
    consume("outer" + " loan", if (outer) {
        loop { return }
    } else { true })
}
fun main(): Unit {
    work(true)
    work(false)
    println("done")
}
"#;
    let check = || {
        assert_success(
            &emit_link_and_run("argument-diverging-loop.ko", source, "main"),
            b"outer loan\ndone\n",
        );
    };
    check();
    crate::ssa::with_per_family_lowering(check);
}

#[test]
fn boxed_enum_when_skips_closed_selectors_with_arm_local_borrow() {
    let source = r#"
enum class Packet { Full(text: String), Empty }
fun never(own packet: Packet): Packet { println("unexpected selector"); return packet }
fun view(source: String): borrow String from source = source
fun probe(own packet: Packet, stop: Boolean): Unit {
    val source = "owner".clone()
    when ((packet)) {
        is Packet.Full -> {
            borrow val item = view(source)
            if (stop) { println(item); return }
            println(item)
        };
        !is Packet.Full, never(Packet.Empty) -> { println("empty") }
    }
    println("joined")
    return
}
fun main(): Unit {
    probe(Packet.Full("payload".clone()), true)
    probe(Packet.Full("payload".clone()), false)
    probe(Packet.Empty, false)
}
"#;
    assert_success(
        &emit_link_and_run("lexical-borrow-boxed-enum-when.ko", source, "main"),
        b"owner\nowner\njoined\nempty\njoined\n",
    );
}

#[test]
fn resource_deinit_native_terminal_constructor_and_rc_members_keep_only_completed_prefixes() {
    let source = r#"
class Leaf(val name: String) { deinit() { println(name) } }
class Outer(val first: Leaf, val second: Leaf, val code: Int, val suffix: Leaf) {
    deinit() { println("outer") }
}
fun inspect(borrow first: Leaf, borrow second: Leaf): Unit { println("unreachable") }
fun constructor(): Unit {
    val owner = Rc(Outer(Leaf("first".clone()), Leaf("second".clone()),
        if (true) { println("return"); return } else { return },
        Leaf("suffix".clone()))).share()
}
fun borrowPrefix(): Unit {
    inspect(Rc(Leaf("prefix".clone())).value, Rc<Leaf>(return).value)
}
fun fields(): Unit {
    val name = Rc<Leaf>(return).value.name.clone()
    println(name)
}
fun entry(): Unit {
    constructor(); borrowPrefix(); fields()
    println("done")
}
"#;
    let ir = super::boxed_enum_tests::lower_to_llvm("terminal-rc-member-prefixes.ko", source);
    let output =
        super::boxed_enum_tests::run_counted_allocations_in_order(&ir, &[2, 3, 0, 1, 4, 5, 6]);
    super::boxed_enum_tests::assert_success(&output, b"return\nsecond\nfirst\nprefix\ndone\n");
}

#[test]
fn resource_deinit_native_nested_rc_loans_end_before_release() {
    let source = r#"
class Leaf(val name: String) { deinit() { println("leaf") } }
class Holder(val inner: Rc<Leaf>) { deinit() { println("holder") } }
fun show(borrow leaf: Leaf, ready: Boolean): Unit { if (ready) { println(leaf.name) } }
fun lend(stop: Boolean): Unit {
    show(Rc(Rc(Leaf("payload".clone()))).value.value,
        if (stop) { println("return"); return } else { true })
    println("tail")
}
fun share(): Unit {
    val retained = Rc(Holder(Rc(Leaf("kept".clone())))).value.inner.share()
    println(retained.value.name)
    println("held")
}
fun entry(): Unit { lend(true); lend(false); share(); println("done") }
"#;
    let ir = super::boxed_enum_tests::lower_to_llvm("nested-rc-loans.ko", source);
    let output = super::boxed_enum_tests::run_counted_allocations_in_order(
        &ir,
        &[0, 1, 2, 3, 4, 5, 6, 7, 11, 12, 8, 9, 10],
    );
    super::boxed_enum_tests::assert_success(
        &output,
        b"return\nleaf\npayload\nleaf\ntail\nholder\nkept\nheld\nleaf\ndone\n",
    );
}

#[test]
fn resource_deinit_native_borrowed_nullable_rc_preserves_the_parent_through_short_circuit() {
    let source = r#"
class Leaf(val name: String) { deinit() { println("drop") } }
fun read(borrow owner: Rc<Leaf>): Unit { println(owner.value.name) }
fun test(borrow owner: Rc<Leaf>?, gate: Boolean): Unit {
    if (!(owner != null)) { println("null"); return }
    if ((gate && owner != null) || owner != null) {
        read((owner))
        val retained = owner.share()
        read(retained)
    }
    println("tail")
}
fun entry(): Unit {
    val absent: Rc<Leaf>? = null
    val present: Rc<Leaf>? = Rc(Leaf("leaf".clone()))
    test(absent, false)
    test(present, false)
    test(present, true)
    println("done")
}
"#;
    let ir = super::boxed_enum_tests::lower_to_llvm("borrowed-nullable-rc.ko", source);
    let output = super::boxed_enum_tests::run_counted_allocations_in_order(&ir, &[0, 1, 2]);
    super::boxed_enum_tests::assert_success(
        &output,
        b"null\nleaf\nleaf\ntail\nleaf\nleaf\ntail\ndone\ndrop\n",
    );
}

#[test]
fn singleton_abandoned_box_and_containers_retire_elements_and_backing_storage() {
    let source = r#"
class Leaf(val label: String) { deinit() { println(label) } }
enum class Packet { Full(value: Leaf), Empty }
object Gate {
    fun boxed(value: Box<Packet>, stop: Int): Unit { println("dispatch") }
    fun optional(value: Box<Packet>?, stop: Int): Unit { println("dispatch") }
    fun array(value: Array<Leaf>, stop: Int): Unit { println("dispatch") }
    fun list(value: List<Leaf>, stop: Int): Unit { println("dispatch") }
    fun mutableList(value: MutableList<Leaf>, stop: Int): Unit { println("dispatch") }
    fun map(value: Map<Int, Leaf>, stop: Int): Unit { println("dispatch") }
    fun mutableMap(value: MutableMap<Int, Leaf>, stop: Int): Unit { println("dispatch") }
    fun optionalMap(value: Map<Int, Leaf>?, stop: Int): Unit { println("dispatch") }
}
fun maybe(present: Boolean): Box<Packet>? {
    if (present) { return Box(Packet.Full(Leaf("nullable"))) }
    return null
}
fun makeMap(): MutableMap<Int, Leaf> {
    var values = mutableMapOf<Int, Leaf>()
    values.put(1, Leaf("map"))
    return values
}
fun makeImmutable(): Map<Int, Leaf> {
    var values = mutableMapOf<Int, Leaf>()
    values.put(1, Leaf("immutable-map"))
    return values.consume()
}
fun maybeMap(present: Boolean): Map<Int, Leaf>? {
    if (!present) { return null }
    var entries = mutableMapOf<Int, Leaf>()
    entries.put(1, Leaf("nullable-map"))
    return entries.consume()
}
fun abandonMap(present: Boolean): Unit { Gate.optionalMap(maybeMap(present), return) }
fun abandonOptional(present: Boolean): Unit { Gate.optional(maybe(present), return) }
fun abandonList(): Unit { Gate.list(listOf(Leaf("list")), return) }
fun entry(): Unit {
    loop { Gate.boxed(Box(Packet.Full(Leaf("box"))), break) }
    var again = true
    while (again) {
        again = false
        Gate.array(arrayOf(Leaf("array")), continue)
    }
    abandonList()
    loop { Gate.mutableList(mutableListOf(Leaf("mutable-list")), break) }
    loop { Gate.map(mapOf<Int, Leaf>(), break) }
    loop { Gate.map(makeImmutable(), break) }
    loop { Gate.mutableMap(makeMap(), break) }
    abandonOptional(true)
    abandonOptional(false)
    abandonMap(true)
    abandonMap(false)
    println("done")
}
"#;
    let llvm = super::boxed_enum_tests::lower_to_llvm("singleton-container-prefix.ko", source);
    // Eight nonempty owners each allocate a Leaf and backing storage.
    // The empty map still allocates buckets; the null handles allocate nothing.
    let run = super::boxed_enum_tests::run_counted_allocations(&llvm, 17);
    super::boxed_enum_tests::assert_success(
        &run,
        b"box\narray\nlist\nmutable-list\nimmutable-map\nmap\nnullable\nnullable-map\ndone\n",
    );
}

#[test]
fn singleton_mixed_argument_dispatch_uses_the_continuing_arm_only() {
    for (control, body, expected) in [
        (
            "break",
            "loop { CALL; break }; println(\"tail\")",
            b"tail\ndone\nsuffix\ndispatch\ntail\ndone\n".as_slice(),
        ),
        (
            "continue",
            "var again = true; while (condition(again)) { again = false; CALL }; println(\"tail\")",
            b"condition\ncondition\ntail\ndone\ncondition\nsuffix\ndispatch\ncondition\ntail\ndone\n".as_slice(),
        ),
        (
            "return",
            "CALL; println(\"tail\")",
            b"done\nsuffix\ndispatch\ntail\ndone\n".as_slice(),
        ),
    ] {
        let body = body.replace(
            "CALL",
            &format!("Gate.choose(7, if (stop) {control} else 0, suffix())"),
        );
        let source = format!(
            r#"
object Gate {{ fun choose(first: Int, stop: Int, last: Int): Unit {{ println("dispatch") }} }}
fun suffix(): Int {{ println("suffix"); return 0 }}
fun condition(active: Boolean): Boolean {{ println("condition"); return active }}
fun exercise(stop: Boolean): Unit {{ {body} }}
fun main(): Unit {{ exercise(true); println("done"); exercise(false); println("done") }}
"#
        );
        let output = super::emit_link_and_run("singleton-mixed-scalar.ko", &source, "main");
        super::boxed_enum_tests::assert_success(&output, expected);
    }
}

#[test]
fn singleton_mixed_dispatch_releases_its_own_resources_on_each_edge() {
    let source = r#"
class Leaf { deinit() { println("drop-leaf") } }
fun makeText(): String { println("text-prefix"); return "text-argument".clone() }
fun makeOwner(): Rc<Leaf> { println("rc-prefix"); return Rc(Leaf()) }
fun suffix(): Int { println("suffix"); return 0 }
object Gate {
    fun inspect(owner: Rc<Leaf>?, text: String, stop: Int, last: Int): Unit {
        println("dispatch")
        println(text)
    }
}
fun exercise(stop: Boolean): Unit {
    loop {
        Gate.inspect(text = makeText(), owner = makeOwner(),
                     stop = if (stop) break else 0, last = suffix())
        break
    }
    println("done")
}
fun entry(): Unit { exercise(true); exercise(false) }
"#;
    let llvm = super::boxed_enum_tests::lower_to_llvm("singleton-mixed-owner.ko", source);
    let run = super::boxed_enum_tests::run_counted_allocations(&llvm, 6);
    super::boxed_enum_tests::assert_success(
        &run,
        b"text-prefix\nrc-prefix\ndrop-leaf\ndone\ntext-prefix\nrc-prefix\nsuffix\ndispatch\ntext-argument\ndrop-leaf\ndone\n",
    );
}

#[test]
fn singleton_string_entry_return_retires_temporary_arguments_and_preserves_source() {
    for (value, first) in [
        ("source", "make\ndrop-leaf\nheld\n"),
        ("relay(source)", "make\nrelay\ndrop-leaf\nheld\n"),
    ] {
        let source = format!(
            r#"
class Leaf {{ deinit() {{ println("drop-leaf") }} }}
fun make(): Rc<Leaf> {{ println("make"); return Rc(Leaf()) }}
fun suffix(): Int {{ println("suffix"); return 0 }}
fun relay(source: String): borrow String from source {{ println("relay"); return source }}
object Gate {{
    fun inspect(owner: Rc<Leaf>, observed: String, stop: Int, last: Int): Unit {{ println("dispatch") }}
}}
fun select(source: String, stop: Boolean): borrow String from source {{
    Gate.inspect(make(), source, if (stop) return {value} else 0, suffix())
    return source
}}
fun entry(): Unit {{
    val source = "held".clone()
    borrow val first = select(source, true)
    println(first)
    borrow val second = select(source, false)
    println(second)
    println(source)
}}
"#
        );
        let llvm = super::boxed_enum_tests::lower_to_llvm("singleton-entry-return.ko", &source);
        // One owned source String and two independent pairs of Leaf and Rc allocations.
        let output = super::boxed_enum_tests::run_counted_allocations(&llvm, 5);
        let expected = format!("{first}make\nsuffix\ndispatch\ndrop-leaf\nheld\nheld\n");
        super::boxed_enum_tests::assert_success(&output, expected.as_bytes());
    }
}

#[test]
fn singleton_loop_exit_retires_argument_reborrow_but_keeps_string_entry() {
    for (body, early, normal) in [
        (
            r#"loop {
        Gate.inspect(make(), source, if (stop) break else 0, suffix())
        break
    }"#,
            "make\ndrop-leaf\nheld\n",
            "make\nsuffix\ndispatch\ndrop-leaf\nheld\n",
        ),
        (
            r#"var again = true
    while (condition(again)) {
        again = false
        Gate.inspect(make(), source, if (stop) continue else 0, suffix())
    }"#,
            "condition\nmake\ndrop-leaf\ncondition\nheld\n",
            "condition\nmake\nsuffix\ndispatch\ndrop-leaf\ncondition\nheld\n",
        ),
    ] {
        let source = format!(
            r#"
class Leaf {{ deinit() {{ println("drop-leaf") }} }}
fun make(): Rc<Leaf> {{ println("make"); return Rc(Leaf()) }}
fun suffix(): Int {{ println("suffix"); return 0 }}
fun condition(again: Boolean): Boolean {{ println("condition"); return again }}
object Gate {{
    fun inspect(owner: Rc<Leaf>, observed: String, stop: Int, last: Int): Unit {{ println("dispatch") }}
}}
fun select(source: String, stop: Boolean): borrow String from source {{
    {body}
    return source
}}
fun entry(): Unit {{
    val source = "held".clone()
    borrow val first = select(source, true)
    println(first)
    borrow val second = select(source, false)
    println(second)
    println(source)
}}
"#
        );
        let llvm = super::boxed_enum_tests::lower_to_llvm("singleton-entry-loop.ko", &source);
        let output = super::boxed_enum_tests::run_counted_allocations(&llvm, 5);
        let expected = format!("{early}{normal}held\n");
        super::boxed_enum_tests::assert_success(&output, expected.as_bytes());
    }
}
