//! Signature-first shared declaration/layout stage; no body or publication authority.
use super::*;
use crate::ssa::unit_plan::UnitInstancePlan;

pub(super) struct DeclaredUnit {
    pub(super) types: type_lower::UnitTypeLowering,
    pub(super) function_ids: BTreeMap<UnitFunctionInstanceKey, FunctionId>,
    pub(super) callable_returns: BTreeMap<FunctionId, SsaTypeId>,
    pub(super) plans: Vec<FunctionPlan>,
    pub(super) source_plan: crate::ssa::unit_plan::UnitCallablePlan,
    pub(super) callable_abi: callable_abi::CallableAbi,
    pub(super) callable_plans: BTreeMap<closure::CallablePlanKey, closure::CallablePlan>,
}
pub(super) fn declare(
    module: &mut crate::ssa::model::Module,
    parsed_by_source: &[&ParsedFile],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &dyn UnitLoweringFacts,
    instance_plan: UnitInstancePlan,
) -> Result<DeclaredUnit, LoweringError> {
    let (instances, runtime_type_demands, source_plan) = instance_plan.into_parts();
    let mut types = type_lower::UnitTypeLowering::new(names);
    let mut function_ids = BTreeMap::new();
    let mut callable_returns = BTreeMap::new();
    let mut plans = Vec::new();
    type_plan::intern_map_result_types(module, parsed_by_source, &instances, typed, &mut types)?;
    let callable_layouts = closure::layout::declare(
        module,
        parsed_by_source,
        &instances,
        names,
        typed,
        owned,
        &mut types,
    )?;
    let mut callable_abi = callable_abi::CallableAbi::declare(
        module,
        &instances,
        &source_plan,
        &callable_layouts,
        typed,
        &mut types,
    )?;

    for (ty, span) in source_plan.prefix_views() {
        let view = callable_abi::view_type(module, typed, &mut types, ty, span)?;
        callable_abi.record_view(ty, view);
    }
    for (source_ordinal, instance) in instances.into_iter().enumerate() {
        if instance.key().deinit_owner().is_some() {
            let plan = callable_abi::declare_deinit(
                module,
                parsed_by_source,
                names,
                typed,
                &mut types,
                instance,
            )?;
            function_ids.insert(plan.instance.key().clone(), plan.id);
            plans.push(plan);
            continue;
        }
        let callable = unit_callable_signature(typed, instance.key().target())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?;
        let parsed = parsed_by_source
            .get(instance.source_unit().index())
            .copied()
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let (function_item, item, _) = unwrap_modified(parsed, instance.item())?;
        let Item::Function { form, .. } = item else {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                instance.span(),
            ));
        };
        let body = match form {
            FunctionForm::Explicit {
                body: FunctionBody::Expression { expression, .. },
                ..
            } => FunctionPlanBody::Expression(expression),
            FunctionForm::ImplicitUnitBlock(block)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(block),
                ..
            } => FunctionPlanBody::Block(block),
            FunctionForm::ImplicitUnitAbsent
            | FunctionForm::Explicit {
                body: FunctionBody::Absent,
                ..
            } => {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    instance.span(),
                ));
            }
        };
        let receiver = match (callable.receiver(), instance.owner()) {
            (Some(receiver), Some(owner)) => {
                let concrete = resolve_concrete_type(
                    typed,
                    receiver.ty(),
                    instance.substitutions(),
                    instance.key().static_self(),
                    receiver.declaration_span(),
                )?;
                let ty = types.intern(module, typed, concrete, receiver.declaration_span())?;
                let entity_type = match receiver.mode() {
                    ParameterMode::Value => EntityType::Value(ty),
                    ParameterMode::Borrow => EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: ty,
                    },
                    ParameterMode::Inout => EntityType::Loan {
                        kind: LoanKind::Exclusive,
                        target: ty,
                    },
                };
                Some(ReceiverPlan {
                    owner,
                    mode: receiver.mode(),
                    template_ty: receiver.ty(),
                    ty: concrete,
                    entity_type,
                    origin: receiver.declaration_span(),
                })
            }
            (None, None) => None,
            (Some(_), None)
                if callable.range_extension().is_some() || callable.text_extension().is_some() =>
            {
                None
            }
            (Some(receiver), None) => {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    receiver.declaration_span(),
                ));
            }
            (None, Some(_)) => {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    instance.span(),
                ));
            }
        };
        let mut parameter_symbols = Vec::with_capacity(callable.parameters().len());
        let mut inout_parameter = None;
        let mut parameter_types =
            Vec::with_capacity(callable.parameters().len() + usize::from(receiver.is_some()));
        if let Some(receiver) = receiver {
            parameter_types.push(receiver.entity_type);
        }
        let extension = callable
            .range_extension()
            .map(|binding| (binding.receiver_type(), binding.receiver_span()))
            .or_else(|| {
                callable
                    .text_extension()
                    .map(|binding| (binding.receiver_type(), binding.receiver_span()))
            });
        if let Some((receiver_type, receiver_span)) = extension {
            let symbol = callable
                .extension_receiver_symbol()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?;
            let concrete = resolve_concrete_type(
                typed,
                receiver_type,
                instance.substitutions(),
                instance.key().static_self(),
                receiver_span,
            )?;
            let target = types.intern(module, typed, concrete, receiver_span)?;
            parameter_symbols.push(symbol);
            parameter_types.push(EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            });
        }
        for parameter in callable.parameters() {
            let symbol = parameter
                .symbol()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, parameter.span()))?;
            let concrete = resolve_concrete_type(
                typed,
                parameter.ty(),
                instance.substitutions(),
                instance.key().static_self(),
                parameter.span(),
            )?;
            if parameter.mode() == ParameterMode::Borrow
                && builtin_type(typed, concrete) == Some(BuiltinType::Unit)
            {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    parameter.span(),
                ));
            }
            let ty = if matches!(
                typed.types().get(concrete),
                Some(UnitTypeKind::Function { .. })
            ) {
                {
                    let view = callable_abi::view_type(
                        module,
                        typed,
                        &mut types,
                        concrete,
                        parameter.span(),
                    )?;
                    callable_abi.record_view(concrete, view);
                    view
                }
            } else {
                types.intern(module, typed, concrete, parameter.span())?
            };
            parameter_symbols.push(symbol);
            if parameter.mode() == ParameterMode::Inout {
                inout_parameter.get_or_insert(parameter.span());
            }
            parameter_types.push(match parameter.mode() {
                ParameterMode::Value => EntityType::Value(ty),
                ParameterMode::Borrow => EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                },
                // Only the refinement layer lowers Inout parameters (SPEC-0358); a body left on
                // the per-family path is rejected below.
                ParameterMode::Inout => EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target: ty,
                },
            });
        }
        let return_type = resolve_concrete_type(
            typed,
            callable.return_type(),
            instance.substitutions(),
            instance.key().static_self(),
            instance.span(),
        )?;
        let mut callable_return = None;
        let result_builtin = builtin_type(typed, return_type);
        let return_types = if matches!(
            result_builtin,
            Some(BuiltinType::Unit | BuiltinType::Nothing)
        ) {
            Vec::new()
        } else {
            vec![if matches!(
                typed.types().get(return_type),
                Some(UnitTypeKind::Function { .. })
            ) {
                let returned = callable_abi.return_type(
                    &source_plan,
                    instance.source_token(),
                    instance.span(),
                )?;
                callable_return = Some(returned);
                returned
            } else {
                types.intern(module, typed, return_type, instance.span())?
            }]
        };
        let mut name = instance_function_name(names, &instance)?;
        if !instance.key().callable_arguments().is_empty() {
            name.push_str(&format!(".callable{source_ordinal}"));
        }
        let origin = Origin::Source(instance.span());
        let id = match receiver {
            Some(receiver) => {
                module.add_instance_function(name, receiver.entity_type, return_types, origin)
            }
            None => module.add_function(name, return_types, origin),
        }
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, instance.span()))?;
        if let Some(returned) = callable_return {
            callable_returns.insert(id, returned);
        }
        let function = module
            .function_mut(id)
            .expect("new unit function must exist");
        function.never_returns = result_builtin == Some(BuiltinType::Nothing);
        function
            .add_block(parameter_types, Origin::Source(instance.span()))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, instance.span()))?;
        if let Some(contract) = callable.borrow_return() {
            module
                .function_mut(id)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?
                .borrow_return = Some(crate::ssa::borrow_result_support::source_index(
                contract.origin(),
                instance.span(),
            )?);
        }
        if let lang_frontend::type_checking::CallableResultSource::Carrier(contract) =
            callable.result_source()
        {
            let index = match contract.origin() {
                lang_frontend::type_checking::BorrowReturnOrigin::Parameter(index) => index,
                lang_frontend::type_checking::BorrowReturnOrigin::Receiver
                    if callable.range_extension().is_some() =>
                {
                    0
                }
                _ => {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        instance.span(),
                    ));
                }
            };
            if !owned
                .borrow_results()
                .range_return_origins()
                .iter()
                .any(|fact| fact.declaration_span() == contract.marker_span())
            {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    instance.span(),
                ));
            }
            module
                .function_mut(id)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?
                .carrier_return = Some(index);
        }
        function_ids.insert(instance.key().clone(), id);
        plans.push(FunctionPlan {
            id,
            instance,
            function_item,
            body,
            receiver,
            parameter_symbols,
            inout_parameter,
            return_type,
        });
    }

    // 保持既有 signature-first 类型编号；body-only type 只在全部函数签名建立后追加。
    for plan in &plans {
        type_plan::intern_body_scalar_types(
            module,
            parsed_by_source[plan.instance.source_unit().index()],
            &plan.instance,
            typed,
            &mut types,
        )?;
        type_plan::intern_formed_nullable_map_types(
            module,
            parsed_by_source[plan.instance.source_unit().index()],
            plan,
            names,
            typed,
            owned,
            &mut types,
        )?;
    }
    let callable_plans = closure::declare(module, &plans, callable_layouts)?;
    for plan in callable_plans.values() {
        type_plan::intern_lambda_formed_nullable_map_types(
            module,
            parsed_by_source[plan.source_unit.index()],
            plan,
            names,
            typed,
            owned,
            &mut types,
        )?;
    }
    for (ty, demand) in runtime_type_demands {
        let materialized = types.type_ids().contains_key(&ty);
        match demand {
            UnitRuntimeTypeDemand::InstanceKeyOnly if materialized => {
                return Err(LoweringError {
                    kind: LoweringErrorKind::InvalidModel,
                    span: None,
                });
            }
            UnitRuntimeTypeDemand::RuntimeLayoutRequired if !materialized => {
                return Err(LoweringError {
                    kind: LoweringErrorKind::InvalidModel,
                    span: None,
                });
            }
            UnitRuntimeTypeDemand::InstanceKeyOnly
            | UnitRuntimeTypeDemand::RuntimeLayoutRequired => {}
        }
    }

    Ok(DeclaredUnit {
        types,
        function_ids,
        callable_returns,
        plans,
        source_plan,
        callable_abi,
        callable_plans,
    })
}
