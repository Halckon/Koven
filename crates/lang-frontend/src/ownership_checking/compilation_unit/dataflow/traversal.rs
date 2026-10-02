//! Parser AST 与保守控制流遍历；所有权效果仍由父模块统一执行。

use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    diagnostic::{Diagnostic, Severity},
    parser::{Expression, FunctionBody, FunctionForm, Item, Statement, StringPart},
};

use super::{
    AccessKind, Checker, ExpressionUse, Flows, OwnershipCheckingError, State,
    UnitCallArgumentOwnershipContract, UnitCallArgumentOwnershipKind, merge_state,
};

impl Checker<'_> {
    // Only repeating edges require the owner again; exits keep their ordinary flow checks.
    pub(super) fn check_loop_backedge(
        &mut self,
        body: StatementId,
        flows: &Flows,
    ) -> Result<(), OwnershipCheckingError> {
        let live = &self.statement_live_after[body.index()];
        let mut moved = std::collections::BTreeMap::new();
        for state in [&flows.next, &flows.continues].into_iter().flatten() {
            for (&symbol, &origin) in &state.moved {
                if live.contains(&symbol) {
                    moved.entry(symbol).or_insert(origin);
                }
            }
        }
        for origin in moved.into_values() {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.use_after_move,
                "moved value may be used again on the next loop iteration",
                origin,
            )?;
            diagnostic.add_label(self.sources, origin, "value was moved here")?;
            self.diagnostics.push(diagnostic);
        }
        Ok(())
    }

    pub(super) fn collect_mutability(&mut self, id: ItemId) -> Result<(), OwnershipCheckingError> {
        match self.parsed.ast().items().get(id)?.payload().clone() {
            Item::Error | Item::Constant { .. } | Item::Function { .. } | Item::Deinit { .. } => {}
            Item::Modified { declaration, .. } => self.collect_mutability(declaration)?,
            Item::Variable { kind, name, .. } => {
                if let Some(symbol) = self.marker_symbol(name).copied() {
                    self.variable_kinds.insert(symbol, kind);
                }
            }
            Item::Classifier(classifier) => {
                if let Some(constructor) = classifier.primary_constructor {
                    for field in constructor.fields {
                        if let Some(symbol) = self.marker_symbol(field.name).copied() {
                            self.field_kinds.insert(symbol, field.kind);
                        }
                    }
                }
                if let Some(body) = classifier.body {
                    for member in body.members {
                        self.collect_mutability(member)?;
                    }
                }
            }
            Item::Companion(companion) => {
                for member in companion.body.members {
                    self.collect_mutability(member)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_item(
        &mut self,
        id: ItemId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        match self.parsed.ast().items().get(id)?.payload().clone() {
            Item::Error | Item::Constant { .. } => Ok(Flows::next(state)),
            Item::Modified { declaration, .. } => self.check_item(declaration, state),
            Item::Variable {
                name, initializer, ..
            } => {
                let diagnostic_count = self.diagnostics.len();
                let closure = self.closure_origin(initializer, &state)?;
                let moved_closure = self.expression_root_symbol(initializer)?;
                let mut flows = self.check_expression(
                    initializer,
                    state,
                    ExpressionUse::Consume {
                        parameter_span: None,
                    },
                )?;
                if let Some(next) = flows.next.as_mut() {
                    self.mark_available(name, next);
                    if let Some(source) = moved_closure {
                        next.closures.remove(&source);
                    }
                    if self.diagnostics.len() == diagnostic_count
                        && let Some(symbol) = self.marker_symbol(name).copied()
                        && let Some(closure) = closure
                    {
                        next.closures.insert(symbol, closure);
                    }
                }
                Ok(flows)
            }
            Item::Function { name, form, .. } => {
                let previous_receiver = self.current_receiver;
                self.current_receiver = self.receiver_context(name);
                let result = self.check_function(form);
                self.current_receiver = previous_receiver;
                result?;
                Ok(Flows::next(state))
            }
            Item::Classifier(classifier) => {
                if let Some(body) = classifier.body {
                    for member in body.members {
                        self.check_item(member, State::default())?;
                    }
                }
                Ok(Flows::next(state))
            }
            Item::Companion(companion) => {
                for member in companion.body.members {
                    self.check_item(member, State::default())?;
                }
                Ok(Flows::next(state))
            }
            Item::Deinit { body, .. } => {
                let declaration_span = self.parsed.ast().items().get(id)?.span();
                let receiver = self
                    .typed
                    .signatures()
                    .declarations()
                    .iter()
                    .filter_map(|declaration| declaration.nominal())
                    .filter_map(|nominal| nominal.deinit())
                    .find(|deinit| {
                        deinit.item() == crate::type_checking::UnitItemId::new(self.source_unit, id)
                    })
                    .map(|deinit| super::ReceiverContext {
                        owner: deinit.owner(),
                        mode: deinit.receiver_mode(),
                        ty: deinit.receiver_type(),
                        declaration_span,
                    });
                let previous_receiver = std::mem::replace(&mut self.current_receiver, receiver);
                let result = self.check_statement(body, State::default());
                self.current_receiver = previous_receiver;
                result?;
                Ok(Flows::next(state))
            }
        }
    }

    fn check_function(&mut self, form: FunctionForm) -> Result<(), OwnershipCheckingError> {
        let state = State::default();
        match form {
            FunctionForm::ImplicitUnitAbsent => {}
            FunctionForm::ImplicitUnitBlock(body) => {
                self.check_statement(body, state)?;
            }
            FunctionForm::Explicit { body, .. } => match body {
                FunctionBody::Absent => {}
                FunctionBody::Expression { expression, .. } => {
                    self.check_return_expression(
                        expression,
                        state,
                        ExpressionUse::Consume {
                            parameter_span: None,
                        },
                    )?;
                }
                FunctionBody::Block(body) => {
                    self.check_statement(body, state)?;
                }
            },
        }
        Ok(())
    }

    pub(super) fn check_statement(
        &mut self,
        id: StatementId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        match self.parsed.ast().statements().get(id)?.payload().clone() {
            Statement::Error => Ok(Flows::next(state)),
            Statement::Block { elements }
            | Statement::LambdaBody { elements }
            | Statement::ControlBody { elements } => self.check_elements(&elements, state),
            Statement::LocalVariable { declaration } => {
                let mut flows = self.check_item(declaration, state)?;
                if let Item::Variable { name, .. } = self
                    .parsed
                    .ast()
                    .items()
                    .get(declaration)?
                    .payload()
                    .clone()
                    && let Some(symbol) = self.marker_symbol(name).copied()
                    && !self.statement_live_after[id.index()].contains(&symbol)
                {
                    for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
                        .into_iter()
                        .flatten()
                    {
                        self.release_closure(symbol, state);
                    }
                }
                Ok(flows)
            }
            Statement::LocalDestructuring { initializer, .. } => self.check_expression(
                initializer,
                state,
                ExpressionUse::Consume {
                    parameter_span: None,
                },
            ),
            Statement::While {
                condition, body, ..
            }
            | Statement::For {
                source: condition,
                body,
                ..
            } => {
                let errors = self.diagnostics.len();
                let prefix = self.check_expression(condition, state, ExpressionUse::Read)?;
                self.check_maybe_loop(prefix, body, errors)
            }
            Statement::Loop { body, .. } => {
                let errors = self.diagnostics.len();
                let body_id = body;
                let body = self.check_statement(body, state)?;
                if self.diagnostics.len() == errors {
                    self.check_loop_backedge(body_id, &body)?;
                }
                Ok(Flows {
                    next: body.breaks,
                    ..Flows::default()
                })
            }
            Statement::Expression { expression } => {
                let mut flows = self.check_expression(expression, state, ExpressionUse::Read)?;
                self.release_last_closure_use(expression, &mut flows)?;
                Ok(flows)
            }
        }
    }

    pub(super) fn check_elements(
        &mut self,
        elements: &[StatementId],
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let mut flows = Flows::next(state);
        for &element in elements {
            let Some(next) = flows.next.take() else {
                break;
            };
            flows.merge(self.check_statement(element, next)?);
        }
        Ok(flows)
    }

    fn check_maybe_loop(
        &mut self,
        mut prefix: Flows,
        body: StatementId,
        errors: usize,
    ) -> Result<Flows, OwnershipCheckingError> {
        let Some(base) = prefix.next.take() else {
            return Ok(prefix);
        };
        let body_id = body;
        let body = self.check_statement(body, base.clone())?;
        if self.diagnostics.len() == errors {
            self.check_loop_backedge(body_id, &body)?;
        }
        let mut next = base;
        for state in [body.next, body.breaks, body.continues]
            .into_iter()
            .flatten()
        {
            merge_state(&mut next, state);
        }
        prefix.next = Some(next);
        Ok(prefix)
    }

    pub(super) fn check_expression(
        &mut self,
        id: ExpressionId,
        state: State,
        usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        if let Some(descriptor) = self.constant_use(id) {
            let kind = if matches!(
                descriptor.value(),
                crate::type_checking::ConstValue::String(_)
            ) {
                crate::ownership_checking::ConstantMaterializationKind::StringTemporary
            } else {
                crate::ownership_checking::ConstantMaterializationKind::InlineCopy
            };
            self.constant_materializations.insert(
                self.unit_expression(id),
                super::super::constant::UnitConstantMaterializationPlan {
                    descriptor: descriptor.clone(),
                    kind,
                },
            );
            return Ok(Flows::next(state));
        }
        if let Some(descriptor) = self
            .construction_descriptors
            .get(&self.unit_expression(id))
            .cloned()
        {
            return self.check_construction(descriptor, state, usage);
        }
        if let Some(descriptor) = self.typed.integer_operation(self.unit_expression(id)) {
            return self.check_expression(
                descriptor.receiver().expression(),
                state,
                ExpressionUse::Read,
            );
        }
        if let Some(descriptor) = self.typed.string_operation(self.unit_expression(id)) {
            return self.check_string_operation(descriptor, state);
        }
        if let Some(descriptor) = self.typed.rc_operation(self.unit_expression(id)) {
            return self.check_rc_operation(descriptor, state, usage);
        }
        if let Some(plan) = self.short_circuit_plan(id)? {
            return self.check_short_circuit(plan, state);
        }
        let node = self.parsed.ast().expressions().get(id)?;
        let span = node.span();
        match node.payload().clone() {
            Expression::Error | Expression::Literal(_) | Expression::SuperMember { .. } => {
                Ok(Flows::next(state))
            }
            Expression::This => {
                let mut state = state;
                self.use_this(id, span, usage, &mut state)?;
                Ok(Flows::next(state))
            }
            Expression::Name => {
                let mut state = state;
                self.use_name(id, span, usage, &mut state)?;
                Ok(Flows::next(state))
            }
            Expression::Group { expression } => self.check_expression(expression, state, usage),
            Expression::String { parts } => {
                let mut flows = Flows::next(state);
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part {
                        flows = self.chain_expression(flows, expression, ExpressionUse::Read)?;
                    }
                }
                Ok(flows)
            }
            Expression::Lambda { body, .. } => self.check_lambda(id, body, state),
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let branch_usage = self.control_result_usage(id)?;
                self.check_if(
                    condition,
                    then_branch,
                    else_branch,
                    state,
                    branch_usage,
                    false,
                )
            }
            Expression::When {
                subject, entries, ..
            } => {
                let branch_usage = self.control_result_usage(id)?;
                self.check_when(subject, &entries, state, branch_usage, false)
            }
            Expression::Return { value, .. } => {
                let mut flows = if let Some(value) = value {
                    self.check_return_expression(
                        value,
                        state,
                        ExpressionUse::Consume {
                            parameter_span: None,
                        },
                    )?
                } else {
                    Flows::next(state)
                };
                flows.next = None;
                Ok(flows)
            }
            Expression::Break { .. } => Ok(Flows {
                breaks: Some(state),
                ..Flows::default()
            }),
            Expression::Continue { .. } => Ok(Flows {
                continues: Some(state),
                ..Flows::default()
            }),
            Expression::NonNullAssert { operand, .. } => {
                self.check_non_null_assertion(id, operand, state)
            }
            Expression::Prefix { operand, .. }
            | Expression::Cast {
                expression: operand,
                ..
            }
            | Expression::TypeTest {
                expression: operand,
                ..
            }
            | Expression::Propagate { value: operand, .. } => {
                self.check_expression(operand, state, ExpressionUse::Read)
            }
            Expression::Binary { left, right, .. } => {
                let flows = self.check_expression(left, state, ExpressionUse::Read)?;
                self.chain_expression(flows, right, ExpressionUse::Read)
            }
            Expression::Assignment {
                target,
                operator,
                value,
                ..
            } => {
                let diagnostic_count = self.diagnostics.len();
                if self.place(target)?.is_some_and(|place| {
                    !place.fields().is_empty()
                        || self.symbol_kind(place.root())
                            == Some(crate::name_resolution::SymbolKind::Field)
                }) {
                    self.reject_borrowed_closure_escape(value, &state)?;
                }
                if self.diagnostics.len() != diagnostic_count {
                    return self.check_expression(
                        value,
                        state,
                        ExpressionUse::Consume {
                            parameter_span: None,
                        },
                    );
                }
                self.check_assignment(target, operator, value, state)
            }
            Expression::Member {
                receiver,
                name_span,
                ..
            } => {
                let mut state = state;
                if let Some(place) = self.place(id)? {
                    match usage {
                        ExpressionUse::Read => {
                            self.access_place(
                                &place,
                                AccessKind::Read,
                                name_span,
                                None,
                                &mut state,
                            )?;
                        }
                        ExpressionUse::Consume { parameter_span } => {
                            self.consume_place(&place, id, name_span, parameter_span, &mut state)?;
                        }
                        ExpressionUse::Place { parameter_span } => {
                            self.ensure_available(&place, name_span, parameter_span, &state)?;
                        }
                    }
                    Ok(Flows::next(state))
                } else if self
                    .typed
                    .aggregate_projection(self.unit_expression(id))
                    .is_some()
                    && self.element_place_descriptor(receiver)?.is_some()
                {
                    // 与单文件所有权检查保持一致：元素后的聚合字段投影仍是 Place，
                    // 其精确 drop 计划由 liveness 标记为 IndexPlace 暂缓。
                    self.check_expression(
                        receiver,
                        state,
                        ExpressionUse::Place {
                            parameter_span: None,
                        },
                    )
                } else {
                    self.check_expression(receiver, state, ExpressionUse::Read)
                }
            }
            Expression::Call {
                callee, arguments, ..
            } => self.check_call(id, callee, &arguments, state),
            Expression::Index { receiver, index } => {
                if self.element_place_descriptor(id)?.is_some() {
                    return self.check_element_expression(id, state, usage);
                }
                let flows = self.check_expression(receiver, state, ExpressionUse::Read)?;
                self.chain_expression(flows, index, ExpressionUse::Read)
            }
            Expression::CallableReference { receiver, .. } => {
                if let Some(receiver) = receiver {
                    self.check_expression(receiver, state, ExpressionUse::Read)
                } else {
                    Ok(Flows::next(state))
                }
            }
        }
    }

    fn check_call(
        &mut self,
        id: ExpressionId,
        callee: ExpressionId,
        arguments: &[crate::parser::CallArgument],
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let diagnostic_count = self.diagnostics.len();
        let call = self.unit_expression(id);
        let contracts = self
            .contracts_by_call
            .get(&call)
            .cloned()
            .unwrap_or_default();
        let receiver_contract = self.receiver_contracts_by_call.get(&call).copied();
        if !contracts.is_empty() && contracts.len() != arguments.len() {
            return Err(OwnershipCheckingError::InvalidUnitCall {
                source_unit: self.source_unit.index(),
                expression: id.index(),
            });
        }
        let mut receiver_expression = None;
        let mut flows = if let Some(contract) = receiver_contract {
            match contract.source() {
                crate::type_checking::UnitCallReceiverOrigin::Expression(receiver) => {
                    if receiver.source_unit() != self.source_unit {
                        return Err(OwnershipCheckingError::InvalidUnitCall {
                            source_unit: self.source_unit.index(),
                            expression: id.index(),
                        });
                    }
                    receiver_expression = Some(receiver.expression());
                    let usage = match contract.kind() {
                        UnitCallArgumentOwnershipKind::Value
                            if self.expression_is_this(receiver.expression())? =>
                        {
                            ExpressionUse::Place {
                                parameter_span: contract.declaration_span(),
                            }
                        }
                        UnitCallArgumentOwnershipKind::Value => ExpressionUse::Consume {
                            parameter_span: contract.declaration_span(),
                        },
                        UnitCallArgumentOwnershipKind::SharedLoan
                        | UnitCallArgumentOwnershipKind::ExclusiveLoan => ExpressionUse::Place {
                            parameter_span: contract.declaration_span(),
                        },
                    };
                    self.check_expression(receiver.expression(), state, usage)?
                }
                crate::type_checking::UnitCallReceiverOrigin::ImplicitThis(_) => Flows::next(state),
            }
        } else {
            self.check_expression(callee, state, ExpressionUse::Read)?
        };
        if let (Some(contract), Some(next)) = (receiver_contract, flows.next.as_mut()) {
            self.apply_receiver_contract(contract, next)?;
        }
        for (index, argument) in arguments.iter().enumerate() {
            let contract = contracts.get(index).copied();
            if let Some(contract) = contract
                && contract.argument() != self.unit_expression(argument.value)
            {
                return Err(OwnershipCheckingError::InvalidUnitCallArgument {
                    source_unit: self.source_unit.index(),
                    expression: id.index(),
                    argument: index,
                });
            }
            let usage = match contract.map(UnitCallArgumentOwnershipContract::kind) {
                Some(UnitCallArgumentOwnershipKind::Value) => ExpressionUse::Consume {
                    parameter_span: contract.and_then(|contract| contract.parameter_span()),
                },
                Some(
                    UnitCallArgumentOwnershipKind::SharedLoan
                    | UnitCallArgumentOwnershipKind::ExclusiveLoan,
                ) => ExpressionUse::Place {
                    parameter_span: contract.and_then(|contract| contract.parameter_span()),
                },
                None => ExpressionUse::Read,
            };
            let diagnostic_count = self.diagnostics.len();
            if contract.is_some_and(|contract| {
                contract.kind() == UnitCallArgumentOwnershipKind::Value
                    && !contract.crosses_thread()
            }) && let Some(next) = flows.next.as_ref()
            {
                self.reject_borrowed_closure_escape(argument.value, next)?;
            }
            flows = self.chain_expression(flows, argument.value, usage)?;
            if self.is_nothing_expression(self.unit_expression(argument.value)) {
                flows.next = None;
                continue;
            }
            if self.diagnostics.len() == diagnostic_count
                && let (Some(contract), Some(next)) = (contract, flows.next.as_mut())
            {
                if contract.crosses_thread() {
                    self.check_cross_thread_delivery(argument.value, next)?;
                }
                self.apply_contract(contract, next)?;
            }
        }
        if let Some(next) = flows.next.as_mut() {
            self.activate_receiver(call, next)?;
        }
        if flows.next.is_some() && self.diagnostics.len() == diagnostic_count {
            self.record_ownership_primitive(id, arguments)?;
        }
        for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
            .into_iter()
            .flatten()
        {
            state
                .loans
                .retain(|loan| loan.owner != super::ActiveLoanOwner::Call(call));
        }
        for expression in receiver_expression
            .into_iter()
            .chain(receiver_contract.is_none().then_some(callee))
            .chain(arguments.iter().map(|argument| argument.value))
        {
            self.release_last_closure_use(expression, &mut flows)?;
        }
        if self.is_nothing_expression(call) {
            flows.next = None;
        }
        Ok(flows)
    }
}
