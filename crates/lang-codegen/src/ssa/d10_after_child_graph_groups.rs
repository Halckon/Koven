//! Dependency-based instruction groups and real edge transport for the closed graph.
use super::*;
use lang_frontend::{
    name_resolution::SourceUnitId,
    ownership_checking::{UnitDropFact, UnitLoanFact, d10_catalog_experiment::RetainedLoopRoot},
    parser::{Expression, SyntaxAst},
};
pub(super) fn constant_origin(
    ast: &SyntaxAst,
    id: UnitExpressionId,
    condition: bool,
    source: SourceUnitId,
) -> Option<Origin> {
    if id.source_unit() != source {
        return None;
    }
    let operand = match (
        condition,
        ast.expressions().get(id.expression()).ok()?.payload(),
    ) {
        (true, Expression::Binary { right, .. }) => *right,
        (false, Expression::Assignment { value, .. }) => *value,
        _ => return None,
    };
    Some(Origin::Source(ast.expressions().get(operand).ok()?.span()))
}
pub(super) fn condition<'a>(
    g: &Graph<'a>,
    cmp: &Instruction,
    env: &Env,
    role: Role,
    origin: Origin,
) -> Option<&'a Instruction> {
    let Operation::Compare {
        operator: ComparisonOperator::GreaterThan,
        left,
        right,
    } = cmp.operation
    else {
        return None;
    };
    if left != role_value(env, role)? {
        return None;
    }
    g.integer(left)?;
    let zero = g.unique(&origin, |o| {
        matches!(o, Operation::Constant(ScalarConstant::Integer(0)))
    })?;
    if value(zero)? != right || zero.block != cmp.block {
        return None;
    }
    g.integer(right)?;
    value(cmp)?;
    Some(zero)
}
pub(super) fn step<'a>(
    g: &Graph<'a>,
    step: &Instruction,
    env: &Env,
    role: Role,
    origin: Origin,
) -> Option<&'a Instruction> {
    let Operation::CheckedArithmetic {
        operator: CheckedArithmeticOperator::Subtract,
        left,
        right,
    } = step.operation
    else {
        return None;
    };
    let [EntityId::Value(updated), EntityId::Value(_overflow)] = step.results.as_slice() else {
        return None;
    };
    if left != role_value(env, role)? {
        return None;
    }
    g.integer(left)?;
    g.integer(*updated)?;
    let one = g.unique(&origin, |o| {
        matches!(o, Operation::Constant(ScalarConstant::Integer(1)))
    })?;
    if value(one)? != right || one.block != step.block {
        return None;
    }
    g.integer(right)?;
    Some(one)
}
pub(super) fn replacement<'a>(
    g: &Graph<'a>,
    replace: &'a Instruction,
    literal: &'a Instruction,
    current: ValueId,
    loan: &UnitLoanFact,
    drop: UnitDropFact,
) -> Option<Vec<&'a Instruction>> {
    let Operation::RootReplace {
        owner,
        loan: actual_loan,
        replacement,
    } = replace.operation
    else {
        return None;
    };
    let [EntityId::Value(new), EntityId::Value(old)] = replace.results.as_slice() else {
        return None;
    };
    if owner != current || replacement != value(literal)? || new == old {
        return None;
    }
    g.owner(owner)?;
    g.owner(*new)?;
    g.owner(*old)?;
    let origin = Origin::Source(loan.begin_span());
    let place = g.unique(
        &origin,
        |o| matches!(o,Operation::RootPlace{owner:o} if *o==owner),
    )?;
    let [EntityId::Place(place_id)] = place.results.as_slice() else {
        return None;
    };
    let begin = g.unique(
        &origin,
        |o| matches!(o,Operation::BorrowBegin{place:p,kind:LoanKind::Exclusive} if p==place_id),
    )?;
    if begin.results.as_slice() != [EntityId::Loan(actual_loan)] {
        return None;
    }
    let cleanup = g.unique(
        &Origin::Source(drop.value_origin()),
        |o| matches!(o,Operation::Drop{owner:v} if v==old),
    )?;
    if !cleanup.results.is_empty() {
        return None;
    }
    let group = vec![place, begin, literal, replace, cleanup];
    if group.iter().any(|i| i.block != replace.block) {
        return None;
    }
    Some(group)
}
pub(super) fn suffix<'a>(
    g: &Graph<'a>,
    source: &RetainedLoopRoot,
    current: ValueId,
    block: BlockId,
    call_origin: Origin,
) -> Option<Vec<&'a Instruction>> {
    g.owner(current)?;
    let origin = Origin::Source(source.loan().begin_span());
    let place = g.unique(
        &origin,
        |o| matches!(o,Operation::RootPlace{owner} if *owner==current),
    )?;
    let [EntityId::Place(place_id)] = place.results.as_slice() else {
        return None;
    };
    let begin = g.unique(
        &origin,
        |o| matches!(o,Operation::BorrowBegin{place,kind:LoanKind::Shared} if place==place_id),
    )?;
    let [EntityId::Loan(loan)] = begin.results.as_slice() else {
        return None;
    };
    let print = g.unique(
        &call_origin,
        |o| matches!(o,Operation::PrintString{value} if value==loan),
    )?;
    let end = g.unique(
        &call_origin,
        |o| matches!(o,Operation::BorrowEnd{loan:l} if l==loan),
    )?;
    let drop = g.unique(
        &Origin::Source(source.cleanup().value_origin()),
        |o| matches!(o,Operation::Drop{owner} if *owner==current),
    )?;
    if [print, end, drop].iter().any(|i| !i.results.is_empty()) {
        return None;
    }
    let group = vec![place, begin, print, end, drop];
    if group.iter().any(|i| i.block != block) {
        return None;
    }
    Some(group)
}
pub(super) fn step_edges(
    g: &Graph<'_>,
    step: &Instruction,
    one: &Instruction,
    updated: &Env,
    roles: &[Role],
    header: BlockId,
    header_env: &Env,
) -> Option<(BlockId, BlockId)> {
    let [EntityId::Value(_), EntityId::Value(overflow)] = step.results.as_slice() else {
        return None;
    };
    let (abort, resume) = g.conditional(step.block, *overflow)?;
    if abort.target == resume.target {
        return None;
    }
    g.transport(abort, updated, roles)?;
    let resumed = g.transport(resume, updated, roles)?;
    let terminator = g.term(abort.target)?;
    if terminator.kind != TerminatorKind::Abort
        || terminator.abort
            != Some(AbortReason::CheckedArithmetic {
                operator: CheckedArithmeticOperator::Subtract,
                divisor: value(one)?,
            })
    {
        return None;
    }
    let back = g.branch(resume.target)?;
    if back.target != header || g.transport(back, &resumed, roles)? != *header_env {
        return None;
    }
    Some((abort.target, resume.target))
}
