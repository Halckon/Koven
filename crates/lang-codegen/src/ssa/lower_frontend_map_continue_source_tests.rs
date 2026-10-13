//! Ordinary-source diagnosis of the independent owned provider presence boundary.
use super::{analyze, lower_scalar_file};

fn check(text: &str) {
    let analysis = analyze(text);
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    eprintln!(
        "SOURCE={text}\nDROPS={:?}\nITERATIONS={:?}\nCONDITIONS={:?}",
        analysis.owned.drops(),
        analysis.owned.iterations(),
        analysis.owned.cleanup_conditions()
    );
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
}

#[test]
fn map_continue_source_probe_owned_after_use_without_map() {
    check(
        "fun main(): Unit { val keys = arrayOf(1, 2, 3); for (key in keys) { if (key == 2) { continue } }; keys.size }",
    );
}

#[test]
fn map_continue_source_probe_owned_without_after_use_with_map() {
    check(
        "fun main(): Unit { val keys = arrayOf(1, 2, 3); var cache = mutableMapOf<Int, String>(); for (key in keys) { cache.put(key, \"head\".clone()); if (key == 2) { continue }; cache.put(9, \"tail\".clone()) }; cache.contains(9) }",
    );
}

#[test]
fn map_continue_source_probe_temporary_with_map() {
    check(
        "fun main(): Unit { var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2, 3)) { cache.put(key, \"head\".clone()); if (key == 2) { continue }; cache.put(9, \"tail\".clone()) }; cache.contains(9) }",
    );
}
