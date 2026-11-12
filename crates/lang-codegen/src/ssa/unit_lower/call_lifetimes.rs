//! 未提交的同步调用前缀；源码 owner 由 frontend drop facts 决定。
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingCallFrame {
    pub(super) call: UnitExpressionId,
    pub(super) receiver: bool,
    pub(super) loan_arguments: Vec<(UnitExpressionId, Vec<usize>)>,
    pub(super) loop_depth: usize,
    pub(super) pending_start: usize,
    pub(super) created_loans: Vec<usize>,
    /// Argument temporaries whose ordinary direct-call prefix was validated.
    pub(super) control_prefix_temporaries: Vec<UnitExpressionId>,
}

impl UnitExpressionLowerer<'_> {
    /// Iteration facts authorize the same CFG-safe call prefixes through both unit views.
    /// Span containment selects the capability slice, never an ownership or cleanup action.
    pub(super) fn supports_control_prefix(&self, span: Span) -> bool {
        self.constant_owned.is_some()
            || self.owned.borrow_results().range_uses().iter().any(|fact| {
                matches!(fact.site(), lang_frontend::ownership_checking::RangeUseSite::Call(call)
                    if call.source_unit() == self.source_unit
                        && self.parsed.ast().expressions().get(call.expression()).is_ok_and(|node| node.span() == span))
            })
            || self.typed.sequential_iterations().iter().any(|descriptor| {
                descriptor.statement().source_unit() == self.source_unit
                    && self
                        .statement_span(descriptor.statement().statement())
                        .is_ok_and(|iteration| {
                            (iteration.start() <= span.start() && span.end() <= iteration.end())
                                || (span.start() <= iteration.start()
                                    && iteration.end() <= span.end())
                        })
            })
    }

    pub(super) fn supports_loop_control_prefix(&self, span: Span) -> bool {
        self.supports_control_prefix(span)
            || self.temporaries.values().all(|owner| {
                // A Group may record multiple source IDs for the same carried owner.
                self.pending_call_frames.iter().any(|frame| {
                    frame
                        .control_prefix_temporaries
                        .iter()
                        .any(|argument| self.temporaries.get(argument) == Some(owner))
                })
            })
    }

    /// 声明调用复用已封闭的 root/temporary 实参前缀事务；可信文本的 Borrow receiver
    /// 另复用既有 receiver 帧。不授予普通 receiver、字段实参或借用结果新能力。
    pub(super) fn supports_direct_control_prefix(
        &self,
        call: UnitExpressionId,
        arguments: &[lang_frontend::parser::CallArgument],
        descriptor: &lang_frontend::type_checking::UnitCallDescriptor,
        span: Span,
    ) -> Result<bool, LoweringError> {
        use lang_frontend::ownership_checking::{
            LoanKind as FrontendLoanKind, UnitCallArgumentOwnershipKind, UnitLoanTarget,
            UnitValueDeliveryKind, UnitValueDeliverySource,
        };
        use lang_frontend::type_checking::{
            CallableResultSource, UnitCallReceiverOrigin, UnitCallTarget,
        };

        let UnitCallTarget::Declaration(target) = descriptor.target() else {
            return Ok(false);
        };
        let supported_receiver = descriptor.receiver().is_none_or(|receiver| {
            receiver.mode() == ParameterMode::Borrow
                && matches!(receiver.origin(), UnitCallReceiverOrigin::Expression(_))
                && builtin_type(self.typed, receiver.ty()) == Some(BuiltinType::String)
                && self
                    .typed
                    .signatures()
                    .declaration(target)
                    .and_then(|declaration| declaration.callable())
                    .and_then(|callable| callable.text_extension())
                    .is_some_and(|binding| {
                        binding.callable() == UnitCallableTarget::Declaration(target)
                            && binding.receiver_type() == receiver.ty()
                    })
        });
        if !supported_receiver
            || descriptor.result_source() != CallableResultSource::Owned
            || descriptor.aborts()
            || descriptor.prints_line()
        {
            return Ok(false);
        }
        if call.source_unit() != self.source_unit
            || descriptor.expression() != call
            || self.typed.call(call) != Some(descriptor)
            || arguments.len() != descriptor.arguments().len()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let route = self
            .source_plan
            .call_site(self.source_token, call)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if route.key().target() != UnitCallableTarget::Declaration(target)
            || route
                .dispatch_key()
                .is_some_and(|key| !self.function_ids.contains_key(key))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if !route.delegation().is_empty() || !route.key().callable_arguments().is_empty() {
            return Ok(false);
        }
        let contracts = self
            .owned
            .call_argument_contracts_for(call)
            .collect::<Vec<_>>();
        if contracts.len() != arguments.len() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let mut seen_parameters = vec![false; arguments.len()];
        for (index, argument) in arguments.iter().enumerate() {
            let mut mappings = descriptor
                .arguments()
                .iter()
                .filter(|mapping| mapping.argument_index() == index);
            let mapping = mappings
                .next()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
            let Some(seen) = seen_parameters.get_mut(mapping.parameter_index()) else {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            };
            if mappings.next().is_some() || std::mem::replace(seen, true) {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            }
            let argument_id = UnitExpressionId::new(self.source_unit, argument.value);
            let argument_span = self
                .parsed
                .ast()
                .expressions()
                .get(argument.value)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, argument.span))?
                .span();
            let mut matches = contracts
                .iter()
                .filter(|contract| contract.argument() == argument_id);
            let contract = matches
                .next()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
            let expected_kind = match mapping.mode() {
                ParameterMode::Value => UnitCallArgumentOwnershipKind::Value,
                ParameterMode::Borrow => UnitCallArgumentOwnershipKind::SharedLoan,
                ParameterMode::Inout => UnitCallArgumentOwnershipKind::ExclusiveLoan,
            };
            if matches.next().is_some()
                || contract.parameter_index() != mapping.parameter_index()
                || contract.parameter_type() != mapping.parameter_type()
                || contract.category() != mapping.category()
                || contract.kind() != expected_kind
                || contract.crosses_thread() != mapping.crosses_thread()
                || contract.argument_span() != argument_span
                || contract.call_span() != span
                || self.typed.expression_category(argument_id) != Some(mapping.category())
            {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            }
            if mapping.mode() == ParameterMode::Inout || contract.crosses_thread() {
                return Ok(false);
            }
            let mut loans = self
                .owned
                .loans()
                .iter()
                .filter(|fact| fact.call() == call && fact.argument() == argument_id);
            let loan = loans.next();
            let mut deliveries = self
                .owned
                .value_deliveries()
                .iter()
                .filter(|fact| fact.call() == call && fact.argument() == argument_id);
            let delivery = deliveries.next();
            if loans.next().is_some() || deliveries.next().is_some() {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            }
            // Nothing/不可达后缀没有成功交付事实；实际完成的实参仍由原 consumer 严格核验。
            match mapping.mode() {
                ParameterMode::Value => {
                    if loan.is_some() {
                        return Err(lowering_error(
                            LoweringErrorKind::MissingFact,
                            argument.span,
                        ));
                    }
                    if let Some(delivery) = delivery {
                        if delivery.span() != argument_span
                            || delivery.parameter_span() != contract.parameter_span()
                        {
                            return Err(lowering_error(
                                LoweringErrorKind::MissingFact,
                                argument.span,
                            ));
                        }
                        let root_or_temporary = match (delivery.kind(), delivery.source()) {
                            (
                                UnitValueDeliveryKind::Copy | UnitValueDeliveryKind::Move,
                                UnitValueDeliverySource::Place(place),
                            ) => {
                                self.direct_control_prefix_root(argument_id, place, argument_span)?
                            }
                            (
                                UnitValueDeliveryKind::Temporary,
                                UnitValueDeliverySource::Temporary(origin),
                            ) => *origin == argument_id,
                            _ => false,
                        };
                        if !root_or_temporary {
                            return Ok(false);
                        }
                    }
                }
                ParameterMode::Borrow => {
                    if delivery.is_some() {
                        return Err(lowering_error(
                            LoweringErrorKind::MissingFact,
                            argument.span,
                        ));
                    }
                    if let Some(loan) = loan {
                        if loan.kind() != FrontendLoanKind::Shared
                            || loan.begin_span() != contract.loan_begin_span()
                            || loan.end_span() != span
                            || loan.parameter_span() != contract.parameter_span()
                        {
                            return Err(lowering_error(
                                LoweringErrorKind::MissingFact,
                                argument.span,
                            ));
                        }
                        let root_or_temporary = match loan.target() {
                            UnitLoanTarget::Place(place) => {
                                self.direct_control_prefix_root(argument_id, place, argument_span)?
                            }
                            UnitLoanTarget::Temporary(origin) => {
                                if *origin != argument_id {
                                    false
                                } else if mapping.category() == ExpressionCategory::Temporary {
                                    true
                                } else {
                                    let parameter = resolve_concrete_type(
                                        self.typed,
                                        mapping.parameter_type(),
                                        self.substitutions,
                                        self.static_self,
                                        argument_span,
                                    )?;
                                    let target =
                                        self.type_ids.get(&parameter).copied().ok_or_else(
                                            || {
                                                lowering_error(
                                                    LoweringErrorKind::MissingFact,
                                                    argument_span,
                                                )
                                            },
                                        )?;
                                    self.is_copyable_inline_nullable_borrow(
                                        argument_id,
                                        self.expression_ssa_type(argument.value, argument_span)?,
                                        target,
                                        argument_span,
                                    )?
                                }
                            }
                            UnitLoanTarget::This(_) => false,
                        };
                        if !root_or_temporary {
                            return Ok(false);
                        }
                    }
                }
                ParameterMode::Inout => return Ok(false),
            }
        }
        Ok(true)
    }

    fn direct_control_prefix_root(
        &self,
        argument: UnitExpressionId,
        place: &lang_frontend::ownership_checking::UnitOwnershipPlace,
        span: Span,
    ) -> Result<bool, LoweringError> {
        if !place.is_root() || place.root().source_unit() != self.source_unit {
            return Ok(false);
        }
        let mut expression = argument.expression();
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression: inner } => expression = *inner,
                Expression::Name => break,
                _ => return Ok(false),
            }
        }
        if self.direct_name_symbol(expression, span)? != Some(place.root()) {
            return Ok(false);
        }
        let symbol = self
            .names
            .names()
            .source_units()
            .get(place.root().source_unit().index())
            .and_then(|source| {
                source
                    .resolution()
                    .symbols()
                    .get(place.root().symbol().index())
            })
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        Ok(matches!(
            symbol.kind(),
            SymbolKind::Variable | SymbolKind::ValueParameter
        ) && !symbol.is_synthetic())
    }

    /// 槽位在 CFG 重绑定后仍稳定；不可保存旧 block 的 LoanId。
    pub(super) fn end_pending_call_loans(
        &mut self,
        minimum_loop_depth: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        let mut retained = self.pending_operands.len();
        for frame_index in (0..self.pending_call_frames.len()).rev() {
            let frame = &self.pending_call_frames[frame_index];
            if frame.loop_depth < minimum_loop_depth {
                continue;
            }
            retained = retained.min(frame.pending_start);
            let ending_slots = frame
                .created_loans
                .iter()
                .rev()
                .copied()
                .collect::<Vec<_>>();
            for index in ending_slots {
                let Some(EntityId::Loan(loan)) = self.pending_operands.get(index).copied() else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                };
                self.function
                    .append_instruction(
                        self.block,
                        Operation::BorrowEnd { loan },
                        Vec::new(),
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            }
        }
        self.pending_operands.truncate(retained);
        Ok(())
    }
}

