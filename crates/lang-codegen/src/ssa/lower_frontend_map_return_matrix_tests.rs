//! Frozen ordinary sources shared by SSA and unmodified object/link/run.
use super::{analyze, lower_scalar_file};

fn check(source: &str) {
    let analysis = analyze(source);
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
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
fn single_map_return_cardinality_matrix() {
    check(include_str!(
        "../../tests/fixtures/map-conditional-return-0313/live-dead.ko"
    ));
}
#[test]
fn single_map_return_distinct_owner_matrix() {
    check(include_str!(
        "../../tests/fixtures/map-conditional-return-0313/distinct-owners.ko"
    ));
}
#[test]
fn single_map_return_provider_matrix() {
    check(include_str!(
        "../../tests/fixtures/map-conditional-return-0313/borrowed-temporary-providers.ko"
    ));
}
#[test]
fn single_map_return_operand_cleanup_matrix() {
    check(include_str!(
        "../../tests/fixtures/map-conditional-return-0313/operand-cleanup.ko"
    ));
}
