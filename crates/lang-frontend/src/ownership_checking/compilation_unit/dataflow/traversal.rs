//! Parser AST 与保守控制流遍历；所有权效果仍由父模块统一执行。

use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    parser::{Expression, FunctionBody, FunctionForm, Item, Statement, StringPart},
};

use super::{
    AccessKind, Checker, ExpressionUse, Flows, OwnershipCheckingError, State,
    UnitCallArgumentOwnershipContract, UnitCallArgumentOwnershipKind, merge_state,
};

impl Checker<'_> {
    pub(super) fn collect_mutability(&mut self, id: ItemId) -> Result<(), OwnershipCheckingError> {
        match self.parsed.ast().items().get(id)?.payload().clone() {
            Item::Error | Item::Constant { .. } | Item::Function { .. } => {}
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
            Item::Error => Ok(Flows::next(state)),
            Item::Modified { declaration, .. } => self.check_item(declaration, state),
            Item::Variable {
                name, initializer, ..
            }
            | Item::Constant {
                name, initializer, ..
            } => {
                let mut flows = self.check_expression(
                    initializer,
                    state,
                    ExpressionUse::Consume {
                        parameter_span: None,
                    },
                )?;
                if let Some(next) = flows.next.as_mut() {
                    self.mark_available(name, next);
                }
                Ok(flows)
            }
            Item::Function { form, .. } => {
                self.check_function(form)?;
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
                    self.check_expression(
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
            Statement::LocalVariable { declaration } => self.check_item(declaration, state),
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
                let prefix = self.check_expression(condition, state, ExpressionUse::Read)?;
                self.check_maybe_loop(prefix, body)
            }
            Statement::Loop { body, .. } => {
                let body = self.check_statement(body, state)?;
                Ok(Flows {
                    next: body.breaks,
                    ..Flows::default()
                })
            }
            Statement::Expression { expression } => {
                self.check_expression(expression, state, ExpressionUse::Read)
            }
        }
    }

    fn check_elements(
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
    ) -> Result<Flows, OwnershipCheckingError> {
        let Some(base) = prefix.next.take() else {
            return Ok(prefix);
        };
        let body = self.check_statement(body, base.clone())?;
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
        if let Some(descriptor) = self
            .construction_descriptors
            .get(&self.unit_expression(id))
            .cloned()
        {
            return self.check_construction(descriptor, state, usage);
        }
        if let Some(descriptor) = self.typed.rc_operation(self.unit_expression(id)) {
            return self.check_rc_operation(descriptor, state, usage);
        }
        let node = self.parsed.ast().expressions().get(id)?;
        let span = node.span();
        match node.payload().clone() {
            Expression::Error
            | Expression::This
            | Expression::Literal(_)
            | Expression::SuperMember { .. } => Ok(Flows::next(state)),
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
            Expression::Lambda { body, .. } => {
                // Lambda body has its own invocation state. Capture effects are published by a
                // later SPEC-0198 slice, so its local moves must not leak into formation.
                self.check_statement(body, State::default())?;
                Ok(Flows::next(state))
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.check_if(condition, then_branch, else_branch, state),
            Expression::When {
                subject, entries, ..
            } => self.check_when(subject, &entries, state),
            Expression::Return { value, .. } => {
                let mut flows = Flows::next(state);
                if let Some(value) = value {
                    flows = self.chain_expression(
                        flows,
                        value,
                        ExpressionUse::Consume {
                            parameter_span: None,
                        },
                    )?;
                }
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
            Expression::Prefix { operand, .. }
            | Expression::Cast {
                expression: operand,
                ..
            }
            | Expression::TypeTest {
                expression: operand,
                ..
            }
            | Expression::NonNullAssert { operand, .. }
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
            } => self.check_assignment(target, operator, value, state),
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
        let call = self.unit_expression(id);
        let contracts = self
            .contracts_by_call
            .get(&call)
            .cloned()
            .unwrap_or_default();
        if !contracts.is_empty() && contracts.len() != arguments.len() {
            return Err(OwnershipCheckingError::InvalidUnitCall {
                source_unit: self.source_unit.index(),
                expression: id.index(),
            });
        }
        let mut flows = self.check_expression(callee, state, ExpressionUse::Read)?;
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
            flows = self.chain_expression(flows, argument.value, usage)?;
            if self.is_nothing_expression(self.unit_expression(argument.value)) {
                flows.next = None;
                continue;
            }
            if self.diagnostics.len() == diagnostic_count
                && let (Some(contract), Some(next)) = (contract, flows.next.as_mut())
            {
                self.apply_contract(contract, next)?;
            }
        }
        for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
            .into_iter()
            .flatten()
        {
            state.loans.retain(|loan| loan.owner != call);
        }
        if self.is_nothing_expression(call) {
            flows.next = None;
        }
        Ok(flows)
    }
}
