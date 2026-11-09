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
