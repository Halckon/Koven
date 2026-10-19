//! Actual provider lease retirement precedes its proved ordinary Return or final owner Drop.
use super::{LoweringError, LoweringErrorKind, transport};
use crate::ssa::model::*;
use lang_frontend::source::Span;
use std::collections::BTreeMap;

pub(in super::super) enum Terminal {
    Return(Span),
    Drop {
        marker: Span,
        call: Span,
        callee: FunctionId,
    },
}

fn instruction(
    function: &Function,
    entity: EntityId,
    span: Span,
) -> Result<&Instruction, LoweringError> {
    let bad = || LoweringError {
        kind: LoweringErrorKind::MissingFact,
        span: Some(span),
    };
    let Definition::InstructionResult {
        instruction,
        index: 0,
    } = function.entity(entity).ok_or_else(bad)?.definition
    else {
        return Err(bad());
    };
    let instruction = function.instruction(instruction).ok_or_else(bad)?;
    if instruction.results.as_slice() != [entity] {
        return Err(bad());
    }
    Ok(instruction)
}

pub(in super::super) fn complete(
    function: &Function,
    provider: Span,
    terminal: Terminal,
) -> Result<(EntityId, EntityId), LoweringError> {
    let bad = || LoweringError {
        kind: LoweringErrorKind::MissingFact,
        span: Some(provider),
    };
    let births = function
        .instructions
        .iter()
        .filter(|i| {
            i.origin == Origin::Source(provider)
                && matches!(i.operation, Operation::MapIterationFirst { .. })
        })
        .collect::<Vec<_>>();
    let [first] = births.as_slice() else {
        return Err(bad());
    };
    let Operation::MapIterationFirst { source } = first.operation else {
        return Err(bad());
    };
    let lease = instruction(function, EntityId::Loan(source), provider)?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = lease.operation
    else {
        return Err(bad());
    };
    let place = instruction(function, EntityId::Place(place), provider)?;
    let Operation::RootPlace { owner: seed } = place.operation else {
        return Err(bad());
    };
    let seed = EntityId::Value(seed);
    let none = BTreeMap::new();
    let ends = function
        .instructions
        .iter()
        .filter(|i| match i.operation {
            Operation::BorrowEnd { loan } => transport::check(
                function,
                EntityId::Loan(loan),
                EntityId::Loan(source),
                &none,
                provider,
            )
            .is_ok(),
            _ => false,
        })
        .collect::<Vec<_>>();
    let [lease_end] = ends.as_slice() else {
        return Err(bad());
    };
    let (block, owner, final_instruction) = match terminal {
        Terminal::Return(at) => {
            let blocks = function
                .blocks
                .iter()
                .filter(|b| {
                    b.terminator.as_ref().is_some_and(|t| {
                        t.origin == Origin::Source(at)
                            && matches!(t.kind, TerminatorKind::Return { .. })
                    })
                })
                .collect::<Vec<_>>();
            let [block] = blocks.as_slice() else {
                return Err(bad());
            };
            let Some(TerminatorKind::Return { values }) =
                block.terminator.as_ref().map(|t| &t.kind)
            else {
                return Err(bad());
            };
            let [owner] = values.as_slice() else {
                return Err(bad());
            };
            let ty = function.entity(EntityId::Value(*owner)).ok_or_else(bad)?.ty;
            if !matches!(ty, EntityType::Value(_))
                || function.return_types.as_slice() != [ty.semantic_type()]
                || function.borrow_return.is_some()
                || function.carrier_return.is_some()
            {
                return Err(bad());
            }
            if function
                .instructions
                .iter()
                .any(|i| i.operation == (Operation::Drop { owner: *owner }))
            {
                return Err(bad());
            }
            (block.id, *owner, None)
        }
        Terminal::Drop {
            marker,
            call,
            callee,
        } => {
            let drops = function
                .instructions
                .iter()
                .filter(|i| {
                    i.origin == Origin::Source(marker)
                        && matches!(i.operation, Operation::Drop { .. })
                })
                .collect::<Vec<_>>();
            let [drop] = drops.as_slice() else {
                return Err(bad());
            };
            let Operation::Drop { owner } = drop.operation else {
                return Err(bad());
            };
            let calls = function
                .instructions
                .iter()
                .filter(|i| {
                    i.origin == Origin::Source(call)
                        && matches!(i.operation, Operation::DirectCall { .. })
                })
                .collect::<Vec<_>>();
            let [observe] = calls.as_slice() else {
                return Err(bad());
            };
            let Operation::DirectCall {
                callee: actual,
                receiver: None,
                arguments,
            } = &observe.operation
            else {
                return Err(bad());
            };
            if *actual != callee {
                return Err(bad());
            }
            let [EntityId::Loan(loan)] = arguments.as_slice() else {
                return Err(bad());
            };
            let begin = instruction(function, EntityId::Loan(*loan), provider)?;
            let Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } = begin.operation
            else {
                return Err(bad());
            };
            let root = instruction(function, EntityId::Place(place), provider)?;
            if root.operation != (Operation::RootPlace { owner }) {
                return Err(bad());
            }
            let ends = function
                .instructions
                .iter()
                .filter(|i| i.operation == (Operation::BorrowEnd { loan: *loan }))
                .collect::<Vec<_>>();
            let [end] = ends.as_slice() else {
                return Err(bad());
            };
            let block = function.block(drop.block).ok_or_else(bad)?;
            let position = |id| {
                block
                    .instructions
                    .iter()
                    .position(|i| *i == id)
                    .ok_or_else(bad)
            };
            if [root, begin, observe, end]
                .iter()
                .any(|i| i.block != drop.block)
                || end.origin != Origin::Source(call)
                || !(position(root.id)? < position(begin.id)?
                    && position(begin.id)? < position(observe.id)?
                    && position(observe.id)? < position(end.id)?
                    && position(end.id)? < position(drop.id)?)
            {
                return Err(bad());
            }
            (drop.block, owner, Some(root.id))
        }
    };
    let owner = EntityId::Value(owner);
    transport::check(function, owner, seed, &none, provider)?;
    let block = function.block(block).ok_or_else(bad)?;
    let end = block
        .instructions
        .iter()
        .position(|id| *id == lease_end.id)
        .ok_or_else(bad)?;
    if lease_end.block != block.id
        || final_instruction.is_some_and(|id| {
            block
                .instructions
                .iter()
                .position(|i| *i == id)
                .is_none_or(|position| end >= position)
        })
    {
        return Err(bad());
    }
    Ok((seed, owner))
}
