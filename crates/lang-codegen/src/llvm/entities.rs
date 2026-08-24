//! SSA entity 类型与 instruction result 的只读 LLVM 映射辅助。

use crate::ssa::model::{
    EntityId, EntityType, Function, Instruction, LoanId, PlaceAccess, PlaceId, SsaTypeId, ValueId,
};

use super::LlvmAdapterError;

pub(super) fn value_type(
    function: &Function,
    value: ValueId,
) -> Result<SsaTypeId, LlvmAdapterError> {
    match function.entity(EntityId::Value(value)).map(|data| data.ty) {
        Some(EntityType::Value(ty)) => Ok(ty),
        _ => Err(LlvmAdapterError::InvalidSsa(
            "ValueId 缺少 value entity type".to_owned(),
        )),
    }
}

pub(super) fn place_type(
    function: &Function,
    place: PlaceId,
) -> Result<SsaTypeId, LlvmAdapterError> {
    match function.entity(EntityId::Place(place)).map(|data| data.ty) {
        Some(EntityType::Place(ty)) => Ok(ty),
        _ => Err(LlvmAdapterError::InvalidSsa(
            "PlaceId 缺少 place entity type".to_owned(),
        )),
    }
}

pub(super) fn access_type(
    function: &Function,
    access: PlaceAccess,
) -> Result<SsaTypeId, LlvmAdapterError> {
    let entity = match access {
        PlaceAccess::Place(place) => EntityId::Place(place),
        PlaceAccess::Loan(loan) => EntityId::Loan(loan),
    };
    function
        .entity(entity)
        .map(|data| data.ty.semantic_type())
        .ok_or_else(|| LlvmAdapterError::InvalidSsa("place access 缺少 entity type".to_owned()))
}

pub(super) fn value_results(instruction: &Instruction) -> Result<Vec<ValueId>, LlvmAdapterError> {
    instruction
        .results
        .iter()
        .map(|entity| match entity {
            EntityId::Value(value) => Ok(*value),
            EntityId::Place(_) | EntityId::Loan(_) => Err(LlvmAdapterError::Unsupported(
                "LLVM instruction result 必须是 value".to_owned(),
            )),
        })
        .collect()
}

pub(super) fn place_result(instruction: &Instruction) -> Result<PlaceId, LlvmAdapterError> {
    match instruction.results.as_slice() {
        [EntityId::Place(place)] => Ok(*place),
        _ => Err(LlvmAdapterError::InvalidSsa(
            "place operation 必须产生一个 place".to_owned(),
        )),
    }
}

pub(super) fn loan_result(instruction: &Instruction) -> Result<LoanId, LlvmAdapterError> {
    match instruction.results.as_slice() {
        [EntityId::Loan(loan)] => Ok(*loan),
        _ => Err(LlvmAdapterError::InvalidSsa(
            "borrow begin 必须产生一个 loan".to_owned(),
        )),
    }
}

pub(super) fn value_name(value: ValueId) -> String {
    format!("v{}", value.index())
}
