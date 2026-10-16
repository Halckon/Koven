//! Integer conversion contracts derive source and target from actual SSA entities.
use super::{EntityType, Function, Module, ValueId, is_bitwise_integer, is_boolean, value_type};

pub(super) fn checked_conversion_contract(
    module: &Module,
    function: &Function,
    operand: ValueId,
    results: &[EntityType],
) -> bool {
    let [EntityType::Value(target), EntityType::Value(failed)] = results else {
        return false;
    };
    is_bitwise_integer(module, value_type(function, operand))
        && is_bitwise_integer(module, Some(*target))
        && is_boolean(module, *failed)
}
