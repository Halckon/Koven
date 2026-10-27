//! unit 实例规划、函数入口与完整 SSA module 组装。

#[cfg(test)]
pub(super) mod nullable_map_guard_support;
#[cfg(test)]
mod pr120_range_source_contract_tests;
#[cfg(test)]
mod pr120_verifier_diagnostics;
#[cfg(test)]
mod registration_tests;
use super::*;

/// 共享 lowering 只读取本轮事实；调用方必须先验证完整分析身份链。
pub(super) fn lower_unit_from_facts(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    constant_owned: Option<&ConstEnabledOwnedUnit>,
    entry: DeclarationId,
) -> Result<(Program, FunctionId), LoweringError> {
    lower_unit_impl(
        (sources, inputs),
        names,
        typed,
        owned,
        constant_owned,
        entry,
        #[cfg(test)]
        None,
    )
}

fn lower_unit_impl(
    source_inputs: (&SourceMap, &[SourceUnitInput<'_>]),
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    constant_owned: Option<&ConstEnabledOwnedUnit>,
    entry: DeclarationId,
    #[cfg(test)] capture: Option<&mut pr120_verifier_diagnostics::Capture>,
) -> Result<(Program, FunctionId), LoweringError> {
    let (sources, inputs) = source_inputs;
    let parsed_by_source = super::super::unit_source_query::parsed_by_source_unit(inputs, names)?;
    // Both sealed views have already checked the complete frontend identity chain.
    // This whole-set capability check must precede instance planning and Module creation.
    super::map_iteration::validate(&parsed_by_source, names, typed, owned)?;
    super::borrow_result::validate(&parsed_by_source, names, typed, owned)?;
    // Runtime global initialization is not lowered yet. Check every source root,
    // including unused declarations, before reachability can discard its initializer.
    for parsed in &parsed_by_source {
        for root in parsed.roots() {
            let node = parsed.ast().items().get(*root).map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
            let (_, item, _) = unwrap_modified(parsed, *root)?;
            if matches!(item, Item::Variable { .. }) {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    node.span(),
                ));
            }
        }
    }
    let instance_plan = plan_unit_instances_from_facts(
        &parsed_by_source,
        names,
        typed,
        owned,
        entry,
        MAX_UNIT_GENERIC_INSTANCES,
    )?;
    let (instances, runtime_type_demands, source_plan) = instance_plan.into_parts();
    let mut program = Program::default();
    let module_id = program.add_module("main");
    let module = program
        .module_mut(module_id)
        .expect("new unit module must exist");
    let mut types = type_lower::UnitTypeLowering::new();
    let mut function_ids = BTreeMap::new();
    let mut plans = Vec::new();
    type_plan::intern_map_result_types(module, &parsed_by_source, &instances, typed, &mut types)?;
    let callable_layouts = closure::layout::declare(
        module,
        &parsed_by_source,
        &instances,
        names,
        typed,
        owned,
        &mut types,
    )?;
    let callable_abi = callable_abi::CallableAbi::declare(
        module,
        &instances,
        &source_plan,
        &callable_layouts,
        typed,
        &mut types,
    )?;

    for (source_ordinal, instance) in instances.into_iter().enumerate() {
        if instance.key().deinit_owner().is_some() {
            let plan = deinit::declare(
                module,
                &parsed_by_source,
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
            (Some(_), None) if callable.range_extension().is_some() => None,
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
        if let Some(binding) = callable.range_extension() {
            let symbol = callable
                .extension_receiver_symbol()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?;
            let concrete = resolve_concrete_type(
                typed,
                binding.receiver_type(),
                instance.substitutions(),
                instance.key().static_self(),
                binding.receiver_span(),
            )?;
            let target = types.intern(module, typed, concrete, binding.receiver_span())?;
            parameter_symbols.push(symbol);
            parameter_types.push(EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            });
        }
        for (slot, parameter) in callable.parameters().iter().enumerate() {
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
                callable_abi.parameter_type(&source_plan, instance.key(), slot, parameter.span())?
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
        let return_types = if builtin_type(typed, return_type) == Some(BuiltinType::Unit) {
            Vec::new()
        } else {
            vec![if matches!(
                typed.types().get(return_type),
                Some(UnitTypeKind::Function { .. })
            ) {
                callable_abi.return_type(&source_plan, instance.source_token(), instance.span())?
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
        module
            .function_mut(id)
            .expect("new unit function must exist")
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

    let entry_id = function_ids
        .get(&UnitFunctionInstanceKey::for_entry(entry))
        .copied()
        .ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;

    for plan in plans {
        let parsed = parsed_by_source[plan.instance.source_unit().index()];
        let references = symbol_references(names, plan.instance.source_unit(), Namespace::Value);
        let type_references =
            symbol_references(names, plan.instance.source_unit(), Namespace::Type);
        // ADR-0032: a function wholly inside the migrated subset uses the refinement layer.
        let refine_entry = {
            let function = &module.functions[plan.id.index()];
            let entry = function
                .entry_block()
                .expect("planned unit function has an entry block");
            let entities = function
                .block(entry)
                .expect("entry block exists")
                .parameters
                .clone();
            let receiver = plan.receiver.as_ref();
            (function.borrow_return.is_none()
                && function.carrier_return.is_none()
                && entities.len() == plan.parameter_symbols.len() + usize::from(receiver.is_some())
                && entities
                    .iter()
                    .all(|entity| matches!(entity, EntityId::Value(_) | EntityId::Loan(_))))
            .then(|| {
                let modes = entities
                    .iter()
                    .skip(usize::from(receiver.is_some()))
                    .map(
                        |entity| match function.entity(*entity).map(|data| data.ty) {
                            Some(EntityType::Loan {
                                kind: LoanKind::Exclusive,
                                ..
                            }) => ParameterMode::Inout,
                            Some(EntityType::Loan { .. }) => ParameterMode::Borrow,
                            _ => ParameterMode::Value,
                        },
                    )
                    .collect::<Vec<_>>();
                (entry, entities, modes)
            })
        };
        if let Some((entry, entities, modes)) = refine_entry {
            let refine_body = match &plan.body {
                FunctionPlanBody::Expression(expression) => {
                    crate::ssa::refine::RefineBody::Expression(*expression)
                }
                FunctionPlanBody::Block(statement) => {
                    crate::ssa::refine::RefineBody::Block(*statement)
                }
            };
            let inputs = crate::ssa::refine::RefineInputs {
                sources,
                parsed,
                source_unit: plan.instance.source_unit(),
                names,
                typed,
                owned,
                constant_owned,
                function_ids: &function_ids,
                source_plan: &source_plan,
                source_token: plan.instance.source_token(),
                references: &references,
                function_item: plan.function_item,
                body: refine_body,
                return_type: plan.return_type,
                receiver: plan.receiver.as_ref().map(|receiver| {
                    crate::ssa::refine::RefineReceiver {
                        owner: receiver.owner,
                        ty: receiver.ty,
                        mode: receiver.mode,
                    }
                }),
                parameters: plan.parameter_symbols.clone(),
                modes,
                span: plan.instance.span(),
                substitutions: plan.instance.substitutions(),
                static_self: plan.instance.key().static_self(),
            };
            if crate::ssa::refine::routes(&inputs) {
                for ty in crate::ssa::refine::type_demands(&inputs) {
                    types.intern(module, typed, ty, plan.instance.span())?;
                }
                let function = module
                    .functions
                    .get_mut(plan.id.index())
                    .expect("planned unit function must exist");
                let refine_types = crate::ssa::refine::RefineTypes {
                    type_ids: types.type_ids(),
                    heap_payloads: types.heap_payloads(),
                    field_indices: types.field_indices(),
                    enum_payloads: types.enum_payloads(),
                };
                crate::ssa::refine::lower(&inputs, function, entry, &entities, &refine_types)?;
                continue;
            }
        }
        let function = module
            .functions
            .get_mut(plan.id.index())
            .expect("planned unit function must exist");
        let block = function
            .entry_block()
            .expect("planned unit function has an entry block");
        let parameters = function
            .block(block)
            .expect("entry block exists")
            .parameters
            .clone();
        if parameters.len() != plan.parameter_symbols.len() + usize::from(plan.receiver.is_some()) {
            return Err(lowering_error(
                LoweringErrorKind::InvalidModel,
                plan.instance.span(),
            ));
        }
        // Inout parameters are lowered only by the refinement layer (SPEC-0358).
        if let Some(parameter) = plan.inout_parameter {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                parameter,
            ));
        }
        let (current_receiver, parameters) = match (plan.receiver, parameters.split_first()) {
            (Some(receiver), Some((entity, parameters)))
                if receiver.entity_type
                    == function
                        .entity(*entity)
                        .map(|data| data.ty)
                        .ok_or_else(|| {
                            lowering_error(LoweringErrorKind::InvalidModel, plan.instance.span())
                        })? =>
            {
                (
                    Some(ReceiverBinding {
                        owner: receiver.owner,
                        mode: receiver.mode,
                        template_ty: receiver.template_ty,
                        ty: receiver.ty,
                        entity: *entity,
                        origin: receiver.origin,
                    }),
                    parameters,
                )
            }
            (None, _) => (None, parameters.as_slice()),
            _ => {
                return Err(lowering_error(
                    LoweringErrorKind::InvalidModel,
                    plan.instance.span(),
                ));
            }
        };
        let mut bindings = BTreeMap::new();
        let mut borrow_bindings = BTreeMap::new();
        let mut entry_boolean_roots = std::collections::BTreeSet::new();
        for (symbol, entity) in plan
            .parameter_symbols
            .into_iter()
            .zip(parameters.iter().copied())
        {
            match entity {
                EntityId::Value(value) => {
                    bindings.insert(symbol, LoweredValue::Value(value));
                }
                EntityId::Loan(loan) => {
                    if matches!(function.entity(entity).map(|data| data.ty),
                        Some(EntityType::Loan { kind: LoanKind::Shared, target })
                            if matches!(module.types.get(target.index()),
                                Some(crate::ssa::model::SsaTypeKind::Boolean)))
                    {
                        entry_boolean_roots.insert(symbol);
                    }
                    borrow_bindings.insert(symbol, loan);
                }
                EntityId::Place(_) => {
                    return Err(lowering_error(
                        LoweringErrorKind::InvalidModel,
                        plan.instance.span(),
                    ));
                }
            }
        }
        let mut lowerer = UnitExpressionLowerer {
            sources,
            parsed,
            source_unit: plan.instance.source_unit(),
            names,
            typed,
            owned,
            constant_owned,
            function_ids: &function_ids,
            source_plan: &source_plan,
            callable_abi: &callable_abi,
            source_token: plan.instance.source_token(),
            type_ids: types.type_ids(),
            heap_payloads: types.heap_payloads(),
            map_results: &module.map_results,
            nullable_maps: &module.nullable_maps,
            ssa_types: &module.types,
            enum_payloads: types.enum_payloads(),
            field_indices: types.field_indices(),
            substitutions: plan.instance.substitutions(),
            static_self: plan.instance.key().static_self(),
            references: &references,
            type_references: &type_references,
            function,
            block,
            bindings,
            borrow_bindings,
            entry_boolean_roots,
            nullable_proofs: BTreeMap::new(),
            map_borrow_guards: BTreeMap::new(),
            result_source_loans: BTreeMap::new(),
            temporary_range_slots: BTreeMap::new(),
            short_range_ends: BTreeMap::new(),
            current_receiver,
            consumed_receiver: None,
            closure_bindings: BTreeMap::new(),
            closure_binding_context: false,
            capture_loans: BTreeMap::new(),
            thunk_expression: None,
            callable_plans: &callable_plans,
            closure_scope: plan.id,
            temporaries: BTreeMap::new(),
            pending_operands: Vec::new(),
            pending_call_frames: Vec::new(),
            loops: Vec::new(),
            map_local_continuation: None,
            return_type: plan.return_type,
        };
        lowerer.emit_drops(UnitDropPoint::FunctionEntry(UnitItemId::new(
            plan.instance.source_unit(),
            plan.function_item,
        )))?;
        if lowerer.function.borrow_return.is_some() {
            let FunctionPlanBody::Expression(expression) = plan.body else {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    plan.instance.span(),
                ));
            };
            lowerer.lower_result_return(expression, plan.instance.span())?;
            continue;
        }
        if lowerer.function.carrier_return.is_some()
            && let FunctionPlanBody::Expression(expression) = plan.body
        {
            lowerer.lower_range_return(expression, plan.instance.span())?;
            continue;
        }
        let (mut result, result_expression) = match plan.body {
            FunctionPlanBody::Expression(expression) => {
                (lowerer.lower(expression)?, Some(expression))
            }
            FunctionPlanBody::Block(statement) => (lowerer.lower_statement(statement)?, None),
        };
        lowerer.finish_local_map_function(plan.instance.span())?;
        if result == LoweredValue::Diverged {
            continue;
        }
        if let (Some(expression), LoweredValue::Value(value)) = (result_expression, result) {
            let (value, transferred) = lowerer.adapt_owned_value_to_expected(
                expression,
                value,
                plan.return_type,
                plan.instance.span(),
            )?;
            if !transferred {
                lowerer.transfer_owned_expression(expression, value, plan.instance.span())?;
            }
            result = LoweredValue::Value(value);
        }
        if let Some(expression) = result_expression {
            lowerer.emit_drops(UnitDropPoint::ControlTransfer(UnitExpressionId::new(
                plan.instance.source_unit(),
                expression,
            )))?;
        }
        let values = match (builtin_type(typed, plan.return_type), result) {
            (Some(BuiltinType::Unit), LoweredValue::Unit) => Vec::new(),
            (Some(BuiltinType::Unit), LoweredValue::Value(_))
            | (_, LoweredValue::Unit | LoweredValue::Diverged) => {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    plan.instance.span(),
                ));
            }
            (_, LoweredValue::Value(value)) => vec![value],
        };
        lowerer
            .function
            .set_terminator(
                lowerer.block,
                TerminatorKind::Return { values },
                Origin::Source(plan.instance.span()),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, plan.instance.span()))?;
    }

    for plan in callable_plans.values() {
        let parsed = parsed_by_source[plan.source_unit.index()];
        let references = symbol_references(names, plan.source_unit, Namespace::Value);
        let type_references = symbol_references(names, plan.source_unit, Namespace::Type);
        let function = module
            .functions
            .get_mut(plan.thunk.index())
            .expect("planned unit closure thunk exists");
        let block = function
            .entry_block()
            .expect("planned unit closure thunk has entry");
        let mut lowerer = UnitExpressionLowerer {
            sources,
            parsed,
            source_unit: plan.source_unit,
            names,
            typed,
            owned,
            constant_owned,
            function_ids: &function_ids,
            source_plan: &source_plan,
            callable_abi: &callable_abi,
            source_token: plan.source_token,
            type_ids: types.type_ids(),
            heap_payloads: types.heap_payloads(),
            map_results: &module.map_results,
            nullable_maps: &module.nullable_maps,
            ssa_types: &module.types,
            enum_payloads: types.enum_payloads(),
            field_indices: types.field_indices(),
            substitutions: &plan.substitutions,
            static_self: plan.static_self,
            references: &references,
            type_references: &type_references,
            function,
            block,
            bindings: BTreeMap::new(),
            borrow_bindings: BTreeMap::new(),
            entry_boolean_roots: std::collections::BTreeSet::new(),
            nullable_proofs: BTreeMap::new(),
            map_borrow_guards: BTreeMap::new(),
            result_source_loans: BTreeMap::new(),
            temporary_range_slots: BTreeMap::new(),
            short_range_ends: BTreeMap::new(),
            current_receiver: None,
            consumed_receiver: None,
            closure_bindings: BTreeMap::new(),
            closure_binding_context: false,
            capture_loans: BTreeMap::new(),
            thunk_expression: None,
            callable_plans: &callable_plans,
            closure_scope: plan.scope,
            temporaries: BTreeMap::new(),
            pending_operands: Vec::new(),
            pending_call_frames: Vec::new(),
            loops: Vec::new(),
            map_local_continuation: None,
            return_type: plan.return_type,
        };
        closure::finish_thunk(&mut lowerer, plan)?;
    }

    let verification = verify_program(&program);
    #[cfg(test)]
    if let Some(capture) = capture {
        capture.observe(
            &program,
            &verification,
            (sources, names, typed, owned),
            &function_ids,
            &types,
            &source_plan,
        );
    }
    verification.map_err(|_| LoweringError {
        kind: LoweringErrorKind::InvalidSsa,
        span: None,
    })?;
    Ok((program, entry_id))
}
