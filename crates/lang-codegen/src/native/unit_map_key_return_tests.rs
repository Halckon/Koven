//! SPEC-0297: ordinary-source key returns through both public native entries.
//!
//! These tests observe source results, continuation and Resource deinit ordering.
//! They do not instrument allocation or claim complete memory-lifetime evidence.

use super::resource_exchange_tests::run_unit;
use crate::native_tests::emit_link_and_run;

const RESOURCE: &str = r#"
class Resource(val n: Int) {
    deinit() {
        if (n == 7) { println("value-drop") } else { println("prefix-drop") }
    }
}
"#;

#[derive(Clone, Copy)]
enum Entry {
    Single,
    Unit,
}

fn assert_source(entry: Entry, name: &str, source: &str, expected: &str) {
    let run = match entry {
        Entry::Single => emit_link_and_run(name, source, "exercise"),
        Entry::Unit => {
            let provider = format!("package p\n{source}");
            let consumer = "package q\nfun entry(): Unit { p.exercise() }";
            run_unit(&provider, consumer, false).0
        }
    };
    assert!(run.status.success(), "{name}: {run:?}");
    assert_eq!(run.stdout, expected.as_bytes(), "{name}");
    assert!(run.stderr.is_empty(), "{name}: {run:?}");
}

#[test]
fn single_map_key_return_scalar_shapes_preserve_normal_results() {
    scalar_shapes(Entry::Single);
}

#[test]
fn unit_map_key_return_scalar_shapes_preserve_normal_results() {
    scalar_shapes(Entry::Unit);
}

fn scalar_shapes(entry: Entry) {
    for operation in ["get", "contains", "remove"] {
        for (shape, key, expected) in [
            (
                "mixed",
                "if (flag) { println(\"left\"); return } else { println(\"right\"); 1 }",
                "right\nnormal\nprefix-drop\nleft\nprefix-drop\ndone\n",
            ),
            (
                "reversed",
                "if (flag) { println(\"left\"); 1 } else { println(\"right\"); return }",
                "right\nprefix-drop\nleft\nnormal\nprefix-drop\ndone\n",
            ),
            (
                "both",
                "if (flag) { println(\"left\"); return } else { println(\"right\"); return }",
                "right\nprefix-drop\nleft\nprefix-drop\ndone\n",
            ),
            ("direct", "return", "prefix-drop\nprefix-drop\ndone\n"),
        ] {
            let result = match operation {
                "contains" => "if (!result) { error(\"missing contains result\") }",
                _ => "if (result != 7) { error(\"wrong nullable result\") }",
            };
            let remaining = if operation == "remove" {
                "if (m.size != 0) { error(\"remove did not commit\") }"
            } else {
                "if (m.size != 1) { error(\"query changed receiver\") }\n    if (m.get(1) != 7) { error(\"query lost value\") }"
            };
            let source = format!(
                r#"{RESOURCE}
fun inspect(own flag: Boolean): Unit {{
    val prefix = Resource(0)
    var m = mutableMapOf<Int, Int>()
    m.put(1, 7)
    val result = m.{operation}({key})
    {result}
    {remaining}
    println("normal")
}}
fun exercise(): Unit {{ inspect(false); inspect(true); println("done") }}
"#
            );
            assert_source(
                entry,
                &format!("map-key-{operation}-{shape}"),
                &source,
                expected,
            );
        }
    }
}

#[test]
fn single_map_key_return_string_owners_and_resources_follow_selected_edge() {
    string_owners(Entry::Single);
}

#[test]
fn unit_map_key_return_string_owners_and_resources_follow_selected_edge() {
    string_owners(Entry::Unit);
}

