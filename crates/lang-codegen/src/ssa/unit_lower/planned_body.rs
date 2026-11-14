//! One ordinary planned body, preserving refine routing and all entry/cleanup gates.
//! This is internal orchestration, not an experimental ownership capability.
use super::*;
use crate::ssa::refine::CallableRoute;
use lang_frontend::ownership_checking::UnitLoweringFacts;

pub(super) struct PlannedBodyContext<'a> {
    pub(super) sources: &'a SourceMap,
    pub(super) parsed_by_source: &'a [&'a ParsedFile],
    pub(super) while_scopes: &'a [crate::ssa::refine::WhileScopes],
    pub(super) names: &'a ValidatedCompilationUnitNames,
    pub(super) typed: &'a CompilationUnitTypes,
    pub(super) owned: &'a dyn UnitLoweringFacts,
    pub(super) constant_owned: Option<&'a ConstEnabledOwnedUnit>,
    pub(super) function_ids: &'a BTreeMap<UnitFunctionInstanceKey, FunctionId>,
    pub(super) callable_returns: &'a BTreeMap<FunctionId, SsaTypeId>,
    pub(super) known_functions: &'a BTreeMap<UnitCallableTarget, (FunctionId, SsaTypeId)>,
    pub(super) source_plan: &'a super::super::unit_plan::UnitCallablePlan,
    pub(super) callable_abi: &'a callable_abi::CallableAbi,
    pub(super) callable_plans: &'a BTreeMap<closure::CallablePlanKey, closure::CallablePlan>,
}
impl PlannedBodyContext<'_> {
    pub(super) fn emit(
        &self,
        module: &mut crate::ssa::model::Module,
        types: &mut type_lower::UnitTypeLowering,
        plan: FunctionPlan,
    ) -> Result<(), LoweringError> {
        let sources = self.sources;
        let parsed_by_source = self.parsed_by_source;
        let while_scopes = self.while_scopes;
        let names = self.names;
        let typed = self.typed;
        let owned = self.owned;
        let constant_owned = self.constant_owned;
        let function_ids = self.function_ids;
        let callable_returns = self.callable_returns;
        let known_functions = self.known_functions;
        let source_plan = self.source_plan;
        let callable_abi = self.callable_abi;
        let callable_plans = self.callable_plans;
        let parsed = parsed_by_source[plan.instance.source_unit().index()];
        let references = symbol_references(names, plan.instance.source_unit(), Namespace::Value);
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
            (entities.len() == plan.parameter_symbols.len() + usize::from(receiver.is_some())
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
                (
                    entry,
                    entities,
                    modes,
                    function.borrow_return,
                    function.carrier_return,
                )
            })
        };
        let refine_callables = closure::refine_callables(callable_plans, plan.id);
        let mut compatibility = false;
        if let Some((entry, entities, modes, borrow_return, carrier_return)) = refine_entry {
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
                while_scopes: &while_scopes[plan.instance.source_unit().index()],
                source_unit: plan.instance.source_unit(),
                names,
                typed,
                owned,
                constant_owned,
                function_ids,
                callable_returns,
                known_functions,
                source_plan,
                source_token: plan.instance.source_token(),
                references: &references,
                function_item: Some(plan.function_item),
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
                callables: &refine_callables,
                views: callable_abi.views(),
                borrow_return,
                carrier_return,
            };
            let route = crate::ssa::refine::route(&inputs);
            compatibility = matches!(route, CallableRoute::Compatibility);
            if matches!(route, CallableRoute::Refine) {
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
                    map_results: &module.map_results,
                    nullable_maps: &module.nullable_maps,
                    ssa_types: &module.types,
                };
                crate::ssa::refine::lower(&inputs, function, entry, &entities, &refine_types)?;
                return Ok(());
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
        // Inout parameters and Inout receivers are lowered only by the refinement layer (SPEC-0358).
        if let Some(parameter) = plan.inout_parameter {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                parameter,
            ));
        }
        if let Some(receiver) = plan
            .receiver
            .filter(|receiver| receiver.mode == ParameterMode::Inout)
        {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                receiver.origin,
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
            migrated_family_compatibility: compatibility,
            sources,
            parsed,
            source_unit: plan.instance.source_unit(),
            names,
            typed,
            owned,
            constant_owned,
            function_ids,
            source_plan,
            callable_abi,
            source_token: plan.instance.source_token(),
            type_ids: types.type_ids(),
            heap_payloads: types.heap_payloads(),
            map_results: &module.map_results,
            nullable_maps: &module.nullable_maps,
            ssa_types: &module.types,
            field_indices: types.field_indices(),
            substitutions: plan.instance.substitutions(),
            static_self: plan.instance.key().static_self(),
            references: &references,
            function,
            block,
            bindings,
            borrow_bindings,
            entry_boolean_roots,
            nullable_proofs: BTreeMap::new(),
            current_receiver,
            consumed_receiver: None,
            closure_binding_context: false,
            callable_plans,
            temporaries: BTreeMap::new(),
            pending_operands: Vec::new(),
            pending_call_frames: Vec::new(),
            loops: Vec::new(),
            return_type: plan.return_type,
        };
        lowerer.emit_drops(UnitDropPoint::FunctionEntry(UnitItemId::new(
            plan.instance.source_unit(),
            plan.function_item,
        )))?;
        // `borrow_return` and `carrier_return` functions lower through `ssa::refine` (SPEC-0393).
        if lowerer.function.borrow_return.is_some() || lowerer.function.carrier_return.is_some() {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                plan.instance.span(),
            ));
        }
        let (mut result, result_expression) = match plan.body {
            FunctionPlanBody::Expression(expression) => {
                (lowerer.lower(expression)?, Some(expression))
            }
            FunctionPlanBody::Block(statement) => (lowerer.lower_statement(statement)?, None),
        };
        if result == LoweredValue::Diverged {
            return Ok(());
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

        Ok(())
    }
}
