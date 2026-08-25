//! Tagged-union construction and statically refined payload access.

use inkwell::{
    builder::Builder,
    values::{IntValue, PointerValue, StructValue},
};

use crate::ssa::model::SsaTypeId;

use super::{LlvmAdapterError, type_map::TypeMap};

pub(super) fn construct<'ctx>(
    builder: &Builder<'ctx>,
    types: &TypeMap<'ctx>,
    tagged: SsaTypeId,
    variant: usize,
    payload: StructValue<'ctx>,
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let layout = types.tagged_layout(tagged)?;
    if layout.payloads.get(variant).copied() != Some(payload.get_type()) {
        return Err(LlvmAdapterError::InvalidSsa(
            "tagged construction payload type does not match variant".to_owned(),
        ));
    }
    let storage = builder.build_alloca(layout.value, &format!("{name}.slot"))?;
    let tag = builder.build_struct_gep(layout.value, storage, 0, &format!("{name}.tag"))?;
    builder.build_store(
        tag,
        payload.get_type().get_context().i32_type().const_int(
            u64::try_from(variant).map_err(|_| {
                LlvmAdapterError::InvalidSsa("tagged variant index exceeds i32 tag".to_owned())
            })?,
            false,
        ),
    )?;
    let payload_slot =
        builder.build_struct_gep(layout.value, storage, 1, &format!("{name}.payload"))?;
    builder.build_store(payload_slot, payload)?;
    Ok(builder
        .build_load(layout.value, storage, name)?
        .into_struct_value())
}

pub(super) fn discriminant<'ctx>(
    builder: &Builder<'ctx>,
    types: &TypeMap<'ctx>,
    tagged: SsaTypeId,
    owner: StructValue<'ctx>,
    name: &str,
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    let layout = types.tagged_layout(tagged)?;
    if owner.get_type() != layout.value {
        return Err(LlvmAdapterError::InvalidSsa(
            "tagged discriminant has invalid owner".to_owned(),
        ));
    }
    Ok(builder
        .build_extract_value(owner, 0, name)?
        .into_int_value())
}

pub(super) fn payload_place<'ctx>(
    builder: &Builder<'ctx>,
    types: &TypeMap<'ctx>,
    tagged: SsaTypeId,
    variant: usize,
    owner: StructValue<'ctx>,
    name: &str,
) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
    let layout = types.tagged_layout(tagged)?;
    if layout.payloads.get(variant).is_none() || owner.get_type() != layout.value {
        return Err(LlvmAdapterError::InvalidSsa(
            "tagged payload place has invalid owner or variant".to_owned(),
        ));
    }
    let storage = builder.build_alloca(layout.value, &format!("{name}.owner"))?;
    builder.build_store(storage, owner)?;
    Ok(builder.build_struct_gep(layout.value, storage, 1, name)?)
}