fn string_owners(entry: Entry) {
    for operation in ["get", "contains", "remove"] {
        for (form, key) in [
            ("named", "key"),
            ("grouped", "(key)"),
            ("temporary", "\"key\".clone()"),
        ] {
            let (value_type, value, check, expected) = match operation {
                "get" => (
                    "Int",
                    "7",
                    "if (result != 7) { error(\"wrong get result\") }\n    if (m.size != 1) { error(\"get changed receiver\") }",
                    "key-evaluated\nnormal\nprefix-drop\nreturning\nprefix-drop\ndone\n",
                ),
                "contains" => (
                    "Resource",
                    "Resource(7)",
                    "if (!result) { error(\"missing contains result\") }\n    if (m.size != 1) { error(\"contains changed receiver\") }",
                    "key-evaluated\nnormal\nvalue-drop\nprefix-drop\nreturning\nvalue-drop\nprefix-drop\ndone\n",
                ),
                "remove" => (
                    "Resource",
                    "Resource(7)",
                    "val taken = result!!\n    if (taken.n != 7) { error(\"wrong removed resource\") }\n    if (m.size != 0) { error(\"resource still in receiver\") }",
                    "key-evaluated\nnormal\nvalue-drop\nprefix-drop\nreturning\nvalue-drop\nprefix-drop\ndone\n",
                ),
                _ => unreachable!(),
            };
            // A control tail delivers its String owner before the outer Borrow.
            // Named/grouped keys are intentionally not reused after this move.
            let source = format!(
                r#"{RESOURCE}
fun inspect(own flag: Boolean): Unit {{
    val prefix = Resource(0)
    var m = mutableMapOf<String, {value_type}>()
    m.put("key".clone(), {value})
    val key = "key".clone()
    val result = m.{operation}(if (flag) {{ println("returning"); return }} else {{ println("key-evaluated"); {key} }})
    {check}
    println("normal")
}}
fun exercise(): Unit {{ inspect(false); inspect(true); println("done") }}
"#
            );
            assert_source(
                entry,
                &format!("map-key-{operation}-{form}"),
                &source,
                expected,
            );
        }
    }
}

#[test]
fn single_map_key_return_without_suffix_cleans_existing_receiver() {
    without_suffix(Entry::Single);
}

#[test]
fn unit_map_key_return_without_suffix_cleans_existing_receiver() {
    without_suffix(Entry::Unit);
}

fn without_suffix(entry: Entry) {
    for operation in ["get", "contains", "remove"] {
        let (value_type, value, expected) = if operation == "get" {
            ("Int", "7", "prefix-drop\nprefix-drop\ndone\n")
        } else {
            (
                "Resource",
                "Resource(7)",
                "value-drop\nprefix-drop\nvalue-drop\nprefix-drop\ndone\n",
            )
        };
        let source = format!(
            r#"{RESOURCE}
fun direct(): Unit {{
    val prefix = Resource(0)
    var m = mutableMapOf<String, {value_type}>()
    m.put("key".clone(), {value})
    m.{operation}(return)
}}
fun both(own flag: Boolean): Unit {{
    val prefix = Resource(0)
    var m = mutableMapOf<String, {value_type}>()
    m.put("key".clone(), {value})
    m.{operation}(if (flag) {{ return }} else {{ return }})
}}
fun exercise(): Unit {{ direct(); both(false); println("done") }}
"#
        );
        assert_source(
            entry,
            &format!("map-key-{operation}-no-suffix"),
            &source,
            expected,
        );
    }
}

#[test]
fn single_map_key_return_borrowed_receivers_remain_owned_by_caller() {
    borrowed_receivers(Entry::Single);
}

#[test]
fn unit_map_key_return_borrowed_receivers_remain_owned_by_caller() {
    borrowed_receivers(Entry::Unit);
}

