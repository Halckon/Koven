//! Text storage accepts the exact byte-array identity, not another sequential container.
use super::*;
use crate::ssa::model::{LoanId, SequentialContainerKind};

pub(super) fn shared_byte_array(module: &Module, function: &Function, source: LoanId) -> bool {
    let EntityType::Loan {
        target,
        kind: LoanKind::Shared,
    } = entity_type(function, EntityId::Loan(source))
    else {
        return false;
    };
    let Some(SsaTypeKind::SequentialContainer {
        kind: SequentialContainerKind::Array,
        element,
    }) = module.type_kind(target)
    else {
        return false;
    };
    matches!(
        module.type_kind(*element),
        Some(SsaTypeKind::Integer {
            bits: 8,
            signed: false
        })
    )
}
