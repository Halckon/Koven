//! SPEC-0312 unit controls: unchanged ordinary/constant implementations, real native outputs.
use super::resource_exchange_tests::run_unit;
use crate::native_tests::boxed_enum_tests::assert_success;

fn run(constants: bool, live: bool) {
    let after = if live {
        "if (keys.size == 3) { println(\"live\") }"
    } else {
        ""
    };
    let provider = format!(
        "package p\nfun main(): Unit {{ val keys = arrayOf(1, 2, 3); for (key in keys) {{ if (key == 2) {{ continue }}; println(\"tail\") }}; {after} }}"
    );
    let (output, _) = run_unit(
        &provider,
        "package q\nfun entry(): Unit { p.main() }",
        constants,
    );
    assert_success(
        &output,
        if live {
            b"tail\ntail\nlive\n"
        } else {
            b"tail\ntail\n"
        },
    );
}
#[test]
fn owned_source_continue_unit_native_basic_live() {
    run(false, true);
}
#[test]
fn owned_source_continue_unit_native_basic_dead() {
    run(false, false);
}
#[test]
fn owned_source_continue_unit_native_constant_live() {
    run(true, true);
}
#[test]
fn owned_source_continue_unit_native_constant_dead() {
    run(true, false);
}

#[test]
fn owned_source_continue_unit_native_post_loop_array_replacement() {
    let provider = r#"package p
fun main(): Unit {
    var keys = arrayOf(1, 2, 3)
    for (key in keys) { if (key == 2) { continue } }
    keys[0] = 9
    if (keys[0] == 9) { println("replaced") }
}
"#;
    for constants in [false, true] {
        let (output, _) = run_unit(
            provider,
            "package q\nfun entry(): Unit { p.main() }",
            constants,
        );
        assert_success(&output, b"replaced\n");
    }
}