fn borrowed_receivers(entry: Entry) {
    // remove requires Inout; a Borrow receiver is only legal for these queries.
    for operation in ["get", "contains"] {
        let check = if operation == "get" {
            "if (result != 7) { error(\"wrong borrowed get\") }"
        } else {
            "if (!result) { error(\"wrong borrowed contains\") }"
        };
        let source = format!(
            r#"{RESOURCE}
fun inspect(m: MutableMap<String, Int>, own flag: Boolean): Unit {{
    val prefix = Resource(0)
    val result = m.{operation}(if (flag) {{ println("returning"); return }} else {{ "key".clone() }})
    {check}
    println("normal")
}}
fun exercise(): Unit {{
    var m = mutableMapOf<String, Int>()
    m.put("key".clone(), 7)
    inspect(m, true)
    if (m.get("key") != 7) {{ error("return changed caller map") }}
    inspect(m, false)
    m.put("key".clone(), 9)
    if (m.remove("key") != 9) {{ error("caller cannot mutate after query") }}
    println("caller-kept")
}}
"#
        );
        assert_source(
            entry,
            &format!("map-key-{operation}-borrow-receiver"),
            &source,
            "returning\nprefix-drop\nnormal\nprefix-drop\ncaller-kept\n",
        );
    }
}

#[test]
fn single_map_key_return_normal_borrow_controls_preserve_named_keys() {
    normal_borrow_controls(Entry::Single);
}

#[test]
fn unit_map_key_return_normal_borrow_controls_preserve_named_keys() {
    normal_borrow_controls(Entry::Unit);
}

fn normal_borrow_controls(entry: Entry) {
    let source = r#"
fun exercise(): Unit {
    var m = mutableMapOf<String, Int>()
    m.put("key".clone(), 0)
    val key = "key".clone()
    if (m.get(key) != 0) { error("get lost present zero") }
    if (!m.contains((key))) { error("contains lost entry") }
    if (m.remove(key) != 0) { error("remove lost present zero") }
    if (m.get((key)) != null) { error("missing get must be null") }
    if (m.contains(key)) { error("removed entry still present") }
    if (m.remove((key)) != null) { error("missing remove must be null") }
    if (m.get("missing".clone()) != null) { error("temporary missing get") }
    if (m.contains("missing".clone())) { error("temporary missing contains") }
    if (m.remove("missing".clone()) != null) { error("temporary missing remove") }
    println(key)
    println("done")
}
"#;
    assert_source(
        entry,
        "map-key-normal-borrow-controls",
        source,
        "key\ndone\n",
    );
}

#[test]
fn single_map_key_return_no_map_control_preserves_resource_cleanup() {
    no_map_control(Entry::Single);
}

#[test]
fn unit_map_key_return_no_map_control_preserves_resource_cleanup() {
    no_map_control(Entry::Unit);
}

fn no_map_control(entry: Entry) {
    let source = format!(
        r#"{RESOURCE}
fun inspect(own flag: Boolean): Unit {{
    val prefix = Resource(0)
    val value = if (flag) {{ println("returning"); return }} else {{ 7 }}
    if (value != 7) {{ error("wrong ordinary result") }}
    println("normal")
}}
fun exercise(): Unit {{ inspect(false); inspect(true); println("done") }}
"#
    );
    assert_source(
        entry,
        "map-key-no-map-control",
        &source,
        "normal\nprefix-drop\nreturning\nprefix-drop\ndone\n",
    );
}

#[test]
fn single_map_key_return_temporary_receivers_evaluate_once_and_cleanup() {
    temporary_receivers(Entry::Single, false);
}

#[test]
fn unit_map_key_return_temporary_receivers_evaluate_once_and_cleanup() {
    temporary_receivers(Entry::Unit, false);
}

