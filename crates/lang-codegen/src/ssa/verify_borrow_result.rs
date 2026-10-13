//! 普通 shared 结果的签名与唯一来源合同；非来源参数保留 Value/Shared Loan identity。
use super::{
    model::*,
    verify::{VerifyError, VerifyErrorKind, VerifyLocation},
};

pub(super) fn parameter(function: &Function) -> Option<(LoanId, SsaTypeId)> {
    if function.receiver.is_some() {
        return None;
    }
    let index = function.borrow_return?;
    if !function.blocks.first()?.parameters.iter().all(|entity| {
        matches!(
            function.entity(*entity).map(|data| data.ty),
            Some(
                EntityType::Value(_)
                    | EntityType::Loan {
                        kind: LoanKind::Shared,
                        ..
                    }
            )
        )
    }) {
        return None;
    }
    let [target] = function.return_types.as_slice() else {
        return None;
    };
    let entity = *function.blocks.first()?.parameters.get(index)?;
    let EntityId::Loan(loan) = entity else {
        return None;
    };
    matches!(
        function.entity(entity)?.ty,
        EntityType::Loan {
            kind: LoanKind::Shared,
            ..
        }
    )
    .then_some((loan, *target))
}

pub(super) fn call_contract(
    module: &Module,
    function: &Function,
    callee: FunctionId,
    arguments: &[EntityId],
    source: LoanId,
    results: &[EntityType],
) -> bool {
    let Some(callee) = module.function(callee) else {
        return false;
    };
    let Some((_, target)) = parameter(callee) else {
        return false;
    };
    let Some(index) = callee.borrow_return else {
        return false;
    };
    let Some(entry) = callee.blocks.first() else {
        return false;
    };
    arguments.len() == entry.parameters.len()
        && arguments.get(index) == Some(&EntityId::Loan(source))
        && arguments.iter().zip(&entry.parameters).all(|(arg, param)| {
            !matches!(arg, EntityId::Place(_))
                && callee.entity(*param).is_some_and(|parameter| {
                    super::verify_operation::is_first_class(module, parameter.ty.semantic_type())
                        && function.entity(*arg).map(|data| data.ty) == Some(parameter.ty)
                })
        })
        && super::verify_operation::is_first_class(module, target)
        && results
            == [EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            }]
}

pub(super) fn return_contract(
    function: &Function,
    loan: LoanId,
    block: BlockId,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    let valid = parameter(function).is_some_and(|(_, target)| {
        function.entity(EntityId::Loan(loan)).map(|data| data.ty)
            == Some(EntityType::Loan {
                kind: LoanKind::Shared,
                target,
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
