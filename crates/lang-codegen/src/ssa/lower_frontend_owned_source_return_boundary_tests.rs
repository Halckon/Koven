//! Ordinary-source boundaries kept separate from the core acceptance matrix.
use super::{analyze, lower_scalar_file};

fn observe(text: &str, supported: bool) {
    let a = analyze(text);
    assert!(
        a.parsed.diagnostics().is_empty(),
        "{:?}",
        a.parsed.diagnostics()
    );
    assert!(
        a.names.diagnostics().is_empty(),
        "{:?}",
        a.names.diagnostics()
    );
    assert!(
        a.typed.diagnostics().is_empty(),
        "{:?}",
        a.typed.diagnostics()
    );
    assert!(
        a.owned.diagnostics().is_empty(),
        "{:?}",
        a.owned.diagnostics()
    );
    let result = lower_scalar_file(&a.sources, &a.parsed, &a.names, &a.typed, &a.owned);
    if supported {
        crate::llvm::render_verified_program(&result.unwrap()).unwrap();
    } else {
        let error = result
            .err()
            .expect("bounded source proof remains unsupported");
        assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
        assert_eq!(a.sources.slice(error.span.unwrap()).unwrap(), "keys");
        println!("BOUNDARY {text}: {error:?}");
    }
}

#[test]
fn owned_source_return_group_and_same_type_decoy() {
    for live in [false, true] {
        let text = super::source("arrayOf", "Array", false, live, true)
            .replace(
                "val keys = arrayOf(1, 2, 3)",
                "var keys = ((arrayOf(1, 2, 3)))",
            )
            .replace("if (key == 2)", "val decoy = arrayOf(7, 8); if (key == 2)")
            .replace("println(\"tail\")", "decoy.size; println(\"tail\")");
        super::single_text(&text, live, true);
    }
}

#[test]
fn owned_source_return_two_returns_remain_bounded() {
    observe(
        "fun main(): Unit { val keys = arrayOf(1, 2); for (key in keys) { if (key == 1) { return }; if (key == 2) { return } }; keys.size }",
        false,
    );
}
#[test]
fn owned_source_return_mixed_continue_remains_bounded() {
    observe(
        "fun main(): Unit { val keys = arrayOf(1, 2); for (key in keys) { if (key == 1) { return }; if (key == 2) { continue } }; keys.size }",
        false,
    );
}
#[test]
fn owned_source_return_replacement_seed_remains_bounded() {
    observe(
        "fun main(): Unit { var keys = arrayOf(1, 2); keys = arrayOf(2, 3); for (key in keys) { if (key == 2) { return } }; keys.size }",
        false,
    );
}
#[test]
fn owned_source_return_alias_seed_remains_bounded() {
    observe(
        "fun main(): Unit { val original = arrayOf(1, 2); val keys = original; for (key in keys) { if (key == 2) { return } }; keys.size }",
        false,
    );
}
#[test]
fn owned_source_return_resource_source_remains_bounded() {
    observe(
        "class Item(val id: Int) { deinit() {} }\nfun main(): Unit { val keys = arrayOf(Item(1), Item(2)); for (key in keys) { if (key.id == 2) { return } }; keys.size }",
        false,
    );
}
#[test]
fn owned_source_return_fresh_inner_instance() {
    observe(
        "fun main(): Unit { for (outer in arrayOf(1, 2)) { val keys = arrayOf(1, 2); for (key in keys) { if (key == 2) { return } }; keys.size } }",
        true,
    );
}
#[test]
fn owned_source_return_outer_owner_with_inner_loop() {
    observe(
        "fun main(): Unit { val keys = arrayOf(1, 2); for (key in keys) { for (other in arrayOf(3)) { other }; if (key == 2) { return } }; keys.size }",
        false,
    );
}
#[test]
fn owned_source_return_same_source_nested() {
    observe(
        "fun main(): Unit { val keys = arrayOf(1, 2); for (outer in keys) { for (key in keys) { if (key == 2) { return } } }; keys.size }",
        false,
    );
}
#[test]
fn owned_source_return_same_source_sequential() {
    observe(
        "fun main(): Unit { val keys = arrayOf(1, 2); for (key in keys) { if (key == 2) { return } }; for (key in keys) { if (key == 2) { return } }; keys.size }",
        false,
    );
}

#[test]
fn owned_source_return_source_loan_rejects_body_moves_and_mutation() {
    for (factory, operation) in [
        ("arrayOf", "consume(keys)"),
        ("arrayOf", "keys[0] = 9"),
        ("mutableListOf", "keys.add(4)"),
        ("mutableListOf", "keys.removeLast()"),
    ] {
        let text = format!(
            "fun consume(own source: Array<Int>): Unit {{}}\nfun main(): Unit {{ var keys = {factory}(1, 2, 3); for (key in keys) {{ {operation}; if (key == 2) {{ return }} }} }}"
        );
        let a = analyze(&text);
        assert!(a.parsed.diagnostics().is_empty());
        assert!(a.names.diagnostics().is_empty());
        assert!(a.typed.diagnostics().is_empty());
        assert!(
            a.owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0135")
        );
    }
}
