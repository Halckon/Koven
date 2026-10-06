//! Reachable compilation-unit body 使用的 scalar storage type 预规划。

use std::collections::BTreeSet;

use lang_frontend::{
    parser::{
        AssignmentOperator, BinaryOperator, Expression, LiteralKind, ParsedFile, PrefixOperator,
        WhenCondition,
    },
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypes, UnitCallTarget, UnitExpressionId, UnitTypeId,
    },
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
    let static_callees = static_call_callees(parsed, instance, typed)?;
    if typed
        .sequential_iterations()
        .iter()
        .any(|plan| plan.statement().source_unit() == instance.source_unit())
    {
        for builtin in [BuiltinType::Int, BuiltinType::Boolean] {
            let ty = typed
                .types()
                .builtin(builtin)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?;
            types.intern(module, typed, ty, instance.span())?;
        }
    }
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
        if static_callees.contains(&expression) {
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
        if typed
            .container_construction(expression)
            .is_some_and(|descriptor| {
                descriptor.kind()
                    == lang_frontend::type_checking::ContainerConstructionKind::RuntimeLength
            })
            || requires_enum_discriminant(parsed, expression)?
        {
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

/// Direct declaration calls lower their selected instance, never the callee's template Function value.
fn static_call_callees(
    parsed: &ParsedFile,
    instance: &UnitPlannedInstance,
    typed: &CompilationUnitTypes,
) -> Result<BTreeSet<UnitExpressionId>, LoweringError> {
    let mut callees = BTreeSet::new();
    for call in typed.calls() {
        let expression = call.expression();
        if expression.source_unit() != instance.source_unit() {
            continue;
        }
        let UnitCallTarget::Declaration(target) = call.target() else {
            continue;
        };
        if typed
            .signatures()
            .declaration(target)
            .and_then(|signature| signature.callable())
            .is_none()
        {
            continue;
        }
        let node = parsed
            .ast()
            .expressions()
            .get(expression.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?;
        if !span_contains(instance.span(), node.span()) {
            continue;
        }
        let Expression::Call { callee, .. } = node.payload() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, node.span()));
        };
        mark_static_callee_chain(parsed, expression, *callee, &mut callees)?;
    }
    Ok(callees)
}

/// Parentheses preserve the descriptor-selected static role; arguments and receivers keep their types.
fn mark_static_callee_chain(
    parsed: &ParsedFile,
    call: UnitExpressionId,
    mut callee: lang_frontend::ast::ExpressionId,
    callees: &mut BTreeSet<UnitExpressionId>,
) -> Result<(), LoweringError> {
    loop {
        let node = parsed
            .ast()
            .expressions()
            .get(callee)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        callees.insert(UnitExpressionId::new(call.source_unit(), callee));
        let Expression::Group { expression } = node.payload() else {
            return Ok(());
        };
        callee = *expression;
    }
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
        name_resolution::{SourceUnitInput, index_compilation_unit},
        source::SourceMap,
        type_checking::{UnitCallableTarget, UnitTypeKind, standard_environments},
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

    #[test]
    fn scalar_type_plan_keeps_static_callees_and_generic_function_values_out_of_storage() {
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(
            &mut sources,
            "p/provider.ko",
            "package p\n\
             fun <T> inner(own input: T): Unit {}\n\
             fun <T> relay(own input: T): Unit { inner(input) }\n\
             fun <T> functionValue(own input: T): Unit {\n\
                 val action: (borrow T) -> Unit = { value -> }\n\
                 action(input)\n\
             }",
        );
        let (consumer_source, consumer) = parsed(
            &mut sources,
            "q/consumer.ko",
            "package q\nfun entry(): Unit { p.relay(\"generic\"); p.functionValue(1) }",
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
        .expect("both source-selected generic instances are planned");
        for (name, direct) in [("relay", true), ("functionValue", false)] {
            let target = UnitCallableTarget::Declaration(declaration(&names, "p", name));
            let instance = instances
                .iter()
                .find(|instance| instance.key().target() == target)
                .unwrap();
            let excluded = static_call_callees(&provider, instance, typed.types()).unwrap();
            assert_eq!(excluded.len(), usize::from(direct));
            if direct {
                let callee = *excluded.first().unwrap();
                assert!(matches!(
                    typed
                        .types()
                        .expression_type(callee)
                        .and_then(|ty| typed.types().types().get(ty)),
                    Some(UnitTypeKind::Function { .. })
                ));
                let concrete = resolve_concrete_type(
                    typed.types(),
                    typed.types().expression_type(callee).unwrap(),
                    instance.substitutions(),
                    None,
                    instance.span(),
                )
                .expect("the frontend has published the source-selected concrete Function type");
                let Some(UnitTypeKind::Function { parameters, .. }) =
                    typed.types().types().get(concrete)
                else {
                    panic!("the concrete static callee still has a Function type");
                };
                assert_eq!(parameters.len(), 1);
                assert_eq!(
                    builtin_type(typed.types(), parameters[0].ty()),
                    Some(BuiltinType::String)
                );
            } else {
                assert!(typed.types().calls().iter().any(|call| {
                    call.target() == UnitCallTarget::FunctionValue
                        && call.expression().source_unit() == instance.source_unit()
                }));
            }
            let mut program = Program::default();
            let module_id = program.add_module("callee-role-test");
            let module = program.module_mut(module_id).unwrap();
            let mut types = UnitTypeLowering::new();
            let result =
                intern_body_scalar_types(module, &provider, instance, typed.types(), &mut types);
            if direct {
                result.expect("direct calls do not materialize their template callee as a value");
                assert_eq!(module.types, vec![SsaTypeKind::StringOwner]);
            } else {
                result.expect("canonical Function queries do not grant scalar storage");
            }
            assert!(
                !module.types.iter().any(|ty| matches!(
                    ty,
                    SsaTypeKind::FunctionPointer { .. } | SsaTypeKind::ConcreteClosure { .. }
                )),
                "concrete callable ABI must come from sealed provenance, not this scalar planner"
            );
        }
    }

    #[test]
    fn static_callee_chain_marks_groups_but_keeps_call_arguments() {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "groups.ko",
            "fun target(value: Int): Unit {}\nfun entry(): Unit { ((target))(1) }",
        );
        let (call_id, call) = file
            .ast()
            .expressions()
            .iter()
            .find(|(_, node)| matches!(node.payload(), Expression::Call { .. }))
            .unwrap();
        let Expression::Call {
            callee, arguments, ..
        } = call.payload()
        else {
            unreachable!();
        };
        let inputs = [SourceUnitInput::new("root", "groups.ko", source, &file)];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let source_unit = index.source_units()[0].id();
        let call_id = UnitExpressionId::new(source_unit, call_id);
        let mut excluded = BTreeSet::new();
        // This private AST-chain test starts after the descriptor's static-role gate.
        mark_static_callee_chain(&file, call_id, *callee, &mut excluded).unwrap();
        assert_eq!(excluded.len(), 3, "two groups and the selected name");
        assert!(!excluded.contains(&call_id));
        assert!(!excluded.contains(&UnitExpressionId::new(source_unit, arguments[0].value)));
    }
}