pub(in crate::ssa::unit_lower) mod callable_abi {
    //! Concrete callable types are queried by sealed source provenance, never semantic Fn TypeId.

    use super::super::*;
    use crate::ssa::lowering_support::callable_instances::{
        CallableKey, CallableToken, SourceToken,
    };
    use lang_frontend::ownership_checking::UnitCallableOrigin;

    pub(in crate::ssa::unit_lower) struct CallableAbi {
        lambdas: BTreeMap<(SourceToken, UnitExpressionId), SsaTypeId>,
        named: BTreeMap<SourceToken, SsaTypeId>,
        sources: BTreeMap<UnitFunctionInstanceKey, SourceToken>,
        /// ADR-0034 view targets of Borrow function parameters, by concrete function type.
        views: BTreeMap<UnitTypeId, SsaTypeId>,
    }

    impl CallableAbi {
        pub(in crate::ssa::unit_lower) fn declare(
            module: &mut crate::ssa::model::Module,
            instances: &[UnitPlannedInstance],
            plan: &crate::ssa::unit_plan::UnitCallablePlan,
            layouts: &closure::CallableLayouts,
            typed: &CompilationUnitTypes,
            types: &mut type_lower::UnitTypeLowering,
        ) -> Result<Self, LoweringError> {
            let sources = instances
                .iter()
                .map(|instance| (instance.key().clone(), instance.source_token()))
                .collect();
            let mut named_sources = std::collections::BTreeSet::new();
            for instance in instances {
                let runtime_tokens = typed
                    .container_constructions()
                    .iter()
                    .map(|d| d.expression())
                    .chain(typed.map_with_values().iter().map(|d| d.expression()))
                    .filter_map(|call| plan.runtime_callback(instance.source_token(), call));
                for token in instance
                    .key()
                    .callable_arguments()
                    .iter()
                    .map(|(_, token)| *token)
                    .chain(runtime_tokens)
                    .chain(
                        plan.callable_return(instance.source_token())
                            .map(|value| value.callable()),
                    )
                    .chain(plan.borrowed_arguments(instance.source_token()))
                {
                    if let Some(CallableKey::KnownFunction { function }) = plan.callable(token) {
                        named_sources.insert(*function);
                    }
                }
            }
            let mut named = BTreeMap::new();
            for source in named_sources {
                let key = plan.source(source).ok_or(LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })?;
                let instance = instances
                    .iter()
                    .find(|instance| instance.key() == key)
                    .ok_or(LoweringError {
                        kind: LoweringErrorKind::MissingFact,
                        span: None,
                    })?;
                let span = instance.span();
                let signature = unit_callable_signature(typed, key.target())
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let mut parameters = Vec::new();
                for parameter in signature.parameters() {
                    let ty = resolve_concrete_type(
                        typed,
                        parameter.ty(),
                        instance.substitutions(),
                        key.static_self(),
                        parameter.span(),
                    )?;
                    let ty = types.intern(module, typed, ty, parameter.span())?;
                    parameters.push(match parameter.mode() {
                        ParameterMode::Value => EntityType::Value(ty),
                        ParameterMode::Borrow => EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: ty,
                        },
                        ParameterMode::Inout => {
                            return Err(lowering_error(
                                LoweringErrorKind::UnsupportedNode,
                                parameter.span(),
                            ));
                        }
                    });
                }
                let result = resolve_concrete_type(
                    typed,
                    signature.return_type(),
                    instance.substitutions(),
                    key.static_self(),
                    span,
                )?;
                let result_builtin = builtin_type(typed, result);
                let returns = if matches!(
                    result_builtin,
                    Some(BuiltinType::Unit | BuiltinType::Nothing)
                ) {
                    Vec::new()
                } else {
                    vec![types.intern(module, typed, result, span)?]
                };
                let callable = module
                    .add_function_pointer_signature(crate::ssa::model::CallableSignature {
                        parameters,
                        returns,
                        never_returns: result_builtin == Some(BuiltinType::Nothing),
                    })
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                named.insert(source, callable);
            }
            Ok(Self {
                lambdas: layouts
                    .iter()
                    .map(|(key, layout)| (*key, layout.callable))
                    .collect(),
                named,
                sources,
                views: BTreeMap::new(),
            })
        }

        pub(in crate::ssa::unit_lower) fn record_view(
            &mut self,
            function: UnitTypeId,
            view: SsaTypeId,
        ) {
            self.views.insert(function, view);
        }

        pub(in crate::ssa::unit_lower) fn views(&self) -> &BTreeMap<UnitTypeId, SsaTypeId> {
            &self.views
        }

        pub(in crate::ssa::unit_lower) fn known_functions(
            &self,
            function_ids: &BTreeMap<UnitFunctionInstanceKey, FunctionId>,
            plan: &crate::ssa::unit_plan::UnitCallablePlan,
        ) -> BTreeMap<lang_frontend::type_checking::UnitCallableTarget, (FunctionId, SsaTypeId)>
        {
            let mut known = BTreeMap::new();
            for (&source, &callable) in &self.named {
                if let Some(key) = plan.source(source)
                    && key.type_arguments().is_empty()
                    && key.callable_arguments().is_empty()
                    && let Some(&function) = function_ids.get(key)
                {
                    known.insert(key.target(), (function, callable));
                }
            }
            known
        }

        pub(in crate::ssa::unit_lower) fn token_type(
            &self,
            plan: &crate::ssa::unit_plan::UnitCallablePlan,
            token: CallableToken,
            span: Span,
        ) -> Result<SsaTypeId, LoweringError> {
            let ty = match plan.callable(token) {
                Some(CallableKey::Lambda { owner, expression }) => {
                    self.lambdas.get(&(*owner, *expression))
                }
                Some(CallableKey::KnownFunction { function }) => self.named.get(function),
                None => None,
            };
            ty.copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }

        pub(in crate::ssa::unit_lower) fn source_token(
            &self,
            key: &UnitFunctionInstanceKey,
            span: Span,
        ) -> Result<SourceToken, LoweringError> {
            self.sources
                .get(key)
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
        }

        pub(in crate::ssa::unit_lower) fn return_type(
            &self,
            plan: &crate::ssa::unit_plan::UnitCallablePlan,
            source: SourceToken,
            span: Span,
        ) -> Result<SsaTypeId, LoweringError> {
            let returned = plan
                .callable_return(source)
                .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
            self.token_type(plan, returned.callable(), span)
        }

        pub(in crate::ssa::unit_lower) fn expression_type(
            &self,
            plan: &crate::ssa::unit_plan::UnitCallablePlan,
            owned: &CompilationUnitOwnership,
            source: SourceToken,
            expression: UnitExpressionId,
            span: Span,
        ) -> Result<SsaTypeId, LoweringError> {
            if owned.closure(expression).is_some() {
                return self
                    .lambdas
                    .get(&(source, expression))
                    .copied()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let fact = owned
                .callable_origin(expression)
                .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
            match fact.origin() {
                UnitCallableOrigin::Lambda(lambda) => self
                    .lambdas
                    .get(&(source, lambda))
                    .copied()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span)),
                UnitCallableOrigin::Parameter(_) => {
                    Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
                }
                UnitCallableOrigin::KnownFunction(target) => {
                    let key = UnitFunctionInstanceKey::for_target(target, Vec::new());
                    let source = self.source_token(&key, span)?;
                    self.named
                        .get(&source)
                        .copied()
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
                }
                UnitCallableOrigin::FactoryResult(call) => {
                    let route = plan
                        .call_site(source, call)
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                    self.return_type(plan, self.source_token(route.key(), span)?, span)
                }
            }
        }
    }

    /// ADR-0034: the callable view loan target for a concrete function type. Borrow function
    /// parameters receive this view instead of a callable-provenance specialization.
    pub(in crate::ssa::unit_lower) fn view_type(
        module: &mut crate::ssa::model::Module,
        typed: &CompilationUnitTypes,
        types: &mut type_lower::UnitTypeLowering,
        function: UnitTypeId,
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let Some(UnitTypeKind::Function {
            parameters,
            return_type,
            ..
        }) = typed.types().get(function).cloned()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let is_function = |ty| matches!(typed.types().get(ty), Some(UnitTypeKind::Function { .. }));
        let mut entities = Vec::with_capacity(parameters.len());
        for parameter in parameters {
            if is_function(parameter.ty()) {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            let ty = types.intern(module, typed, parameter.ty(), span)?;
            entities.push(match parameter.mode() {
                ParameterMode::Value => EntityType::Value(ty),
                ParameterMode::Borrow => EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                },
                ParameterMode::Inout => {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
            });
        }
        let result_builtin = builtin_type(typed, return_type);
        let returns = if matches!(
            result_builtin,
            Some(BuiltinType::Unit | BuiltinType::Nothing)
        ) {
            Vec::new()
        } else if is_function(return_type) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        } else {
            vec![types.intern(module, typed, return_type, span)?]
        };
        module
            .add_callable_view_type(crate::ssa::model::CallableSignature {
                parameters: entities,
                returns,
                never_returns: result_builtin == Some(BuiltinType::Nothing),
            })
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))
    }

    pub(in crate::ssa::unit_lower) fn declare_deinit(
        module: &mut crate::ssa::model::Module,
        parsed_by_source: &[&lang_frontend::parser::ParsedFile],
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        types: &mut type_lower::UnitTypeLowering,
        instance: UnitPlannedInstance,
    ) -> Result<FunctionPlan, LoweringError> {
        let owner = instance
            .key()
            .deinit_owner()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?;
        let descriptor = typed
            .signatures()
            .declaration(owner)
            .and_then(|signature| signature.nominal())
            .and_then(|nominal| nominal.deinit())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?;
        let parsed = parsed_by_source
            .get(instance.source_unit().index())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?;
        let (function_item, item, _) = unwrap_modified(parsed, instance.item())?;
        let Item::Deinit { body, .. } = item else {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                instance.span(),
            ));
        };
        if descriptor.owner() != owner
            || descriptor.item() != UnitItemId::new(instance.source_unit(), function_item)
            || descriptor.body() != UnitStatementId::new(instance.source_unit(), body)
            || descriptor.receiver_mode() != ParameterMode::Borrow
        {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                instance.span(),
            ));
        }
        let receiver_type = descriptor.receiver_type();
        let owner_type = types.intern(module, typed, receiver_type, instance.span())?;
        let entity_type = EntityType::Loan {
            kind: LoanKind::Shared,
            target: owner_type,
        };
        let origin = Origin::Source(instance.span());
        let id = module
            .add_instance_function(
                instance_function_name(names, &instance)?,
                entity_type,
                Vec::new(),
                origin.clone(),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, instance.span()))?;
        module
            .function_mut(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, instance.span()))?
            .add_block(vec![entity_type], origin)
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, instance.span()))?;
        module
            .set_deinit(owner_type, id)
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, instance.span()))?;
        let return_type = typed
            .types()
            .builtin(BuiltinType::Unit)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?;
        let receiver = Some(ReceiverPlan {
            owner,
            mode: descriptor.receiver_mode(),
            template_ty: receiver_type,
            ty: receiver_type,
            entity_type,
            origin: instance.span(),
        });
        Ok(FunctionPlan {
            id,
            instance,
            function_item,
            body: FunctionPlanBody::Block(body),
            receiver,
            parameter_symbols: Vec::new(),
            inout_parameter: None,
            return_type,
        })
    }
}
