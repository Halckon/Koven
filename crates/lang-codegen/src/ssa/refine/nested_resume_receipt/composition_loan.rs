//! Shared exact replacement preparation check. RootReplace itself ends this loan.
use super::*;
use crate::ssa::model::{Definition, LoanKind};
pub(in crate::ssa::refine) fn verify(
    function: &Function,
    replace: InstructionId,
    owner: EntityId,
    span: Span,
) -> Result<(), LoweringError> {
    let fail = || invalid(span);
    let r = function.instruction(replace).ok_or_else(fail)?;
    let Operation::RootReplace { loan, .. } = r.operation else {
        return Err(fail());
    };
    let loan = EntityId::Loan(loan);
    let Definition::InstructionResult {
        instruction: borrow,
        index: 0,
    } = function.entity(loan).ok_or_else(fail)?.definition
    else {
        return Err(fail());
    };
    let b = function.instruction(borrow).ok_or_else(fail)?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Exclusive,
    } = b.operation
    else {
        return Err(fail());
    };
    let place = EntityId::Place(place);
    let Definition::InstructionResult {
        instruction: prepare,
        index: 0,
    } = function.entity(place).ok_or_else(fail)?.definition
    else {
        return Err(fail());
    };
    let p = function.instruction(prepare).ok_or_else(fail)?;
    if b.block != r.block
        || p.block != r.block
        || b.results != [loan]
        || p.results != [place]
        || !matches!(p.operation,Operation::RootPlace{owner:o} if EntityId::Value(o)==owner)
    {
        return Err(fail());
    }
    let block = function.block(r.block).ok_or_else(fail)?;
    let at = |id| {
        let positions = block
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(i, x)| (*x == id).then_some(i))
            .collect::<Vec<_>>();
        if positions.len() == 1 {
            Ok(positions[0])
        } else {
            Err(fail())
        }
    };
    if !(at(prepare)? < at(borrow)? && at(borrow)? < at(replace)?) {
        return Err(fail());
    }
    for i in &function.instructions {
        if i.results.contains(&loan) != (i.id == borrow)
            || i.results.contains(&place) != (i.id == prepare)
            || i.operation
                .entities()
                .iter()
                .filter(|e| **e == loan)
                .count()
                != usize::from(i.id == replace)
            || i.operation
                .entities()
                .iter()
                .filter(|e| **e == place)
                .count()
                != usize::from(i.id == borrow)
        {
            return Err(fail());
        }
    }
    for block in &function.blocks {
        let term = &block.terminator.as_ref().ok_or_else(fail)?.kind;
        if block.parameters.contains(&loan)
            || block.parameters.contains(&place)
            || term.entities().contains(&loan)
            || term.entities().contains(&place)
        {
            return Err(fail());
        }
    }
    Ok(())
}
