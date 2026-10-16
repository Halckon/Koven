//! Numeric Range operations require a registered identity, never an aggregate lookalike.
use super::{EntityType, Function, Module, ValueId, is_boolean, single_value_result, value_type};

pub(super) fn construct(
    module: &Module,
    function: &Function,
    first: ValueId,
    bound: ValueId,
    results: &[EntityType],
) -> bool {
    let Some(range) = single_value_result(results) else {
        return false;
    };
    let Some(endpoint) = module.integer_range_endpoint(range) else {
        return false;
    };
    module.valid_integer_range_type(range, endpoint)
        && value_type(function, first) == Some(endpoint)
        && value_type(function, bound) == Some(endpoint)
}

pub(super) fn contains(
    module: &Module,
    function: &Function,
    value: ValueId,
    range: ValueId,
    results: &[EntityType],
) -> bool {
    let Some(range) = value_type(function, range) else {
        return false;
    };
    let Some(endpoint) = module.integer_range_endpoint(range) else {
        return false;
    };
    module.valid_integer_range_type(range, endpoint)
        && value_type(function, value) == Some(endpoint)
        && single_value_result(results).is_some_and(|ty| is_boolean(module, ty))
}

pub(super) fn equal(
    module: &Module,
    function: &Function,
    left: ValueId,
    right: ValueId,
    results: &[EntityType],
) -> bool {
    let Some(range) = value_type(function, left) else {
        return false;
    };
    let Some(endpoint) = module.integer_range_endpoint(range) else {
        return false;
    };
    module.valid_integer_range_type(range, endpoint)
        && value_type(function, right) == Some(range)
        && single_value_result(results).is_some_and(|ty| is_boolean(module, ty))
}
