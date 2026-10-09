//! N1a inline descriptor typing; its root loan is never erased into an owned ABI.
use super::*;
use crate::ssa::model::{LoanId, TerminatorKind};

pub(in crate::ssa) fn parameter(
    module: &Module,
    function: &Function,
) -> Option<(LoanId, SsaTypeId, SsaTypeId)> {
    if function.receiver.is_some() || function.borrow_return.is_some() {
        return None;
    }
    let index = function.carrier_return?;
    let [view] = function.return_types.as_slice() else {
        return None;
    };
    let SsaTypeKind::RangeView { source } = module.type_kind(*view)? else {
        return None;
    };
    let entity = *function.blocks.first()?.parameters.get(index)?;
    let EntityId::Loan(loan) = entity else {
        return None;
    };
    (source_root(module, function, loan) == Some(*source)).then_some((loan, *view, *source))
}

fn source_root(module: &Module, function: &Function, loan: LoanId) -> Option<SsaTypeId> {
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    } = function.entity(EntityId::Loan(loan))?.ty
    else {
        return None;
    };
    Some(match module.type_kind(target)? {
        SsaTypeKind::RangeView { source } => *source,
        _ => target,
    })
}

pub(super) fn contract(
    module: &Module,
    function: &Function,
    operation: &Operation,
    results: &[EntityType],
) -> bool {
    let pair = |view, source| {
        results
            == [
                EntityType::Value(view),
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: source,
                },
            ]
    };
    let source_type = |loan| match function.entity(EntityId::Loan(loan)).map(|data| data.ty) {
        Some(EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        }) => Some(target),
        _ => None,
    };
    let int = |value| {
        value_type(function, value).is_some_and(|ty| {
            matches!(
                module.type_kind(ty),
                Some(SsaTypeKind::Integer {
                    bits: 32,
                    signed: true
                })
            )
        })
    };
    match operation {
        Operation::RangeElementPlace { view, index } => {
            let Some(SsaTypeKind::RangeView { source }) =
                source_type(*view).and_then(|ty| module.type_kind(ty))
            else {
                return false;
            };
            matches!(module.type_kind(*source), Some(SsaTypeKind::SequentialContainer { element, .. }) if results == [EntityType::Place(*element)])
                && int(*index)
        }
        Operation::RangeConstruct {
            view,
            source,
            begin,
            end,
        } => {
            matches!(module.type_kind(*view),Some(SsaTypeKind::RangeView { source: target }) if Some(*target)==source_root(module,function,*source) && pair(*view,*target))
                && int(*begin)
                && int(*end)
        }
        Operation::RangeCall {
            callee,
            arguments,
            source,
        } => {
            let Some(callee) = module.function(*callee) else {
                return false;
            };
            let Some((_, view, root)) = parameter(module, callee) else {
                return false;
            };
            let Some(entry) = callee.blocks.first() else {
                return false;
            };
            arguments.get(callee.carrier_return.unwrap_or(usize::MAX))
                == Some(&EntityId::Loan(*source))
                && arguments.len() == entry.parameters.len()
                && arguments.iter().zip(&entry.parameters).all(|(arg, param)| {
                    function.entity(*arg).map(|d| d.ty) == callee.entity(*param).map(|d| d.ty)
                })
                && pair(view, root)
        }
        Operation::RangeLength { view } => {
            let ty = match function.entity(*view).map(|d| d.ty) {
                Some(
                    EntityType::Value(ty)
                    | EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: ty,
                    },
                ) => ty,
                _ => return false,
            };
            matches!(module.type_kind(ty), Some(SsaTypeKind::RangeView { .. }))
                && single_value_result(results).is_some_and(|ty| {
                    matches!(
                        module.type_kind(ty),
                        Some(SsaTypeKind::Integer {
                            bits: 32,
                            signed: true
                        })
                    )
                })
        }
        Operation::RangeEnd { view, source } => {
            results.is_empty()
                && matches!(value_type(function,*view).and_then(|ty|module.type_kind(ty)),Some(SsaTypeKind::RangeView {source:target}) if Some(*target)==source_type(*source))
        }
        _ => false,
    }
}

pub(in crate::ssa) fn return_contract(
    module: &Module,
    function: &Function,
    view: ValueId,
    source: LoanId,
    block: crate::ssa::model::BlockId,
    origin: &crate::ssa::model::Origin,
    errors: &mut Vec<VerifyError>,
) {
    let valid = parameter(module, function).is_some_and(|(_, expected, root)| {
        value_type(function, view) == Some(expected)
            && function.entity(EntityId::Loan(source)).map(|d| d.ty)
                == Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: root,
                })
    });
    if !valid {
        errors.push(VerifyError {
            kind: VerifyErrorKind::ReturnType { index: 0 },
            location: VerifyLocation::Terminator(block),
            origin: Some(origin.clone()),
        });
    }
}

pub(in crate::ssa) fn verify_signature(
    module: &Module,
    function: &Function,
    errors: &mut Vec<VerifyError>,
) {
    let invalid = function.carrier_return.is_some() && parameter(module,function).is_none()
        || function.return_types.iter().any(|ty|matches!(module.type_kind(*ty),Some(SsaTypeKind::RangeView {..}))) && function.carrier_return.is_none() && function.borrow_return.is_none()
        || function.blocks.first().is_some_and(|entry| entry.parameters.iter().any(|p| matches!(function.entity(*p).map(|d|d.ty),Some(EntityType::Value(ty) | EntityType::Loan {kind:LoanKind::Exclusive,target:ty}) if matches!(module.type_kind(ty),Some(SsaTypeKind::RangeView {..})))))
        || function.blocks.iter().any(|block| matches!(block.terminator.as_ref().map(|t| &t.kind),Some(TerminatorKind::Return {..})) && function.carrier_return.is_some());
    if invalid {
        errors.push(VerifyError {
            kind: VerifyErrorKind::ReturnType { index: 0 },
            location: VerifyLocation::Function(function.id),
            origin: Some(function.origin.clone()),
        });
    }
}
