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
