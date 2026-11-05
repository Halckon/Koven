//! Identity is transported at existing planner transitions, never reconstructed from drops.
mod branch;
mod capture;
pub(super) mod choice;
#[cfg(test)]
mod tests;

use super::*;
use crate::ownership_checking::{
    UnitOwnerCallable, UnitOwnerDefinition, UnitOwnerId, UnitOwnerObservation,
    UnitOwnerObservationStatus, UnitOwnerOrigin, UnitOwnerTransportEvent,
    UnitOwnerUnsupportedReason,
};
use crate::type_checking::{OwnershipPrimitiveKind, UnitItemId};

/// One evaluation frame owns its result. Nested calls cannot overwrite their parent's slot.
pub(super) struct EvaluatedOwner {
    pub(super) continues: bool,
    pub(super) owner: Option<UnitOwnerId>,
}

impl DropPlanner<'_, '_> {
    fn owner_observation_active(&self) -> bool {
        self.owner_observation
            .as_ref()
            .is_some_and(|observation| observation.status == UnitOwnerObservationStatus::Complete)
    }

    pub(super) fn begin_owner_function(&mut self, item: ItemId) {
        if !self.owner_observation_enabled {
            return;
        }
        let identity = std::sync::Arc::new(());
        self.owner_choice = choice::ChoiceTransaction::begin(self.checker, item, identity.clone());
        self.owner_branch_depth = 0;
        self.owner_observation = Some(UnitOwnerObservation {
            identity,
            callable: UnitOwnerCallable::Function(UnitItemId::new(self.checker.source_unit, item)),
            status: UnitOwnerObservationStatus::Complete,
            definitions: Vec::new(),
            events: Vec::new(),
            choice_value: None,
        });
    }

    pub(super) fn begin_unsupported_owner(
        &mut self,
        callable: UnitOwnerCallable,
        point: crate::ownership_checking::UnitDropPoint,
    ) {
        if !self.owner_observation_enabled {
            return;
        }
        self.owner_choice = None;
        self.owner_branch_depth = 0;
        self.owner_observation = Some(UnitOwnerObservation {
            identity: std::sync::Arc::new(()),
            callable,
            status: UnitOwnerObservationStatus::Unsupported {
                point,
                reason: UnitOwnerUnsupportedReason::Callable,
            },
            definitions: Vec::new(),
            events: Vec::new(),
            choice_value: None,
        });
    }

    pub(super) fn finish_owner_observation(&mut self) {
        let choice = self
            .owner_choice
            .take()
            .and_then(choice::ChoiceTransaction::finish);
        if let Some(mut observation) = self.owner_observation.take() {
            observation.choice_value = choice.map(Box::new);
            if !observation.is_consistent() {
                let point = match observation.callable {
                    UnitOwnerCallable::Function(item) | UnitOwnerCallable::Deinit(item) => {
                        crate::ownership_checking::UnitDropPoint::FunctionEntry(item)
                    }
                    UnitOwnerCallable::Lambda(expression) => {
                        crate::ownership_checking::UnitDropPoint::LambdaEntry(expression)
                    }
                };
                observation.status = UnitOwnerObservationStatus::Unsupported {
                    point,
                    reason: UnitOwnerUnsupportedReason::InconsistentTransport,
                };
                observation.definitions.clear();
                observation.events.clear();
            }
            self.owner_observations.push(observation);
        }
    }

    fn reject_owner(
        &mut self,
        point: crate::ownership_checking::UnitDropPoint,
        reason: UnitOwnerUnsupportedReason,
    ) {
        if let Some(observation) = &mut self.owner_observation
            && observation.status == UnitOwnerObservationStatus::Complete
        {
            observation.status = UnitOwnerObservationStatus::Unsupported { point, reason };
            observation.definitions.clear();
            observation.events.clear();
        }
    }

    pub(super) fn reject_owner_callable(&mut self, item: ItemId) {
        self.reject_owner(
            PlannerDropPoint::FunctionEntry(item).into_unit(self.checker.source_unit),
            UnitOwnerUnsupportedReason::Callable,
        );
    }

    pub(super) fn reject_owner_expression(
        &mut self,
        expression: ExpressionId,
        reason: UnitOwnerUnsupportedReason,
    ) {
        self.reject_owner(
            PlannerDropPoint::AfterExpression(expression).into_unit(self.checker.source_unit),
            reason,
        );
    }

