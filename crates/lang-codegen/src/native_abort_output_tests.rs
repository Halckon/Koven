//! SPEC-0355: Guide v0.46 abort output contract on real native executables.
use super::emit_link_and_run;
use std::os::unix::process::ExitStatusExt;

/// Runs `entry` and checks stdout, the exact two-line stderr report and SIGABRT.
fn assert_abort(case: &str, source: &str, stdout: &str, message: &str, at: &str) {
    let name = format!("{case}.ko");
    let run = emit_link_and_run(&name, source, "entry");
    assert_eq!(run.status.signal(), Some(6), "{case}: {run:?}");
    assert_eq!(String::from_utf8_lossy(&run.stdout), stdout, "{case}");
    let (line, column) = position(source, at);
    assert_eq!(
        String::from_utf8_lossy(&run.stderr),
        format!("abort: {message}\n  at {name}:{line}:{column}\n"),
        "{case}"
    );
}

/// 1-based line and Unicode-scalar column of the unique `at` text.
fn position(source: &str, at: &str) -> (usize, usize) {
    assert_eq!(source.matches(at).count(), 1, "unique anchor {at:?}");
    let offset = source.find(at).unwrap();
    let before = &source[..offset];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    (line, source[line_start..offset].chars().count() + 1)
}

#[test]
fn error_writes_its_message_verbatim_after_flushed_stdout() {
    assert_abort(
        "error-message",
        "fun entry(): Unit {\n  println(\"before\")\n  /* 界 */ error(\"boom 界 \\\"quoted\\\"\")\n}\n",
        "before\n",
        "boom 界 \"quoted\"",
        "error(",
    );
}

#[test]
fn named_nothing_call_enters_the_callee_and_aborts_during_an_argument_prefix() {
    let check = || {
        assert_abort(
            "named-nothing-prefix",
            r#"
class Ticket { deinit() { println("unexpected drop") } }
fun prefix(): Ticket { println("prefix"); return Ticket() }
fun stop(): Nothing { println("entered stop"); error("from stop") }
fun suffix(): Int { println("unexpected suffix"); return 1 }
fun dormant(own ticket: Ticket, middle: Int, last: Int): Unit { println("unexpected dispatch") }
fun entry(): Unit { dormant(prefix(), stop(), suffix()) }
"#,
            "prefix\nentered stop\n",
            "from stop",
            "error(",
        )
    };
    check();
    // SPEC-0397 retires the legacy function-value fallback; the refinement layer is
    // the only lowering route for an indirect `Nothing` call.
}

#[test]
fn named_nothing_method_evaluates_its_receiver_before_its_arguments() {
    assert_abort(
        "named-nothing-method",
        r#"
class Worker {
    deinit() { println("unexpected drop") }
    fun stop(own value: Int): Nothing { println("entered method"); error("method stop") }
}
fun worker(): Worker { println("receiver"); return Worker() }
fun argument(): Int { println("argument"); return 1 }
fun entry(): Unit { worker().stop(argument()); println("unexpected continuation") }
"#,
        "receiver\nargument\nentered method\n",
        "method stop",
        "error(",
    );
}

#[test]
fn indirect_nothing_function_pointer_has_no_normal_continuation() {
    let check = || {
        assert_abort(
            "indirect-nothing-pointer",
            r#"
fun entry(): Unit {
    val stop: () -> Nothing = { println("pointer"); error("pointer stop") }
    stop()
    println("unexpected continuation")
}
"#,
            "pointer\n",
            "pointer stop",
            "error(",
        );
    };
    check();
    // SPEC-0397 retires the legacy function-value fallback; the refinement layer is
    // the only lowering route for an indirect `Nothing` call.
}

#[test]
fn indirect_nothing_capturing_closure_has_no_normal_continuation() {
    assert_abort(
        "indirect-nothing-capture",
        r#"
fun entry(): Unit {
    val message = "captured stop"
    val stop: () -> Nothing = { println("capture"); error(message) }
    stop()
    println("unexpected continuation")
}
"#,
        "capture\n",
        "captured stop",
        "error(",
    );
}

#[test]
fn indirect_nothing_borrowed_callback_has_no_normal_continuation() {
    assert_abort(
        "indirect-nothing-callback",
        r#"
fun invoke(stop: () -> Nothing): Int {
    println("callback")
    stop()
}
fun entry(): Unit {
    val stop: () -> Nothing = { error("callback stop") }
    val result = invoke(stop)
    println("unexpected continuation")
}
"#,
        "callback\n",
        "callback stop",
        "error(",
    );
}

