//! Independent public unit native path; no source/IR instrumentation.
use super::super::resource_exchange_tests::run_unit;
use crate::native_tests::boxed_enum_tests::assert_success;

fn run(factory: &str, ty: &str, own: bool, live: bool, constants: bool) {
    let parameter = if own {
        format!("own keys: {ty}<Int>, ")
    } else {
        String::new()
    };
    let seed = if own {
        String::new()
    } else {
        format!("val keys = {factory}(1, 2, 3);")
    };
    let after = if live {
        "if (keys.size == 3) { println(\"live\") }"
    } else {
        ""
    };
    let provider = format!(
        "package p\nfun work({parameter}stop: Int): Unit {{ {seed} for (key in keys) {{ if (key == stop) {{ return }}; println(\"tail\") }}; {after} }}"
    );
    let argument = if own {
        format!("{factory}(1, 2, 3), ")
    } else {
        String::new()
    };
    let consumer = format!(
        "package q\nfun entry(): Unit {{ p.work({argument}2); println(\"returned\"); p.work({argument}9); println(\"exhausted\") }}"
    );
    let (output, _) = run_unit(&provider, &consumer, constants);
    let expected = format!(
        "tail\nreturned\ntail\ntail\ntail\n{}exhausted\n",
        if live { "live\n" } else { "" }
    );
    assert_success(&output, expected.as_bytes());
}

#[test]
fn owned_source_return_unit_native_array_local_dead_basic() {
    run("arrayOf", "Array", false, false, false);
}

#[test]
fn owned_source_return_unit_native_array_local_dead_constant() {
    run("arrayOf", "Array", false, false, true);
}

#[test]
fn owned_source_return_unit_native_array_local_live_basic() {
    run("arrayOf", "Array", false, true, false);
}

#[test]
fn owned_source_return_unit_native_array_local_live_constant() {
    run("arrayOf", "Array", false, true, true);
}

#[test]
fn owned_source_return_unit_native_array_own_dead_basic() {
    run("arrayOf", "Array", true, false, false);
}

#[test]
fn owned_source_return_unit_native_array_own_dead_constant() {
    run("arrayOf", "Array", true, false, true);
}

#[test]
fn owned_source_return_unit_native_array_own_live_basic() {
    run("arrayOf", "Array", true, true, false);
}

#[test]
fn owned_source_return_unit_native_array_own_live_constant() {
    run("arrayOf", "Array", true, true, true);
}

#[test]
fn owned_source_return_unit_native_list_local_dead_basic() {
    run("listOf", "List", false, false, false);
}

#[test]
fn owned_source_return_unit_native_list_local_dead_constant() {
    run("listOf", "List", false, false, true);
}

#[test]
fn owned_source_return_unit_native_list_local_live_basic() {
    run("listOf", "List", false, true, false);
}

#[test]
fn owned_source_return_unit_native_list_local_live_constant() {
    run("listOf", "List", false, true, true);
}

#[test]
fn owned_source_return_unit_native_list_own_dead_basic() {
    run("listOf", "List", true, false, false);
}

#[test]
fn owned_source_return_unit_native_list_own_dead_constant() {
    run("listOf", "List", true, false, true);
}

#[test]
fn owned_source_return_unit_native_list_own_live_basic() {
    run("listOf", "List", true, true, false);
}

#[test]
fn owned_source_return_unit_native_list_own_live_constant() {
    run("listOf", "List", true, true, true);
}

#[test]
fn owned_source_return_unit_native_mutable_list_local_dead_basic() {
    run("mutableListOf", "MutableList", false, false, false);
}

#[test]
fn owned_source_return_unit_native_mutable_list_local_dead_constant() {
    run("mutableListOf", "MutableList", false, false, true);
}

#[test]
fn owned_source_return_unit_native_mutable_list_local_live_basic() {
    run("mutableListOf", "MutableList", false, true, false);
}

#[test]
fn owned_source_return_unit_native_mutable_list_local_live_constant() {
    run("mutableListOf", "MutableList", false, true, true);
}

#[test]
fn owned_source_return_unit_native_mutable_list_own_dead_basic() {
    run("mutableListOf", "MutableList", true, false, false);
}

#[test]
fn owned_source_return_unit_native_mutable_list_own_dead_constant() {
    run("mutableListOf", "MutableList", true, false, true);
}

#[test]
fn owned_source_return_unit_native_mutable_list_own_live_basic() {
    run("mutableListOf", "MutableList", true, true, false);
}

#[test]
fn owned_source_return_unit_native_mutable_list_own_live_constant() {
    run("mutableListOf", "MutableList", true, true, true);
}
