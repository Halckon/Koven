//! Reachable compilation-unit body 使用的 scalar storage type 预规划。

use std::collections::BTreeMap;

use lang_frontend::{
    parser::{BinaryOperator, Expression, LiteralKind, ParsedFile, PrefixOperator},
    source::Span,
    type_checking::{BuiltinType, UnitTypeId, ValidatedCompilationUnitTypes},
};

use super::{
    super::{LoweringError, LoweringErrorKind, model::SsaTypeId, unit_plan::resolve_concrete_type},
    builtin_type, intern_scalar_type, is_scalar_storage_builtin, lowering_error,
};
use crate::ssa::{model::Module, unit_plan::UnitPlannedInstance};

/// 只为当前 reachable instance body 中实际出现的已支持 storage type 建立 SSA identity。
pub(super) fn intern_body_scalar_types(
    module: &mut Module,
    parsed: &ParsedFile,
    instance: &UnitPlannedInstance,
    typed: &ValidatedCompilationUnitTypes,
    type_ids: &mut BTreeMap<UnitTypeId, SsaTypeId>,
) -> Result<(), LoweringError> {
    for (&expression, &ty) in typed.types().expression_types() {
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
        let concrete = resolve_concrete_type(typed, ty, instance.substitutions(), span)?;
        if builtin_type(typed, concrete).is_some_and(is_integer_builtin)
            && requires_checked_failure_type(parsed, expression.expression())?
        {
            let boolean = typed
                .types()
                .types()
                .builtin(BuiltinType::Boolean)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            intern_scalar_type(module, typed, type_ids, boolean, span)?;
        }
        if builtin_type(typed, concrete).is_some_and(is_scalar_storage_builtin) {
            intern_scalar_type(module, typed, type_ids, concrete, span)?;
        }
    }
    Ok(())
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

fn requires_checked_failure_type(
    parsed: &ParsedFile,
    expression: lang_frontend::ast::ExpressionId,
) -> Result<bool, LoweringError> {
    let node = parsed
        .ast()
        .expressions()
        .get(expression)
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    let required = match node.payload() {
        Expression::Binary { operator, .. } => matches!(
            operator,
            BinaryOperator::Add
                | BinaryOperator::Subtract
                | BinaryOperator::Multiply
                | BinaryOperator::Divide
                | BinaryOperator::Remainder
        ),
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
            !matches!(
                operand.payload(),
                Expression::Literal(LiteralKind::Integer(_))
            )
        }
        _ => false,
    };
    Ok(required)
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
        let mut type_ids = BTreeMap::new();
        intern_body_scalar_types(module, &provider, keep, &typed, &mut type_ids)
            .expect("direct T body fact resolves through the String substitution");

        assert_eq!(module.types, vec![SsaTypeKind::StringOwner]);
        assert_eq!(type_ids.len(), 1);
    }
}
