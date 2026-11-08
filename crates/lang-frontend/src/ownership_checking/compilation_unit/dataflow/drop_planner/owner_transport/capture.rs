//! Leaf formation reads the actual removed values; metadata alone never grants identity.
use super::*;
use crate::ownership_checking::LoanKind;

impl DropPlanner<'_, '_> {
    pub(super) fn owner_capture_supported(&self, expression: ExpressionId) -> bool {
        let mut count = 0;
        let mut shared = None;
        for capture in self.checker.captures_of(expression) {
            count += 1;
            let is_shared = capture.borrow_kind() == Some(LoanKind::Shared);
            if shared.is_some_and(|prior| prior != is_shared) {
                return false;
            }
            shared = Some(is_shared);
            if !(is_shared
                || (capture.mode() == ClosureCaptureMode::Owned
                    && capture.effect() == ClosureCaptureEffect::Move))
                || !matches!(capture.source(), UnitClosureCaptureSource::Symbol(_))
                || self.checker.typed.types().get(capture.ty())
                    != Some(&UnitTypeKind::Builtin(BuiltinType::String))
            {
                return false;
            }
        }
        count != 0
    }

    pub(in super::super) fn form_capture_owner(
        &mut self,
        expression: ExpressionId,
        usage: DropExpressionUse,
        state: &mut ValueState,
    ) -> Option<UnitOwnerId> {
        // Discarded temporary environments do not yet publish captured cleanup identities.
        if usage != DropExpressionUse::Consume {
            self.reject_owner_expression(expression, UnitOwnerUnsupportedReason::Syntax);
        }
        let captures = self.checker.captures_of(expression).collect::<Vec<_>>();
        let mut inputs = Vec::new();
        let shared = captures
            .first()
            .is_some_and(|capture| capture.borrow_kind() == Some(LoanKind::Shared));
        for capture in captures {
            if capture.mode() == ClosureCaptureMode::Owned
                && capture.effect() == ClosureCaptureEffect::Move
                && let UnitClosureCaptureSource::Symbol(symbol) = capture.source()
            {
                // Preserve the legacy removal and its order even when observation is disabled.
                let value = self.take_named(expression, symbol, state);
                if self.owner_observation_active() {
                    if let Some(owner) = value.and_then(|value| value.owner) {
                        inputs.push((symbol, owner));
                    } else {
                        self.reject_owner_expression(
                            expression,
                            UnitOwnerUnsupportedReason::MissingOwner,
                        );
                    }
                }
            } else if shared
                && self.owner_observation_active()
                && let UnitClosureCaptureSource::Symbol(symbol) = capture.source()
            {
                let owner = state
                    .position(symbol)
                    .and_then(|index| state.values[index].owner.clone());
                if let Some(owner) = owner {
                    inputs.push((symbol, owner));
                } else {
                    self.reject_owner_expression(
                        expression,
                        UnitOwnerUnsupportedReason::MissingOwner,
                    );
                }
            }
        }
        let environment = self.define_owner(UnitOwnerOrigin::Closure(
            self.checker.unit_expression(expression),
        ))?;
        if shared {
            self.owner_event(UnitOwnerTransportEvent::SharedCapture {
                expression: self.checker.unit_expression(expression),
                environment: environment.clone(),
                inputs,
            });
        } else {
            self.owner_event(UnitOwnerTransportEvent::Capture {
                expression: self.checker.unit_expression(expression),
                environment: environment.clone(),
                inputs,
            });
        }
        Some(environment)
    }

    pub(in super::super) fn end_shared_capture(
        &mut self,
        point: PlannerDropPoint,
        value: &OwnedValue,
        capture: crate::ownership_checking::UnitClosureCaptureDescriptor,
        state: &ValueState,
    ) {
        // The existing cleanup action is unconditional on observation, including
        // unsupported captures. Its exact position and payload are unchanged.
        self.iteration_actions.push((
            point,
            crate::ownership_checking::UnitIterationCleanupAction::EndCaptureLoan {
                closure: capture.lambda(),
                source: capture.source(),
            },
        ));
        let environment = value.owner.as_ref();
        let source = capture.source();
        if !self.owner_observation_active() {
            return;
        }
        let (Some(environment), UnitClosureCaptureSource::Symbol(source)) = (environment, source)
        else {
            self.reject_owner(
                point.into_unit(self.checker.source_unit),
                UnitOwnerUnsupportedReason::MissingOwner,
            );
            return;
        };
        let Some(owner) = state
            .position(source)
            .and_then(|index| state.values[index].owner.clone())
        else {
            self.reject_owner(
                point.into_unit(self.checker.source_unit),
                UnitOwnerUnsupportedReason::MissingOwner,
            );
            return;
        };
        self.owner_event(UnitOwnerTransportEvent::SharedCaptureEnd {
            point: point.into_unit(self.checker.source_unit),
            environment: environment.clone(),
            source,
            owner,
        });
    }

    pub(in super::super) fn drop_capture_owner(
        &mut self,
        point: PlannerDropPoint,
        environment: Option<&UnitOwnerId>,
        source: UnitClosureCaptureSource,
    ) {
        if !self.owner_observation_active() {
            return;
        }
        let (Some(environment), UnitClosureCaptureSource::Symbol(source)) = (environment, source)
        else {
            self.reject_owner(
                point.into_unit(self.checker.source_unit),
                UnitOwnerUnsupportedReason::MissingOwner,
            );
            return;
        };
        let owner = self.owner_observation.as_ref().and_then(|observation| {
            observation.events.iter().find_map(|event| match event {
                UnitOwnerTransportEvent::Capture {
                    environment: formed,
                    inputs,
                    ..
                } if formed == environment => inputs
                    .iter()
                    .find(|(symbol, _)| *symbol == source)
                    .map(|(_, owner)| owner.clone()),
                _ => None,
            })
        });
        let Some(owner) = owner else {
            self.reject_owner(
                point.into_unit(self.checker.source_unit),
                UnitOwnerUnsupportedReason::MissingOwner,
            );
            return;
        };
        self.owner_event(UnitOwnerTransportEvent::CaptureDrop {
            point: point.into_unit(self.checker.source_unit),
            environment: environment.clone(),
            source,
            owner,
        });
    }
}
