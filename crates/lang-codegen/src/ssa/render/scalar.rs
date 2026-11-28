//! Rendering for scalar constants and floating arithmetic.
use super::{EntityId, fmt, write_entity_id};
use crate::ssa::model::{FloatOperator, ScalarConstant, ValueId};
use std::fmt::Write;

pub(super) fn write_float_arithmetic(
    output: &mut String,
    operator: FloatOperator,
    left: ValueId,
    right: Option<ValueId>,
) -> fmt::Result {
    write!(output, "float.{operator:?} ")?;
    write_entity_id(output, EntityId::Value(left))?;
    if let Some(right) = right {
        output.write_str(", ")?;
        write_entity_id(output, EntityId::Value(right))?;
    }
    Ok(())
}

pub(super) fn write_constant(output: &mut String, constant: &ScalarConstant) -> fmt::Result {
    match constant {
        #[cfg(test)]
        ScalarConstant::SyntheticZero => output.write_str("synthetic-zero"),
        ScalarConstant::Unit => output.write_str("unit"),
        ScalarConstant::Boolean(value) => write!(output, "{value}"),
        ScalarConstant::Char(value) => write!(output, "U+{value:04X}"),
        ScalarConstant::Float32(bits) => write!(output, "f32:0x{bits:08x}"),
        ScalarConstant::Float64(bits) => write!(output, "f64:0x{bits:016x}"),
        ScalarConstant::Integer(value) => write!(output, "{value}"),
    }
}
