//! Floating operations and typed comparisons.
use super::*;
use crate::ssa::model::FloatOperator;

pub(super) fn float_contract(
    module: &Module,
    function: &Function,
    operator: FloatOperator,
    left: ValueId,
    right: Option<ValueId>,
    results: &[EntityType],
) -> bool {
    let Some(ty) = value_type(function, left) else {
        return false;
    };
    matches!(
        module.type_kind(ty),
        Some(SsaTypeKind::Float32 | SsaTypeKind::Float64)
    ) && single_value_result(results) == Some(ty)
        && match (operator, right) {
            (FloatOperator::Negate, None) => true,
            (FloatOperator::Negate, Some(_)) | (_, None) => false,
            (_, Some(right)) => value_type(function, right) == Some(ty),
        }
}

pub(super) fn comparison_contract(
    module: &Module,
    function: &Function,
    operator: ComparisonOperator,
    left: ValueId,
    right: ValueId,
    results: &[EntityType],
) -> bool {
    let left = value_type(function, left);
    if left != value_type(function, right)
        || !single_value_result(results).is_some_and(|ty| is_boolean(module, ty))
    {
        return false;
    }
    match operator {
        ComparisonOperator::Equal | ComparisonOperator::NotEqual => {
            is_equality_type(module, left)
                || matches!(
                    left.and_then(|ty| module.type_kind(ty)),
                    Some(SsaTypeKind::Float32 | SsaTypeKind::Float64)
                )
                || left.is_some_and(|ty| module.is_null_only(ty))
                || left
                    .and_then(|ty| module.inline_nullable(ty))
                    .is_some_and(|(inner, niche)| {
                        !niche
                            && matches!(
                                module.type_kind(inner),
                                Some(SsaTypeKind::Float32 | SsaTypeKind::Float64)
                            )
                    })
        }
        ComparisonOperator::LessThan
        | ComparisonOperator::LessThanOrEqual
        | ComparisonOperator::GreaterThan
        | ComparisonOperator::GreaterThanOrEqual => matches!(
            left.and_then(|ty| module.type_kind(ty)),
            Some(
                SsaTypeKind::Integer { .. }
                    | SsaTypeKind::Char
                    | SsaTypeKind::Float32
                    | SsaTypeKind::Float64
            )
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssa::model::{Origin, Program};
    use lang_frontend::source::SourceMap;

    #[test]
    fn float_scalar_contracts_distinguish_width_arity_and_map_payloads() {
        let mut sources = SourceMap::new();
        let source = sources.add_source("float.ko", "value").unwrap();
        let origin = Origin::Source(sources.span(source, 0, 5).unwrap());
        let mut program = Program::default();
        let id = program.add_module("float");
        let module = program.module_mut(id).unwrap();
        let f32 = module.intern_type(SsaTypeKind::Float32);
        let f64 = module.intern_type(SsaTypeKind::Float64);
        let boolean = module.intern_type(SsaTypeKind::Boolean);
        let lookup = module.add_map_result_type(f32).unwrap();
        let nullable_f32 = module.add_inline_nullable_type(f32).unwrap();
        let nullable_f64 = module.add_inline_nullable_type(f64).unwrap();
        let id = module
            .add_function("probe", Vec::new(), origin.clone())
            .unwrap();
        let function = module.function_mut(id).unwrap();
        let entry = function
            .add_block(
                [f32, f64, boolean, lookup, nullable_f32, nullable_f64]
                    .into_iter()
                    .map(EntityType::Value)
                    .collect(),
                origin,
            )
            .unwrap();
        let function = module.function(id).unwrap();
        let [
            EntityId::Value(left),
            EntityId::Value(wide),
            EntityId::Value(flag),
            EntityId::Value(query),
            EntityId::Value(nullable_left),
            EntityId::Value(nullable_wide),
        ] = function.block(entry).unwrap().parameters[..]
        else {
            panic!("scalar and lookup parameters");
        };
        let check = |operator, operand, right, result| {
            float_contract(
                module,
                function,
                operator,
                operand,
                right,
                &[EntityType::Value(result)],
            )
        };
        assert!(check(FloatOperator::Add, left, Some(left), f32));
        assert!(check(FloatOperator::Negate, left, None, f32));
        assert!(!check(FloatOperator::Add, left, Some(wide), f32));
        assert!(!check(FloatOperator::Add, left, Some(left), f64));
        assert!(!check(FloatOperator::Add, left, None, f32));
        assert!(!check(FloatOperator::Negate, left, Some(left), f32));
        assert!(!check(FloatOperator::Negate, flag, None, boolean));
        for results in [
            &[][..],
            &[EntityType::Value(f32), EntityType::Value(f32)][..],
        ] {
            assert!(!float_contract(
                module,
                function,
                FloatOperator::Negate,
                left,
                None,
                results
            ));
        }

        let equal = |a, b| {
            comparison_contract(
                module,
                function,
                ComparisonOperator::Equal,
                a,
                b,
                &[EntityType::Value(boolean)],
            )
        };
        assert!(equal(left, left));
        assert!(!equal(left, wide));
        assert!(!equal(query, query));
        assert!(equal(nullable_left, nullable_left));
        assert!(equal(nullable_wide, nullable_wide));
        assert!(!equal(nullable_left, nullable_wide));
        assert!(!equal(nullable_left, left));
        assert!(!comparison_contract(
            module,
            function,
            ComparisonOperator::LessThan,
            nullable_left,
            nullable_left,
            &[EntityType::Value(boolean)],
        ));
    }
}
