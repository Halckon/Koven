//! The sealed call consumer authorizes the existing emitter, never a second call.
use super::*;

impl Builder<'_, '_> {
    pub(super) fn owner_unit_call(
        &mut self,
        call: UnitExpressionId,
        op: ROp,
        span: Span,
    ) -> Result<(), LoweringError> {
        #[cfg(test)]
        let op = {
            use super::owner_consumer_test_support::{OwnerFault, take_fault};
            let mut op = op;
            if self
                .owner_plan
                .as_ref()
                .is_some_and(|p| p.call.is_some_and(|c| c.0 == call))
            {
                if take_fault(OwnerFault::MissingOwnCall) {
                    return Ok(());
                }
                if take_fault(OwnerFault::WrongOwnCallSlot)
                    && let ROp::DirectCall { arguments, .. } = &mut op
                {
                    let index = self
                        .owner_plan
                        .as_ref()
                        .and_then(|p| p.call_layout)
                        .map_or(0, |p| p.0);
                    arguments[index] = Slot(usize::MAX);
                }
            }
            op
        };
        let receipt = if let Some(plan) = self
            .owner_plan
            .as_ref()
            .filter(|p| p.call.is_some_and(|c| c.0 == call))
        {
            let ROp::DirectCall {
                callee,
                receiver: None,
                arguments,
            } = &op
            else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            let selected = plan
                .root_slot
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let (parameter_index, arity) = plan
                .call_layout
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if plan.call != Some((call, *callee))
                || plan.stage != 1
                || arguments.len() != arity
                || arguments.get(parameter_index) != Some(&selected)
                || arguments.iter().filter(|slot| **slot == selected).count() != 1
                || !self.dead.contains(&plan.root)
            {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            }
            Some(selected)
        } else {
            None
        };
        #[cfg(test)]
        let actual = match &op {
            ROp::DirectCall {
                callee, arguments, ..
            } => Some((*callee, arguments.clone())),
            _ => None,
        };
        #[cfg(test)]
        let replay = op.clone();
        self.push(op, Vec::new(), span)?;
        if receipt.is_some() {
            let plan = self
                .owner_plan
                .as_mut()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            plan.stage = 3;
            #[cfg(test)]
            if super::owner_consumer_test_support::take_fault(
                super::owner_consumer_test_support::OwnerFault::DuplicateOwnCall,
            ) {
                return self.owner_unit_call(call, replay, span);
            }
        }
        #[cfg(test)]
        if let Some((callee, arguments)) = actual {
            super::super::record_owner_call(call, callee, &arguments, receipt.map(|s| s.0));
        }
        Ok(())
    }
}