    pub(super) fn owner_event(&mut self, event: UnitOwnerTransportEvent) {
        if let Some(observation) = &mut self.owner_observation
            && observation.status == UnitOwnerObservationStatus::Complete
        {
            observation.events.push(event);
        }
    }

    fn define_owner(&mut self, origin: UnitOwnerOrigin) -> Option<UnitOwnerId> {
        let observation = self.owner_observation.as_mut()?;
        if observation.status != UnitOwnerObservationStatus::Complete {
            return None;
        }
        let id = UnitOwnerId {
            identity: std::sync::Arc::clone(&observation.identity),
            callable: observation.callable,
            index: observation.definitions.len(),
        };
        observation.definitions.push(UnitOwnerDefinition {
            id: id.clone(),
            origin,
        });
        Some(id)
    }

    pub(super) fn parameter_owner(
        &mut self,
        item: ItemId,
        symbol: UnitSymbolId,
    ) -> Option<UnitOwnerId> {
        // First slice has no resource/closure/nullable/component parameter provenance.
        if self
            .checker
            .typed
            .symbol_type(symbol)
            .and_then(|ty| self.checker.typed.types().get(ty))
            != Some(&UnitTypeKind::Builtin(BuiltinType::String))
        {
            self.reject_owner_callable(item);
            return None;
        }
        self.define_owner(UnitOwnerOrigin::Parameter(symbol))
    }

    pub(super) fn define_string_owner(&mut self, expression: ExpressionId) -> Option<UnitOwnerId> {
        if let Some(choice) = &mut self.owner_choice {
            choice.string_completed(self.checker.unit_expression(expression));
        }
        self.define_owner(UnitOwnerOrigin::String(
            self.checker.unit_expression(expression),
        ))
    }

    pub(super) fn moved_owner(
        &mut self,
        expression: ExpressionId,
        value: Option<OwnedValue>,
    ) -> Option<UnitOwnerId> {
        if let Some(choice) = &mut self.owner_choice {
            choice.moved(
                self.checker.unit_expression(expression),
                value.as_ref().map(|value| value.symbol),
            );
        }
        let owner = value.as_ref().and_then(|value| value.owner.clone());
        if let (Some(value), Some(owner)) = (value, owner.as_ref()) {
            self.owner_event(UnitOwnerTransportEvent::Move {
                expression: self.checker.unit_expression(expression),
                symbol: value.symbol,
                owner: owner.clone(),
            });
        }
        if owner.is_none() && self.is_string_expression(expression) {
            self.reject_owner_expression(expression, UnitOwnerUnsupportedReason::MissingOwner);
        }
        owner
    }

    pub(super) fn bound_owner(
        &mut self,
        symbol: UnitSymbolId,
        initializer: ExpressionId,
        owner: Option<UnitOwnerId>,
    ) -> Option<UnitOwnerId> {
        if let Some(owner) = owner.as_ref() {
            self.owner_event(UnitOwnerTransportEvent::Binding {
                symbol,
                initializer: self.checker.unit_expression(initializer),
                owner: owner.clone(),
            });
        } else {
            self.reject_owner_expression(initializer, UnitOwnerUnsupportedReason::MissingOwner);
        }
        owner
    }

    pub(super) fn inserted_owner(&mut self, symbol: UnitSymbolId, initializer: ExpressionId) {
        if let Some(choice) = &mut self.owner_choice {
            choice.bound(symbol, self.checker.unit_expression(initializer));
        }
    }

    pub(super) fn check_owner_statement(
        &mut self,
        id: StatementId,
    ) -> Result<(), OwnershipCheckingError> {
        if !self.owner_observation_active() {
            return Ok(());
        }
        if !matches!(
            self.checker.parsed.ast().statements().get(id)?.payload(),
            Statement::Block { .. }
                | Statement::LocalVariable { .. }
                | Statement::Expression { .. }
        ) {
            self.reject_owner(
                PlannerDropPoint::AfterStatement(id).into_unit(self.checker.source_unit),
                UnitOwnerUnsupportedReason::Syntax,
            );
        }
        Ok(())
    }