#[test]
fn indirect_nothing_while_keeps_borrowed_operands_off_the_bypass() {
    assert_indirect_nothing_while_bypass(r#"stop("inner")"#);
    assert_indirect_nothing_while_bypass(r#"sink("outer", stop("inner"))"#);
}

#[test]
fn indirect_nothing_container_prefix_keeps_size_loan_off_the_bypass() {
    assert_indirect_nothing_while_bypass(r#"val xs = List<Int>(1, stop("inner"))"#);
}

#[test]
fn indirect_nothing_map_prefix_keeps_key_and_receiver_loans_off_the_bypass() {
    assert_indirect_nothing_while_bypass(
        r#"val m = mapOf<String, String>(); m.withValue("key", stop("inner"))"#,
    );
}

fn assert_indirect_nothing_while_bypass(body: &str) {
    let source = format!(
        r#"
fun sink(outer: String, inner: Int): Unit {{ println("unexpected sink") }}
fun work(flag: Boolean): Unit {{
    val stop: (borrow String) -> Nothing = {{ text -> error(text) }}
    while (flag) {{ {body} }}
    return
}}
fun entry(): Unit {{ work(false); println("done") }}
"#
    );
    super::boxed_enum_tests::assert_success(
        &emit_link_and_run("indirect-nothing-while-bypass.ko", &source, "entry"),
        b"done\n",
    );
}

#[test]
fn null_assertion_points_at_the_asserted_expression() {
    assert_abort(
        "null-assertion",
        "class Node(val item: Int)\nfun take(own node: Node): Unit {}\nfun entry(): Unit {\n  val node: Node? = null\n  take(node!!)\n}\n",
        "",
        "null assertion failed",
        "node!!",
    );
}

#[test]
fn checked_arithmetic_names_the_failed_operator() {
    let prelude = "fun one(): Int = 1\nfun zero(): Int = 0\nfun minusOne(): Int = -1\n";
    for (case, expression, message) in [
        ("add", "2147483647 + one()", "integer overflow in +"),
        (
            "subtract",
            "-2147483647 - one() - one()",
            "integer overflow in -",
        ),
        (
            "multiply",
            "1073741824 * (one() + one())",
            "integer overflow in *",
        ),
        ("divide-zero", "10 / zero()", "division by zero"),
        ("remainder-zero", "10 % zero()", "division by zero"),
        (
            "divide-overflow",
            "(-2147483647 - one()) / minusOne()",
            "integer overflow in /",
        ),
    ] {
        let source = format!(
            "{prelude}fun entry(): Unit {{\n  val result = {expression}\n  println(\"unreachable\")\n}}\n"
        );
        let operator_line = source
            .lines()
            .position(|line| line.contains("val result"))
            .unwrap();
        let run = emit_link_and_run(&format!("{case}.ko"), &source, "entry");
        assert_eq!(run.status.signal(), Some(6), "{case}: {run:?}");
        assert!(run.stdout.is_empty(), "{case}");
        let stderr = String::from_utf8_lossy(&run.stderr).into_owned();
        let expected_prefix = format!("abort: {message}\n  at {case}.ko:{}:", operator_line + 1);
        assert!(stderr.starts_with(&expected_prefix), "{case}: {stderr:?}");
        assert!(
            stderr.ends_with('\n') && stderr.lines().count() == 2,
            "{case}: {stderr:?}"
        );
    }
}

#[test]
fn checked_conversion_names_the_target_type() {
    assert_abort(
        "conversion",
        "fun wide(): Int = 300\nfun entry(): Unit {\n  val narrow = wide().toByte()\n}\n",
        "",
        "value out of range for Byte",
        "wide().toByte()",
    );
}

#[test]
fn list_index_reports_the_index_and_size() {
    for (case, index, text) in [
        ("index-beyond", "5", "index 5 out of bounds for size 3"),
        ("index-negative", "-1", "index -1 out of bounds for size 3"),
    ] {
        let source = format!(
            "fun pick(): Int = {index}\nfun entry(): Unit {{\n  val items = listOf(1, 2, 3)\n  val item = items[pick()]\n}}\n"
        );
        assert_abort(case, &source, "", text, "items[pick()]");
    }
}

#[test]
fn collection_and_map_aborts_use_the_guide_text() {
    assert_abort(
        "list-empty",
        "fun entry(): Unit {\n  var items = mutableListOf<Int>()\n  val last = items.removeLast()\n}\n",
        "",
        "list is empty",
        "items.removeLast()",
    );
    assert_abort(
        "map-assert",
        "fun entry(): Unit {\n  val ages = mutableMapOf<Int, Int>()\n  val age = ages.get(3)!!\n}\n",
        "",
        "null assertion failed",
        "ages.get(3)!!",
    );
}
