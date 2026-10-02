//! Reachable compilation-unit body 使用的 scalar storage type 预规划。

use lang_frontend::{
    parser::{
        AssignmentOperator, BinaryOperator, Expression, LiteralKind, ParsedFile, PrefixOperator,
        WhenCondition,
    },
    source::Span,
    type_checking::{BuiltinType, CompilationUnitTypes, UnitExpressionId, UnitTypeId},
};

use super::{
    super::{LoweringError, LoweringErrorKind, unit_plan::resolve_concrete_type},
    builtin_type, lowering_error,
    type_lower::{UnitTypeLowering, is_supported_storage_type},
};
use crate::ssa::{model::Module, unit_plan::UnitPlannedInstance};

/// 只为当前 reachable instance body 中实际出现的已支持 storage type 建立 SSA identity。
pub(super) fn intern_body_scalar_types(
    module: &mut Module,
    parsed: &ParsedFile,
    instance: &UnitPlannedInstance,
    typed: &CompilationUnitTypes,
    types: &mut UnitTypeLowering,
) -> Result<(), LoweringError> {
    for (&expression, &ty) in typed.expression_types() {
        if expression.source_unit() != instance.source_unit() {
            continue;
        }
        let span = parsed
            .ast()
            .expressions()
            .get(expression.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?
            .span();
        if !span_contains(instance.span(), span) {
            continue;
        }
        let concrete = resolve_concrete_type(
            typed,
            ty,
            instance.substitutions(),
            instance.key().static_self(),
            span,
        )?;
        if let Some(primitive) = typed.ownership_primitive(expression) {
            let value_type = resolve_concrete_type(
                typed,
                primitive.value_type(),
                instance.substitutions(),
                instance.key().static_self(),
                span,
            )?;
            if builtin_type(typed, value_type) == Some(BuiltinType::Unit) {
                types.intern(module, typed, value_type, span)?;
            }
        }
        if requires_enum_discriminant(parsed, expression)? {
            for builtin in [BuiltinType::Int, BuiltinType::Boolean] {
                let ty = typed
                    .types()
                    .builtin(builtin)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                types.intern(module, typed, ty, span)?;
            }
        }
        if checked_operand_type(parsed, instance, typed, expression, concrete)?
            .and_then(|ty| builtin_type(typed, ty))
            .is_some_and(is_integer_builtin)
        {
            let boolean = typed
                .types()
                .builtin(BuiltinType::Boolean)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            types.intern(module, typed, boolean, span)?;
        }
        if builtin_type(typed, concrete) != Some(BuiltinType::Unit)
            && is_supported_storage_type(typed, concrete)
        {
            types.intern(module, typed, concrete, span)?;
        }
    }
    Ok(())
}

fn requires_enum_discriminant(
    parsed: &ParsedFile,
    expression: UnitExpressionId,
) -> Result<bool, LoweringError> {
    let node = parsed
        .ast()
        .expressions()
        .get(expression.expression())
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    Ok(matches!(
        node.payload(),
        Expression::When { entries, .. }
            if entries.iter().flat_map(|entry| &entry.conditions).any(|condition| {
                matches!(condition, WhenCondition::TypeTest { .. })
            })
    ))
}

const fn is_integer_builtin(builtin: BuiltinType) -> bool {
    matches!(
        builtin,
        BuiltinType::Byte
            | BuiltinType::Short
            | BuiltinType::Int
            | BuiltinType::Long
            | BuiltinType::UByte
            | BuiltinType::UShort
            | BuiltinType::UInt
            | BuiltinType::ULong
    )
}

fn checked_operand_type(
    parsed: &ParsedFile,
    instance: &UnitPlannedInstance,
    typed: &CompilationUnitTypes,
    expression: UnitExpressionId,
    expression_type: UnitTypeId,
) -> Result<Option<UnitTypeId>, LoweringError> {
    let node = parsed
        .ast()
        .expressions()
        .get(expression.expression())
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    let checked = match node.payload() {
        Expression::Binary {
            operator:
                BinaryOperator::Add
                | BinaryOperator::Subtract
                | BinaryOperator::Multiply
                | BinaryOperator::Divide
                | BinaryOperator::Remainder,
            ..
        } => Some(expression_type),
        Expression::Prefix {
            operator: PrefixOperator::Minus,
            operand,
            ..
        } => {
            let operand = parsed
                .ast()
                .expressions()
                .get(*operand)
                .map_err(|_| LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })?;
            (!matches!(
                operand.payload(),
                Expression::Literal(LiteralKind::Integer(_))
            ))
            .then_some(expression_type)
        }
        Expression::Assignment {
            target, operator, ..
        } if *operator != AssignmentOperator::Assign => {
            let target = typed
                .expression_type(UnitExpressionId::new(expression.source_unit(), *target))
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, node.span()))?;
            Some(resolve_concrete_type(
                typed,
                target,
                instance.substitutions(),
                instance.key().static_self(),
                node.span(),
            )?)
        }
        _ => None,
    };
    Ok(checked)
}

fn span_contains(owner: Span, child: Span) -> bool {
    owner.source_id() == child.source_id()
        && owner.start() <= child.start()
        && child.end() <= owner.end()
}

#[cfg(test)]
mod tests {
    use lang_frontend::{
        name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
    };

    use super::*;
    use crate::ssa::{
        model::{Program, SsaTypeKind},
        unit_lower_test_support::{analyze, declaration, parsed},
        unit_plan::plan_unit_instances,
    };

    #[test]
    fn generic_body_type_plan_resolves_the_concrete_instance_type() {
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(
            &mut sources,
            "p/provider.ko",
            "package p\n\
             fun <T> keep(own input: T): Unit {\n\
                 val kept = input\n\
             }",
        );
        let (consumer_source, consumer) = parsed(
            &mut sources,
            "q/consumer.ko",
            "package q\nfun entry(): Unit { val done = p.keep(\"generic\") }",
        );
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
        ];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let instances = plan_unit_instances(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            declaration(&names, "q", "entry"),
        )
        .expect("generic unit instance plan succeeds");
        let keep = instances
            .iter()
            .find(|instance| !instance.key().type_arguments().is_empty())
            .expect("concrete generic keep instance exists");

        let mut program = Program::default();
        let module_id = program.add_module("type-plan-test");
        let module = program
            .module_mut(module_id)
            .expect("new test module exists");
        let mut types = UnitTypeLowering::new();
        intern_body_scalar_types(module, &provider, keep, typed.types(), &mut types)
            .expect("direct T body fact resolves through the String substitution");

        assert_eq!(module.types, vec![SsaTypeKind::StringOwner]);
        assert_eq!(types.type_ids().len(), 1);
    }
}
