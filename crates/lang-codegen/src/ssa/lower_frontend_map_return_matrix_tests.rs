//! Frozen ordinary sources shared by SSA and unmodified object/link/run.
use super::{analyze, lower_scalar_file};

fn check(source: &str) -> crate::ssa::model::Program {
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
    use lang_frontend::ownership_checking::{DropPoint, DropTarget};
    for fact in analysis.owned.drops() {
        if let (DropPoint::ControlTransfer(_), DropTarget::Named(symbol)) =
            (fact.point(), fact.target())
        {
            assert!(
                !analysis
                    .names
                    .symbols()
                    .iter()
                    .any(|candidate| candidate.id() == symbol && candidate.name() == "result"),
                "the owned return result must be excluded from transfer cleanup"
            );
        }
    }
    for function in program.modules.iter().flat_map(|module| &module.functions) {
        for block in &function.blocks {
            if let crate::ssa::model::TerminatorKind::Return { values } =
                &block.terminator.as_ref().unwrap().kind
            {
                for value in values {
                    assert!(!function.instructions.iter().any(|instruction| matches!(instruction.operation, crate::ssa::model::Operation::Drop { owner } if owner == *value)), "returned value has a single consuming Return");
                }
            }
        }
    }
    program
}

#[test]
fn single_map_return_cardinality_matrix() {
    check(include_str!(
        "../../tests/fixtures/map-conditional-return-0313/live-dead.ko"
    ));
}
#[test]
fn single_map_return_distinct_owner_matrix() {
    let program = check(include_str!(
        "../../tests/fixtures/map-conditional-return-0313/distinct-owners.ko"
    ));
    super::structure::assert_distinct_return_owners(&program);
}
#[test]
fn single_map_return_provider_matrix() {
    check(include_str!(
        "../../tests/fixtures/map-conditional-return-0313/borrowed-temporary-providers.ko"
    ));
}
#[test]
fn single_map_return_operand_cleanup_matrix() {
    let source =
        include_str!("../../tests/fixtures/map-conditional-return-0313/operand-cleanup.ko");
    let program = check(source);
    super::structure::assert_operand_before_body_cleanup(&program, source);
}

#[test]
fn single_map_return_owned_results_matrix() {
    check(include_str!(
        "../../tests/fixtures/map-conditional-return-0313/owned-results.ko"
    ));
}

#[test]
fn owned_source_return_operand_cleanup_structure() {
    let source = include_str!("../../tests/fixtures/owned-source-return-0314/operand-cleanup.ko");
    let program = check(source);
    super::structure::assert_owned_operand_before_body_cleanup(&program, source);
}

#[test]
fn owned_source_return_distinct_maps_structure() {
    let program = check(include_str!(
        "../../tests/fixtures/owned-source-return-0314/distinct-owners.ko"
    ));
    super::structure::assert_distinct_return_owners(&program);
}

#[test]
fn owned_source_return_independent_results_structure() {
    check(include_str!(
        "../../tests/fixtures/owned-source-return-0314/owned-results.ko"
    ));
}
