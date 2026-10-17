//! Exact source, cursor and separate K/V result contracts for ordered Map iteration.
use super::*;
use crate::ssa::model::{LoanId, ValueId};

fn source_types(
    module: &Module,
    function: &Function,
    source: LoanId,
) -> Option<(SsaTypeId, SsaTypeId)> {
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    } = function.entity(EntityId::Loan(source))?.ty
    else {
        return None;
    };
    let (_, key, value) = module.map_container(target)?;
    Some((key, value))
}

fn is_cursor(module: &Module, ty: SsaTypeId) -> bool {
    matches!(
        module.type_kind(ty),
        Some(SsaTypeKind::Integer {
            bits: 64,
            signed: false
        })
    )
}

fn cursor_type(module: &Module, function: &Function, cursor: ValueId) -> Option<SsaTypeId> {
    value_type(function, cursor).filter(|ty| is_cursor(module, *ty))
}

pub(super) fn verify(
    module: &Module,
    function: &Function,
    operation: &Operation,
    results: &[EntityType],
) -> bool {
    let (source, cursor) = match *operation {
        Operation::MapIterationFirst { source } => (source, None),
        Operation::MapIterationHasNext { source, cursor }
        | Operation::MapIterationNext { source, cursor }
        | Operation::MapIterationKeyLoan { source, cursor }
        | Operation::MapIterationValueLoan { source, cursor } => (source, Some(cursor)),
        _ => return false,
    };
    let Some((key, value)) = source_types(module, function, source) else {
        return false;
    };
    if cursor.is_some_and(|cursor| cursor_type(module, function, cursor).is_none()) {
        return false;
    }
    match *operation {
        Operation::MapIterationFirst { .. } => {
            single_value_result(results).is_some_and(|ty| is_cursor(module, ty))
        }
        Operation::MapIterationNext { cursor, .. } => {
            single_value_result(results) == cursor_type(module, function, cursor)
        }
        Operation::MapIterationHasNext { .. } => single_value_result(results)
            .is_some_and(|ty| matches!(module.type_kind(ty), Some(SsaTypeKind::Boolean))),
        Operation::MapIterationKeyLoan { .. } => {
            results
                == [EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: key,
                }]
        }
        Operation::MapIterationValueLoan { .. } => {
            results
                == [EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: value,
                }]
        }
        _ => false,
    }
}
