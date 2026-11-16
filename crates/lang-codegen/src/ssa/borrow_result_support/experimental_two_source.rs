//! Selected bounded two-source controls; ordinary facts still provide no such authority.
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::UnitSymbolId,
    ownership_checking::{
        BorrowBindingFact, BorrowBindingStorage, UnitLoanTarget, UnitPreflightFacts,
    },
    parser::{Expression, ParsedFile},
    type_checking::UnitExpressionId,
};
pub(in crate::ssa) fn admitted(
    file: &ParsedFile,
    owned: &dyn UnitPreflightFacts,
    binding: &BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>,
) -> Option<([ExpressionId; 3], StatementId)> {
    let projection = owned.experimental_two_source_binding(binding.binding())?;
    let fact = projection.fact();
    let shell = projection.shell();
    let unit = binding.binding().source_unit();
    if fact.branch().source_unit() != unit || shell.body().source_id() != file.source_id() {
        return None;
    }
    let Expression::If {
        then_branch,
        else_branch: Some(else_branch),
        ..
    } = file
        .ast()
        .expressions()
        .get(fact.branch().expression())
        .ok()?
        .payload()
    else {
        return None;
    };
    if fact.arms()[0].body().statement() != *then_branch
        || fact.arms()[1].body().statement() != *else_branch
    {
        return None;
    }
    // BOTH arms must still match their unique real binding, source loan and End.
    for arm in fact.arms() {
        let mut matches = owned
            .borrow_results()
            .bindings()
            .iter()
            .filter(|b| b.binding() == arm.binding());
        let actual = matches.next()?;
        let source = actual.source_loan()?;
        if matches.next().is_some()
            || arm.binding().source_unit() != unit
            || arm.callable().source_unit() != unit
            || arm.loop_statement().source_unit() != unit
            || actual.marker_span().source_id() != file.source_id()
            || actual.storage() != BorrowBindingStorage::BorrowedStorage
            || actual.parent().is_some()
            || arm.initializer() != actual.initializer()
            || source.call() != arm.initializer()
            || source.argument() != arm.source_argument()
            || !matches!(actual.origin(), UnitLoanTarget::Place(p) if p.is_root() && p.root() == arm.source())
            || owned.owns_no_carry_loop(arm)
        {
            return None;
        }
        let mut ends = owned
            .borrow_results()
            .ends()
            .iter()
            .filter(|end| end.binding() == arm.binding());
        if ends.next()?.point() != arm.end() || ends.next().is_some() {
            return None;
        }
    }
    let [_, condition, _, decrement] = shell.controls();
    if [condition, decrement]
        .iter()
        .any(|id| id.source_unit() != unit)
    {
        return None;
    }
    Some((
        [
            condition.expression(),
            fact.branch().expression(),
            decrement.expression(),
        ],
        fact.arms()[0].loop_statement().statement(),
    ))
}
