//! 声明入口、callee 状态与返回来源上下文的生命周期。
use super::{Checker, OwnershipCheckingError, State};
use crate::{ast::ItemId, parser::Item, type_checking::ParameterMode};

impl Checker<'_> {
    pub(super) fn check_item(
        &mut self,
        id: ItemId,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        match self.parsed.ast().items().get(id)?.payload().clone() {
            Item::Error | Item::Constant { .. } => {}
            Item::Modified { declaration, .. } => self.check_item(declaration, state)?,
            Item::Variable {
                kind,
                name,
                initializer,
                ..
            } => {
                if let crate::parser::VariableKind::BorrowVal(marker) = kind {
                    self.emit_borrow_binding_diagnostic(
                        crate::diagnostic::codes::BORROW_RESULT_ESCAPE,
                        "borrow binding must remain in a local scope",
                        marker,
                    )?;
                    return Ok(());
                }
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
                let receiver_symbol = self
                    .marker_symbol(name)
                    .and_then(|symbol| self.typed.callables().iter().find(|c| c.symbol() == symbol))
                    .and_then(|c| c.extension_receiver_symbol());
                let borrow_source = self.borrow_return_source(form, &parameters, receiver_symbol);
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
}
