//! First-class aggregate value operation lowering。

use inkwell::{
    builder::Builder,
    values::{BasicValueEnum, StructValue},
};

use crate::ssa::model::SsaTypeId;

use super::{LlvmAdapterError, type_map::TypeMap};

pub(super) fn construct<'ctx>(
    builder: &Builder<'ctx>,
    types: &TypeMap<'ctx>,
    aggregate: SsaTypeId,
    fields: &[BasicValueEnum<'ctx>],
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let mut value = types.aggregate_type(aggregate)?.const_zero();
    for (index, field) in fields.iter().enumerate() {
        value = builder
            .build_insert_value(value, *field, index as u32, &format!("{name}.field{index}"))?
            .into_struct_value();
    }
    value.set_name(name);
    Ok(value)
}

pub(super) fn project<'ctx>(
    builder: &Builder<'ctx>,
    aggregate: StructValue<'ctx>,
    field: usize,
    name: &str,
) -> Result<BasicValueEnum<'ctx>, LlvmAdapterError> {
    Ok(builder.build_extract_value(aggregate, field as u32, name)?)
}

pub(super) fn explode<'ctx>(
    builder: &Builder<'ctx>,
    aggregate: StructValue<'ctx>,
    names: &[String],
) -> Result<Vec<BasicValueEnum<'ctx>>, LlvmAdapterError> {
    names
        .iter()
        .enumerate()
        .map(|(index, name)| Ok(builder.build_extract_value(aggregate, index as u32, name)?))
        .collect()
}
