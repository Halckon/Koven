//! Scalar constant result contracts.
use super::*;
pub(super) fn constant_contract(
    module: &Module,
    constant: &ScalarConstant,
    results: &[EntityType],
) -> bool {
    let Some(result) = single_value_result(results) else {
        return false;
    };
    match (constant, module.type_kind(result)) {
        #[cfg(test)]
        (ScalarConstant::SyntheticZero, Some(SsaTypeKind::ZeroSized { .. })) => true,
        (ScalarConstant::Unit, Some(SsaTypeKind::Unit))
        | (ScalarConstant::Boolean(_), Some(SsaTypeKind::Boolean)) => true,
        (ScalarConstant::Char(value), Some(SsaTypeKind::Char)) => char::from_u32(*value).is_some(),
        (ScalarConstant::Integer(value), Some(SsaTypeKind::Integer { bits, signed })) => {
            integer_fits(*value, *bits, *signed)
        }
        _ => false,
    }
}
