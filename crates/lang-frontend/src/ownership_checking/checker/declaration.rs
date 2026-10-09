//! Callable body context and declaration traversal.
use super::*;

impl Checker<'_> {
    pub(super) fn ownership_bindings(&self) -> Vec<OwnershipBindingDescriptor> {
        self.typed
            .parameter_bindings()
            .iter()
            .map(|binding| {
                let kind = match binding.mode() {
                    ParameterMode::Value => OwnershipBindingKind::Owned,
                    ParameterMode::Borrow => OwnershipBindingKind::Shared,
                    ParameterMode::Inout => OwnershipBindingKind::Exclusive,
                };
                OwnershipBindingDescriptor::new(binding.symbol(), kind)
            })
            .collect()
    }

    pub(super) fn check_item(
        &mut self,
        id: ItemId,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        match self.parsed.ast().items().get(id)?.payload().clone() {
            Item::Error | Item::Constant { .. } => {}
            Item::Modified { declaration, .. } => self.check_item(declaration, state)?,
            Item::Variable {
                name, initializer, ..
            } => {
                if let Some(next) = self.check_variable(name, initializer, state.clone())?.next {
                    *state = next;
                }
            }
            Item::Function {
                name,
                parameters,
                form,
                ..
            } => {
                let borrow_source = self.borrow_return_source(form, &parameters);
                let previous_borrow =
                    std::mem::replace(&mut self.current_borrow_return, borrow_source);
                let mut function_state = State::default();
                for parameter in parameters {
                    self.mark_available(parameter.name, &mut function_state);
                    if let Some(symbol) = self.marker_symbol(parameter.name) {
                        self.seed_callable_parameter(symbol, &mut function_state);
                    }
                }
                let receiver_mode = self.marker_symbol(name).and_then(|symbol| {
                    self.typed
                        .callables()
                        .iter()
                        .find(|callable| callable.symbol() == symbol)
                        .and_then(|callable| callable.receiver())
                        .map(|receiver| receiver.mode())
                });
                let previous = std::mem::replace(&mut self.current_receiver_mode, receiver_mode);
                let previous_return = self.enter_callable(self.marker_symbol(name));
                let result = self.check_function(form, function_state);
                self.leave_callable(previous_return);
                self.current_borrow_return = previous_borrow;
                self.current_receiver_mode = previous;
                result?;
            }
            Item::Classifier(classifier) => {
                if let Some(body) = classifier.body {
                    for member in body.members {
                        self.check_item(member, &mut State::default())?;
                    }
                }
            }
            Item::Companion(companion) => {
                for member in companion.body.members {
                    self.check_item(member, &mut State::default())?;
                }
            }
            Item::Deinit { body, .. } => {
                let previous = self.current_receiver_mode.replace(ParameterMode::Borrow);
                let result = self.check_statement(body, State::default());
                self.current_receiver_mode = previous;
                result?;
            }
        }
        Ok(())
    }

    fn check_function(
        &mut self,
        form: FunctionForm,
        state: State,
    ) -> Result<(), OwnershipCheckingError> {
        match form {
            FunctionForm::ImplicitUnitAbsent => {}
            FunctionForm::ImplicitUnitBlock(body) => {
                self.check_statement(body, state)?;
            }
            FunctionForm::Explicit { body, .. } => match body {
                FunctionBody::Absent => {
                    if let Some(source) = self.current_borrow_return {
                        self.diagnostics.push(Diagnostic::new(
                            self.sources,
                            Severity::Error,
                            codes::catalog()?.resolve(codes::UNSUPPORTED_BORROW_FLOW)?,
                            "borrow result has no body proving its origin",
                            source.marker,
                        )?);
                    }
                }
                FunctionBody::Expression { expression, .. } => {
                    self.check_return_expression(expression, state, ExpressionUse::Consume)?;
                }
                FunctionBody::Block(body) => {
                    self.check_statement(body, state)?;
                }
            },
        }
        Ok(())
    }
}
