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
    let mut selected =
        pairs(owned).filter(|p| p.arms().iter().any(|a| a.binding() == binding.binding()));
    let pair = selected.next()?;
    if selected.next().is_some() || !owns_pair(owned, pair) {
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

/// Internal mechanical traversal; the frontend public capability domains stay separate.
pub(in crate::ssa) fn pairs(
    owned: &CompilationUnitOwnership,
) -> impl Iterator<Item = &lang_frontend::ownership_checking::UnitLoopBranchNoCarryFact> {
    owned
        .no_carry_branches()
        .iter()
        .chain(owned.no_carry_counter_branches().iter().map(|f| f.pair()))
        .chain(owned.no_carry_break_branches().iter().map(|f| f.pair()))
        .chain(counter_breaks(owned).map(|f| f.counter().pair()))
}
pub(in crate::ssa) fn owns_pair(
    owned: &CompilationUnitOwnership,
    pair: &lang_frontend::ownership_checking::UnitLoopBranchNoCarryFact,
) -> bool {
    counter_breaks(owned)
        .any(|f| owns_counter_break(owned, f) && f.counter().pair().same_identity(pair))
        || owned.owns_no_carry_branch(pair)
        || owned
            .no_carry_counter_branches()
            .iter()
            .any(|f| owned.owns_no_carry_counter_branch(f) && f.pair().same_identity(pair))
        || owned
            .no_carry_break_branches()
            .iter()
            .any(|f| owned.owns_no_carry_break_branch(f) && f.pair().same_identity(pair))
}

/// Mechanical traversal still requires the owning domain at every admission.
pub(in crate::ssa) fn counter_breaks(
    owned: &CompilationUnitOwnership,
) -> impl Iterator<Item = &lang_frontend::ownership_checking::UnitLoopCounterBreakBranchNoCarryFact>
{
    owned.no_carry_counter_break_branches().iter().chain(
        owned
            .no_carry_prefix_counter_breaks()
            .iter()
            .map(|p| p.counter_break()),
    )
}
pub(in crate::ssa) fn owns_counter_break(
    owned: &CompilationUnitOwnership,
    fact: &lang_frontend::ownership_checking::UnitLoopCounterBreakBranchNoCarryFact,
) -> bool {
    owned.owns_no_carry_counter_break_branch(fact)
        || owned.no_carry_prefix_counter_breaks().iter().any(|p| {
            owned.owns_no_carry_prefix_counter_break(p) && p.counter_break().same_identity(fact)
        })
}
