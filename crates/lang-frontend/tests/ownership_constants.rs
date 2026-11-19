//! SPEC-0208：常量读取必须复用普通值的所有权路径。

use lang_frontend::{
    name_resolution::resolve_names,
    ownership_checking::{OwnershipCheckedFile, check_ownership},
    source::SourceMap,
    type_checking::{TypedFile, check_types, standard_environments},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

#[path = "ownership_constants/unit_cleanup_order.rs"]
mod unit_cleanup_order;
#[path = "ownership_constants/unit_failure.rs"]
mod unit_failure;
#[path = "ownership_constants/unit_materialization.rs"]
mod unit_materialization;
#[path = "ownership_constants/unit_runtime_identity.rs"]
mod unit_runtime_identity;
#[path = "ownership_constants/unit_transfer_cleanup.rs"]
mod unit_transfer_cleanup;

fn analyzed(text: &str) -> (SourceMap, TypedFile, OwnershipCheckedFile) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("constants.ko", text).expect("source");
    let parsed = parser_test_assertions::parse_file_twice(&sources, source, "constants");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (names, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &names).expect("names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.constants().is_some());
    let ownership = check_ownership(&sources, &parsed, &names, &typed).expect("ownership");
    (sources, typed, ownership)
}

fn checked(text: &str) -> (SourceMap, OwnershipCheckedFile) {
    let (sources, typed, ownership) = analyzed(text);
    assert!(
        ownership.diagnostics().is_empty(),
        "{:?}",
        ownership.diagnostics()
    );
    assert!(
        ownership.deferred().is_empty(),
        "{:?}",
        ownership.deferred()
    );
    assert!(
        ownership
            .constant_materializations()
            .expect("validated materializations")
            .matches(&typed)
    );
    (sources, ownership)
}

#[test]
fn repeated_string_borrows_own_distinct_temporaries_without_declaration_loans() {
    let text = r#"
object Labels { const val TEXT = "hello" }
const val COPY = Labels.TEXT
fun view(text: String): Unit {}
fun use(): Unit {
    val first = view(COPY)
    val second = view(COPY)
    val third = view(Labels.TEXT)
}
"#;
    unit_runtime_identity::repeated(text);
}

#[test]
fn string_binary_operands_drop_materializations_in_reverse_order() {
    let text = r#"
const val TEXT = "hello"
fun use(): Boolean = (TEXT + TEXT) == TEXT
"#;
    unit_cleanup_order::binary(text);
}

#[test]
fn scalar_delivery_and_string_return_do_not_own_constant_declarations() {
    let text = r#"
const val NUMBER = 7
const val LETTER = '文'
const val TEXT = "hello"
fun take(own number: Int, own letter: Char): Unit {}
fun use(): Unit {
    val first = take(NUMBER, LETTER)
    val second = take(NUMBER, LETTER)
}
fun first(): String = TEXT
fun second(): String = TEXT
"#;
    unit_runtime_identity::delivery(text);
}

#[test]
fn closure_reads_do_not_capture_constant_or_namespace_identity() {
    let text = r#"
object Labels { const val TEXT = "hello" }
class Names { companion { const val TEXT = Labels.TEXT } }
fun use(): Unit {
    val shared: () -> String = { Labels.TEXT }
    val owned: move () -> String = move { Names.TEXT }
}
"#;
    unit_runtime_identity::closure(text);
}

#[test]
fn materialization_plans_exclude_dependencies_and_bind_exact_analysis() {
    let source = r#"
const val TEXT = "hi"
const val COPY = TEXT
const val NUMBER = 7
fun stop(): Nothing = stop()
fun forever(): Unit {
    loop { continue }
    val unreachable = view(TEXT)
}
fun aborted(): Unit {
    val stopped = stop()
    val unreachable = view(TEXT)
}
fun view(text: String): Unit {}
fun use(): Int {
    val first = view(COPY)
    val second = view(TEXT)
    return NUMBER
    val unreachable = view(TEXT)
}
"#;
    unit_materialization::identity(source);
}

#[test]
fn ownership_failure_clears_materialization_capability() {
    let text = r#"
const val TEXT = "hi"
fun take(own text: String): Unit {}
fun use(own input: String): Unit {
    val first = take(TEXT)
    val second = take(input)
    val third = take(input)
}
"#;
    let (sources, _, owned) = analyzed(text);
    assert!(!owned.diagnostics().is_empty());
    assert!(owned.constant_materializations().is_none());
    assert!(owned.drops().is_empty());
    assert!(owned.loans().is_empty());
    unit_failure::check(&sources, owned.diagnostics());
}

#[test]
fn constant_cleanup_matches_literal_on_control_flow_edges() {
    for body in [
        "val result = view(VALUE, if (flag) { return } else { 0 })",
        "val result = view(VALUE, if (flag) { stop() } else { 0 })",
        "loop { val result = view(VALUE, if (flag) { break } else { 0 })\nbreak }",
        "loop { val result = view(VALUE, if (flag) { continue } else { 0 })\nbreak }",
        "val result = view(if (flag) { VALUE } else { VALUE }, 0)",
        "val result = take(VALUE, if (flag) { return } else { 0 })",
        "val result = take(VALUE, if (flag) { stop() } else { 0 })",
    ] {
        let source = format!(
            "const val TEXT = \"hi\"\nfun stop(): Nothing = stop()\nfun view(text: String, number: Int): Unit {{}}\nfun take(own text: String, number: Int): Unit {{}}\nfun use(flag: Boolean): Unit {{ {body} }}"
        );
        let (constant_sources, constant) = checked(&source.replace("VALUE", "TEXT"));
        let (literal_sources, literal) = checked(&source.replace("VALUE", "\"hi\""));
        // Compare actual cleanup boundaries and owner origins, without comparing arena IDs.
        let drops = |sources: &SourceMap, owned: &OwnershipCheckedFile| {
            owned
                .drops()
                .iter()
                .map(|drop| {
                    (
                        std::mem::discriminant(&drop.point()),
                        std::mem::discriminant(&drop.target()),
                        sources
                            .slice(drop.value_origin())
                            .unwrap()
                            .replace("TEXT", "\"hi\""),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            drops(&constant_sources, &constant),
            drops(&literal_sources, &literal),
            "{body}"
        );
        assert_eq!(constant.loans().len(), literal.loans().len(), "{body}");
        assert_eq!(
            constant
                .loan_ends()
                .iter()
                .map(|end| std::mem::discriminant(&end.point()))
                .collect::<Vec<_>>(),
            literal
                .loan_ends()
                .iter()
                .map(|end| std::mem::discriminant(&end.point()))
                .collect::<Vec<_>>(),
            "{body}"
        );
        assert!(
            !constant
                .constant_materializations()
                .unwrap()
                .plans()
                .is_empty(),
            "{body}"
        );
        assert!(
            literal
                .constant_materializations()
                .unwrap()
                .plans()
                .is_empty()
        );
    }
}

#[test]
fn every_closed_scalar_type_repeats_value_delivery_without_runtime_owners() {
    for (ty, literal) in [
        ("Boolean", "true"),
        ("Byte", "1"),
        ("Short", "1"),
        ("Int", "1"),
        ("Long", "1L"),
        ("UByte", "1u"),
        ("UShort", "1u"),
        ("UInt", "1u"),
        ("ULong", "1uL"),
        ("Char", "'文'"),
    ] {
        let text = format!(
            "const val VALUE: {ty} = {literal}\nfun take(own input: {ty}): Unit {{}}\nfun use(): Unit {{\nval first = take(VALUE)\nval second = take((VALUE))\n}}"
        );
        unit_materialization::scalar(&text);
    }
}

#[test]
fn deferred_ownership_does_not_publish_constant_capability() {
    let (_, _, owned) = analyzed(
        r#"
const val TEXT = "hi"
class Holder(val text: String)
fun use(): String = Holder(TEXT).text
"#,
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(!owned.deferred().is_empty());
    assert!(owned.constant_materializations().is_none());
}

#[test]
fn aborting_binary_left_has_no_right_or_following_materialization_cleanup() {
    let (_, owned) = checked(
        r#"
const val TEXT = "hi"
fun stop(): Nothing = stop()
fun view(text: String): Unit {}
fun use(): Unit {
    val stopped = stop() == TEXT
    val unreachable = TEXT
}
"#,
    );
    assert!(
        owned
            .constant_materializations()
            .unwrap()
            .plans()
            .is_empty()
    );
    assert!(owned.loans().is_empty());
    assert!(owned.loan_ends().is_empty());
    assert!(owned.drops().is_empty(), "{:?}", owned.drops());
}

#[test]
fn grouped_string_reads_keep_the_materialization_owner_identity() {
    let text = r#"
const val TEXT = "hi"
fun view(text: String): Unit {}
fun take(own text: String, own number: Int): Unit {}
fun use(flag: Boolean): Unit {
    val first = view((TEXT))
    val second = take((TEXT), if (flag) { return } else { 0 })
}
fun discard(): Unit { (TEXT) }
"#;
    unit_materialization::grouped(text);
}

#[test]
fn aborting_string_operand_does_not_drop_unevaluated_constants() {
    let text = r#"
const val TEXT = "hi"
fun stop(): Nothing = stop()
fun use(flag: Boolean): Unit {
    val stopped = "${stop()}" + TEXT
    val unreachable = TEXT
}
"#;
    for rooted in [false, true] {
        let (_, _, unit) = unit_materialization::checked(text, rooted);
        assert!(unit.materializations().is_empty());
        assert!(
            unit.ownership().drops().is_empty(),
            "{:?}",
            unit.ownership().drops()
        );
    }
}

#[test]
fn string_left_temporary_cleanup_follows_right_control_transfer() {
    for (body, transfers, normal) in [
        (
            "val result = LEFT + \"${if (flag) { return } else { 0 }}\"",
            1,
            1,
        ),
        (
            "loop { val result = LEFT + \"${if (flag) { break } else { 0 }}\"\nbreak }",
            1,
            1,
        ),
        (
            "loop { val result = LEFT + \"${if (flag) { continue } else { 0 }}\"\nbreak }",
            1,
            1,
        ),
        (
            "val result = LEFT + \"${if (flag) { stop() } else { 0 }}\"",
            0,
            1,
        ),
        ("val result = LEFT + \"${stop()}\"", 0, 0),
    ] {
        for left in ["TEXT", "\"hi\""] {
            let source = format!(
                "const val TEXT = \"hi\"\nfun stop(): Nothing = stop()\nfun use(flag: Boolean): Unit {{ {} }}",
                body.replace("LEFT", left)
            );
            unit_transfer_cleanup::binary(&source, left, transfers, normal);
        }
    }
}

#[test]
fn nested_string_left_temporaries_cleanup_in_reverse_evaluation_order() {
    let text = r#"
const val TEXT = "hi"
fun use(flag: Boolean): Unit {
    val result = TEXT + (TEXT + "${if (flag) { return } else { 0 }}")
}
"#;
    unit_cleanup_order::nested(text);
}

#[test]
fn interpolated_string_constants_release_inner_owners_before_returning_outer_string() {
    let text = r#"
const val TEXT = "hi"
fun use(): String = "${TEXT}-${(TEXT)}"
"#;
    unit_cleanup_order::interpolation(text);
}

#[test]
fn interpolation_prefix_temporaries_follow_control_transfer_and_abort() {
    for (body, transfers, normal) in [
        (
            "val result = \"${TEXT}-${if (flag) { return } else { 0 }}\"",
            1,
            1,
        ),
        (
            "loop { val result = \"${TEXT}-${if (flag) { break } else { 0 }}\"\nbreak }",
            1,
            1,
        ),
        (
            "loop { val result = \"${TEXT}-${if (flag) { continue } else { 0 }}\"\nbreak }",
            1,
            1,
        ),
        (
            "val result = \"${TEXT}-${if (flag) { stop() } else { 0 }}\"",
            0,
            1,
        ),
        ("val result = \"${TEXT}-${stop()}\"", 0, 0),
    ] {
        let source = format!(
            "const val TEXT = \"hi\"\nfun stop(): Nothing = stop()\nfun use(flag: Boolean): Unit {{ {body} }}"
        );
        unit_transfer_cleanup::interpolation(&source, transfers, normal);
    }
}
