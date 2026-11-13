//! Admission consumes an owned arm pair, never an arm masquerading as a straight loop.
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::UnitSymbolId,
    ownership_checking::{
        BorrowBindingFact, BorrowBindingStorage, CompilationUnitOwnership, UnitLoanTarget,
    },
    parser::{Expression, ParsedFile},
    type_checking::UnitExpressionId,
};
pub(in crate::ssa) fn admitted(
    file: &ParsedFile,
    owned: &CompilationUnitOwnership,
    binding: &BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>,
) -> Option<(StatementId, ExpressionId)> {
    let mut selected = owned
        .no_carry_branches()
        .iter()
        .filter(|p| p.arms().iter().any(|a| a.binding() == binding.binding()));
    let pair = selected.next()?;
    if selected.next().is_some() || !owned.owns_no_carry_branch(pair) {
        return None;
    }
    let branch = file
        .ast()
        .expressions()
        .get(pair.branch().expression())
        .ok()?;
    let Expression::If {
        then_branch,
        else_branch: Some(else_branch),
        ..
    } = branch.payload()
    else {
        return None;
    };
    if pair.branch().source_unit() != binding.binding().source_unit()
        || pair.arms()[0].body().statement() != *then_branch
        || pair.arms()[1].body().statement() != *else_branch
    {
        return None;
    }
    // Both arms must retain their own real binding and End, even when only one is requested.
    for proof in pair.arms() {
        let mut bindings = owned
            .borrow_results()
            .bindings()
            .iter()
            .filter(|b| b.binding() == proof.binding());
        let b = bindings.next()?;
        let source = b.source_loan()?;
        if bindings.next().is_some()
            || b.storage() != BorrowBindingStorage::BorrowedStorage
            || b.parent().is_some()
            || proof.initializer() != b.initializer()
            || proof.initializer() != source.call()
            || proof.source_argument() != source.argument()
            || file.source_id() != b.marker_span().source_id()
            || !matches!(b.origin(),UnitLoanTarget::Place(p) if p.is_root() && p.root()==proof.source())
            || owned.owns_no_carry_loop(proof)
        {
            return None;
        }
        let mut ends = owned
            .borrow_results()
            .ends()
            .iter()
            .filter(|e| e.binding() == proof.binding());
        if ends.next()?.point() != proof.end() || ends.next().is_some() {
            return None;
        }
    }
    Some((
        pair.arms()[0].loop_statement().statement(),
        pair.branch().expression(),
    ))
}
