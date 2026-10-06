//! Local type contract for synchronous borrowed container generation.

use super::{entity_type, is_koven_int, single_value_result, value_type};
use crate::ssa::model::{
    EntityId, EntityType, Function, LoanId, LoanKind, Module, SsaTypeId, SsaTypeKind, ValueId,
};

pub(super) fn contract(
    module: &Module,
    function: &Function,
    container: SsaTypeId,
    length: ValueId,
    initializer: LoanId,
    results: &[EntityType],
) -> bool {
    let Some((_, element)) = module.sequential_container(container) else {
        return false;
    };
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    } = entity_type(function, EntityId::Loan(initializer))
    else {
        return false;
    };
    let Some(signature) = module.callable_signature(target) else {
        return false;
    };
    let [
        EntityType::Loan {
            kind: LoanKind::Shared,
            target: index,
        },
    ] = signature.parameters.as_slice()
    else {
        return false;
    };
    value_type(function, length).is_some_and(|ty| is_koven_int(module, ty))
        && is_koven_int(module, *index)
        && single_value_result(results) == Some(container)
        && (signature.returns == [element]
            || (signature.returns.is_empty()
                && matches!(module.type_kind(element), Some(SsaTypeKind::Unit))))
}
