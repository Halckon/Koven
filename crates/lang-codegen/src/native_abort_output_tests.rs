//! SPEC-0353: Guide v0.46 abort output contract on real native executables.
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