    fn check_owner_expression(&mut self, id: ExpressionId) -> Result<(), OwnershipCheckingError> {
        if !self.owner_observation_active() {
            return Ok(());
        }
        if self.checker.is_constant_use(id) {
            self.reject_owner_expression(id, UnitOwnerUnsupportedReason::Syntax);
            return Ok(());
        }
        let supported = match self.checker.parsed.ast().expressions().get(id)?.payload() {
            Expression::Name | Expression::Group { .. } | Expression::Return { .. } => true,
            Expression::If {
                else_branch: Some(_),
                ..
            } => {
                self.checker
                    .typed
                    .expression_type(self.checker.unit_expression(id))
                    .and_then(|ty| self.checker.typed.types().get(ty))
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
            }
            Expression::Lambda { .. } => {
                self.owner_branch_depth == 0 && self.owner_capture_supported(id)
            }
            Expression::Literal(_) => self
                .checker
                .typed
                .expression_type(self.checker.unit_expression(id))
                .is_some_and(|ty| self.checker.typed.copyability(ty) == Copyability::Copyable),
            Expression::String { parts } => !parts
                .iter()
                .any(|part| matches!(part, StringPart::Interpolation { .. })),
            Expression::Call { .. } => {
                let supported = self.checker.ownership_primitives.iter().any(|plan| {
                    plan.descriptor().expression() == self.checker.unit_expression(id)
                        && plan.descriptor().kind() == OwnershipPrimitiveKind::Replace
                        && plan.descriptor().operands().len() == 2
                        && plan.places().len() == 1
                        && plan.places()[0].is_root()
                        && self
                            .checker
                            .typed
                            .types()
                            .get(plan.descriptor().value_type())
                            == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                });
                if !supported {
                    self.reject_owner_expression(id, UnitOwnerUnsupportedReason::OpaqueCall);
                }
                supported
            }
            _ => false,
        };
        if !supported {
            self.reject_owner_expression(id, UnitOwnerUnsupportedReason::Syntax);
        }
        Ok(())
    }

    // The bool entrypoint preserves every legacy cleanup caller and continuation decision.
    pub(super) fn expression(
        &mut self,
        id: ExpressionId,
        usage: DropExpressionUse,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        Ok(self.evaluate_owner_expression(id, usage, state)?.continues)
    }

    pub(super) fn evaluate_owner_expression(
        &mut self,
        id: ExpressionId,
        usage: DropExpressionUse,
        state: &mut ValueState,
    ) -> Result<EvaluatedOwner, OwnershipCheckingError> {
        self.check_owner_expression(id)?;
        let mut owner = None;
        let continues = self.expression_inner(id, usage, state, &mut owner)?;
        if let Some(choice) = &mut self.owner_choice {
            choice.expression_completed(
                self.checker.unit_expression(id),
                matches!(usage, DropExpressionUse::Consume),
                continues,
            );
        }
        if !continues || !self.owner_observation_active() {
            owner = None;
        }
        if let Some(owner) = owner.as_ref() {
            self.owner_event(UnitOwnerTransportEvent::Result {
                expression: self.checker.unit_expression(id),
                owner: owner.clone(),
            });
        }
        Ok(EvaluatedOwner { continues, owner })
    }

    pub(super) fn commit_owner_replace(
        &mut self,
        call: ExpressionId,
        state: &mut ValueState,
    ) -> Option<UnitOwnerId> {
        if !self.owner_observation_active() {
            return None;
        }
        let plan =
            self.checker.ownership_primitives.iter().find(|plan| {
                plan.descriptor().expression() == self.checker.unit_expression(call)
            })?;
        let root = plan.places().first()?.root();
        let argument = *plan.descriptor().operands().get(1)?;
        let root_index = state.position(root);
        let previous = root_index.and_then(|index| state.values[index].owner.clone());
        let incoming = self.take_pending_identity(call, argument.expression(), state);
        let (Some(index), Some(previous), Some(incoming)) = (root_index, previous, incoming) else {
            self.reject_owner_expression(call, UnitOwnerUnsupportedReason::MissingOwner);
            return None;
        };
        state.values[index].owner = Some(incoming.clone());
        self.owner_event(UnitOwnerTransportEvent::Replace {
            call: self.checker.unit_expression(call),
            root,
            argument,
            previous: previous.clone(),
            incoming,
        });
        Some(previous)
    }
}
