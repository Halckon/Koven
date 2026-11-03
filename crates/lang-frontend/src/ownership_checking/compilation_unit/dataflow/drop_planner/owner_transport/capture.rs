//! Leaf formation reads the actual removed values; metadata alone never grants identity.
use super::*;

impl DropPlanner<'_, '_> {
    pub(super) fn owner_capture_supported(&self, expression: ExpressionId) -> bool {
        let mut count = 0;
        for capture in self.checker.captures_of(expression) {
            count += 1;
            if capture.mode() != ClosureCaptureMode::Owned
                || capture.effect() != ClosureCaptureEffect::Move
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
            }
        }
        let environment = self.define_owner(UnitOwnerOrigin::Closure(
            self.checker.unit_expression(expression),
        ))?;
        self.owner_event(UnitOwnerTransportEvent::Capture {
            expression: self.checker.unit_expression(expression),
            environment: environment.clone(),
            inputs,
        });
        Some(environment)
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
