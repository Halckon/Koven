//! Consume the sealed core no-carry fact; never infer fresh-loop legality from syntax alone.
use lang_frontend::{
    ast::StatementId,
    name_resolution::UnitSymbolId,
    ownership_checking::{
        BorrowBindingFact, BorrowBindingStorage, CompilationUnitOwnership, UnitLoanTarget,
    },
    parser::{ParsedFile, Statement},
    type_checking::UnitExpressionId,
};

pub(in crate::ssa) fn admitted(
    file: &ParsedFile,
    owned: &CompilationUnitOwnership,
    binding: &BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>,
) -> Option<StatementId> {
    let mut selected = owned
        .no_carry_loops()
        .iter()
        .filter(|proof| proof.binding() == binding.binding());
    let proof = selected.next()?;
    let source = binding.source_loan()?;
    if selected.next().is_some()
        || !owned.owns_no_carry_loop(proof)
        || binding.storage() != BorrowBindingStorage::BorrowedStorage
        || binding.parent().is_some()
        || proof.initializer() != binding.initializer()
        || proof.initializer() != source.call()
        || proof.source_argument() != source.argument()
        || proof.body().source_unit() != binding.binding().source_unit()
        || proof.loop_statement().source_unit() != binding.binding().source_unit()
        || proof.callable().source_unit() != binding.binding().source_unit()
        || file.source_id() != binding.marker_span().source_id()
        || !matches!(binding.origin(), UnitLoanTarget::Place(p) if p.is_root() && p.root() == proof.source())
    {
        return None;
    }
    let mut ends = owned
        .borrow_results()
        .ends()
        .iter()
        .filter(|e| e.binding() == binding.binding());
    if ends.next()?.point() != proof.end() || ends.next().is_some() {
        return None;
    }
    let Statement::While { body, .. } = file
        .ast()
        .statements()
        .get(proof.loop_statement().statement())
        .ok()?
        .payload()
    else {
        return None;
    };
    (*body == proof.body().statement()).then_some(proof.loop_statement().statement())
}