fn temporary_receivers(entry: Entry, grouped: bool) {
    let receiver = if grouped { "(make())" } else { "make()" };
    for operation in ["get", "contains"] {
        for readonly in [true, false] {
            let (receiver_type, make_body, check, receiver_drop) = match (operation, readonly) {
                ("get", true) => (
                    "Map<String, Int>",
                    "return mapOf<String, Int>()",
                    "if (result != null) { error(\"readonly get must be missing\") }",
                    "",
                ),
                ("contains", true) => (
                    "Map<String, Resource>",
                    "return mapOf<String, Resource>()",
                    "if (result) { error(\"readonly contains must be false\") }",
                    "",
                ),
                ("get", false) => (
                    "MutableMap<String, Int>",
                    "var result = mutableMapOf<String, Int>()\n    result.put(\"key\".clone(), 7)\n    return result",
                    "if (result != 7) { error(\"owned get lost present value\") }",
                    "",
                ),
                ("contains", false) => (
                    "MutableMap<String, Resource>",
                    "var result = mutableMapOf<String, Resource>()\n    result.put(\"key\".clone(), Resource(7))\n    return result",
                    "if (!result) { error(\"owned contains lost entry\") }",
                    "value-drop\n",
                ),
                _ => unreachable!(),
            };
            let source = format!(
                r#"{RESOURCE}
fun make(): {receiver_type} {{
    println("receiver-evaluated")
    {make_body}
}}
fun inspect(own flag: Boolean): Unit {{
    val prefix = Resource(0)
    val result = {receiver}.{operation}(if (flag) {{ println("returning"); return }} else {{ println("key-evaluated"); "key".clone() }})
    {check}
    println("normal")
}}
fun exercise(): Unit {{ inspect(false); inspect(true); println("done") }}
"#
            );
            let expected = format!(
                "receiver-evaluated\nkey-evaluated\n{receiver_drop}normal\nprefix-drop\nreceiver-evaluated\nreturning\n{receiver_drop}prefix-drop\ndone\n"
            );
            assert_source(
                entry,
                &format!(
                    "map-key-{operation}-temporary-receiver-readonly-{readonly}-grouped-{grouped}"
                ),
                &source,
                &expected,
            );
        }
    }
}

#[test]
fn single_map_key_return_grouped_temporary_and_control_keys_keep_cleanup() {
    grouped_key_expressions(Entry::Single);
}

#[test]
fn unit_map_key_return_grouped_temporary_and_control_keys_keep_cleanup() {
    grouped_key_expressions(Entry::Unit);
}

fn grouped_key_expressions(entry: Entry) {
    for operation in ["get", "contains", "remove"] {
        let (value_type, value, check, value_drop) = match operation {
            "get" => (
                "Int",
                "7",
                "if (result != 7) { error(\"wrong grouped get\") }\n    if (m.size != 1) { error(\"grouped get changed receiver\") }",
                "",
            ),
            "contains" => (
                "Resource",
                "Resource(7)",
                "if (!result) { error(\"wrong grouped contains\") }\n    if (m.size != 1) { error(\"grouped contains changed receiver\") }",
                "value-drop\n",
            ),
            "remove" => (
                "Resource",
                "Resource(7)",
                "val taken = result!!\n    if (taken.n != 7) { error(\"wrong grouped removed resource\") }\n    if (m.size != 0) { error(\"grouped remove did not commit\") }",
                "value-drop\n",
            ),
            _ => unreachable!(),
        };
        for (form, key, calls, expected) in [
            (
                "temporary",
                "(\"key\".clone())",
                "inspect(false)",
                format!("normal\n{value_drop}prefix-drop\ndone\n"),
            ),
            (
                "control",
                "(if (flag) { println(\"returning\"); return } else { println(\"key-evaluated\"); \"key\".clone() })",
                "inspect(false); inspect(true)",
                format!(
                    "key-evaluated\nnormal\n{value_drop}prefix-drop\nreturning\n{value_drop}prefix-drop\ndone\n"
                ),
            ),
        ] {
            let source = format!(
                r#"{RESOURCE}
fun inspect(own flag: Boolean): Unit {{
    val prefix = Resource(0)
    var m = mutableMapOf<String, {value_type}>()
    m.put("key".clone(), {value})
    val result = m.{operation}({key})
    {check}
    println("normal")
}}
fun exercise(): Unit {{ {calls}; println("done") }}
"#
            );
            assert_source(
                entry,
                &format!("map-key-{operation}-grouped-{form}"),
                &source,
                &expected,
            );
        }
    }
}

#[test]
fn single_map_key_return_grouped_temporary_receivers_keep_outer_owner() {
    temporary_receivers(Entry::Single, true);
}

#[test]
fn unit_map_key_return_grouped_temporary_receivers_keep_outer_owner() {
    temporary_receivers(Entry::Unit, true);
}
