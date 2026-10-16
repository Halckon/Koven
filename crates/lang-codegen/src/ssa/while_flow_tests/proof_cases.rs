//! Ordinary nullable-proof acceptance candidates. Initial negative controls pin stage;
//! their complete diagnostic identities are tightened after first real observation.
use super::super::{integer_range_value_tests::collect_sources, verify::verify_program};

fn observe(group: &str, cases: &[(String, bool)]) {
    let sources = cases
        .iter()
        .map(|(source, _)| source.clone())
        .collect::<Vec<_>>();
    let observations = collect_sources(&sources);
    assert_eq!(observations.len(), cases.len() * 2);
    let outcomes = observations
        .into_iter()
        .enumerate()
        .map(|(index, result)| {
            let result = result.and_then(|program| {
                verify_program(&program).map_err(|error| format!("SSA verify: {error:?}"))?;
                let llvm = crate::llvm::render_verified_program(&program)
                    .map_err(|error| format!("LLVM: {error:?}"))?;
                println!("PROOF LLVM group={group} case={index} {llvm}");
                Ok(())
            });
            println!(
                "PROOF OUTCOME group={group} source={} unit={} {result:?}",
                index / 2,
                index % 2 == 1
            );
            result
        })
        .collect::<Vec<_>>();
    for (index, result) in outcomes.into_iter().enumerate() {
        if cases[index / 2].1 {
            let error = result.expect_err("stale read or conflicting child must be rejected");
            assert!(
                error.starts_with("complete:") || error.starts_with("ownership"),
                "{error}"
            );
        } else {
            result.expect("ordinary nullable while proof reaches verified SSA and LLVM");
        }
    }
}

fn holder(body: &str) -> String {
    format!(
        "class Holder(val value: Int)\nfun entry(own initial: Holder?, flag: Boolean): Int {{\nvar current: Holder? = initial\nvar total: Int = 0\n{body}\n}}"
    )
}

#[test]
fn normal_continue_and_group_conditions_reach_llvm() {
    let normal =
        holder("while (current != null) { total = current.value\ncurrent = null }\nreturn total");
    let continued = normal.replace("current = null }", "current = null\ncontinue }");
    let grouped = continued.replace("while (current != null)", "while (((current) != null))");
    observe(
        "normal-continue",
        &[(normal, false), (continued, false), (grouped, false)],
    );
}

#[test]
fn break_return_and_outer_owner_reach_llvm() {
    let broken = r#"class Holder(val value: Int)
class Guard(val text: String) { deinit() { println(this.text) } }
fun entry(own initial: Holder?): Int {
    val outer = Guard("outer")
    var current: Holder? = initial
    var total: Int = 0
    while (current != null) {
        val inner = Guard("inner")
        total = current.value
        break
    }
    println(outer.text)
    return total
}"#
    .to_owned();
    let returned = r#"class Holder(val text: String)
fun inspect(text: String): Int { println(text); return 7 }
fun entry(own initial: Holder?): Int {
    var current: Holder? = initial
    while (current != null) { return inspect(current.text) }
    return 0
}"#
    .to_owned();
    observe("break-return", &[(broken, false), (returned, false)]);
}

#[test]
fn branch_join_and_replacement_keep_current_proof_identity() {
    let read = holder(
        "while (current != null) {\nif (flag) { total = current.value } else { total = current.value }\ntotal = total + current.value\ncurrent = null\n}\nreturn total",
    );
    let retested = holder(
        "while (current != null) {\nif (flag) { current = null }\nif (current != null) { total = current.value }\nbreak\n}\nreturn total",
    );
    let stale = holder(
        "while (current != null) {\nif (flag) { current = null }\ntotal = current.value\nbreak\n}\nreturn total",
    );
    observe(
        "branch-join",
        &[(read, false), (retested, false), (stale, true)],
    );
}

#[test]
fn same_root_nested_loops_preserve_or_invalidate_outer_proof() {
    let readonly = holder(
        "while (current != null) {\ntotal = current.value\nwhile (current != null) { total = total + current.value\nbreak }\ntotal = total + current.value\nbreak\n}\nreturn total",
    );
    let written = holder(
        "while (current != null) {\ntotal = current.value\nwhile (current != null) { total = total + current.value\ncurrent = null\ncontinue }\nreturn total\n}\nreturn total",
    );
    let normal = written.replace("\ncontinue }", "\n}");
    let stale = written.replace("\nreturn total\n}", "\nreturn current.value\n}");
    observe(
        "same-root",
        &[
            (readonly, false),
            (written, false),
            (normal, false),
            (stale, true),
        ],
    );
}

#[test]
fn different_root_and_borrowed_loops_preserve_outer_access() {
    let different = r#"class Holder(val value: Int)
fun entry(own left: Holder?, own right: Holder?): Int {
    var current: Holder? = left
    var other: Holder? = right
    var total: Int = 0
    while (current != null) {
        while (other != null) { total = total + other.value; other = null; continue }
        return total + current.value
    }
    return total
}"#
    .to_owned();
    let numeric = holder(
        "while (current != null) {\nfor (item in 0..<2) { if (item == 0) { continue }\ntotal = total + current.value }\nreturn total + current.value\n}\nreturn total",
    );
    let inner_borrow = holder(
        "val items = listOf(1, 2)\nwhile (current != null) {\nfor (item in items) { if (item == 1) { continue }\ntotal = total + item + current.value }\nreturn total + current.value\n}\nreturn total",
    );
    let outer_borrow = holder(
        "val items = listOf(1, 2)\nfor (item in items) {\nwhile (current != null) { total = total + item + current.value\nbreak }\ntotal = total + item\n}\nreturn total",
    );
    observe(
        "other-loops",
        &[
            (different, false),
            (numeric, false),
            (inner_borrow, false),
            (outer_borrow, false),
        ],
    );
}

#[test]
fn pending_field_child_survives_readonly_nested_control() {
    let read = r#"class Holder(val text: String, val value: Int)
fun inspect(text: String, own marker: Int): Int { println(text); return marker }
fun entry(own initial: Holder?, flag: Boolean): Int {
    var current: Holder? = initial
    while (current != null) {
        val result = inspect(current.text, if (flag) {
            while (current != null) { break }
            current.value
        } else { current.value })
        return result + current.value
    }
    return 0
}"#
    .to_owned();
    let conflict = r#"class Holder(val text: String, val value: Int)
fun inspect(text: String, own marker: Int): Int { println(text); return marker }
fun entry(own initial: Holder?, flag: Boolean): Int {
    var current: Holder? = initial
    while (current != null) {
        val result = inspect(current.text, if (flag) {
            while (current != null) { current = null; break }
            0
        } else { 0 })
        return result
    }
    return 0
}"#
    .to_owned();
    observe("pending-child", &[(read, false), (conflict, true)]);
}

#[test]
fn owner_transfer_and_other_owner_assertion_preserve_cleanup() {
    let transfer = r#"class Holder(val value: Int)
fun entry(own initial: Holder?): Holder? {
    var current: Holder? = initial
    while (current != null) { return current }
    return null
}"#
    .to_owned();
    let assertion = r#"class Holder(val value: Int)
fun entry(own initial: Holder?, own other: Holder?): Int {
    var current: Holder? = initial
    while (current != null) {
        val taken = other!!
        return taken.value + current.value
    }
    return 0
}"#
    .to_owned();
    observe("owner-transfer", &[(transfer, false), (assertion, false)]);
}
