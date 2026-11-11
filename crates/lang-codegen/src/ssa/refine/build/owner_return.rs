//! The Return product authorizes the old payload handoff and unique terminator.
use super::*;
impl Builder<'_, '_> {
    pub(super) fn owner_return(
        &mut self,
        expression: ExpressionId,
        result: Option<Slot>,
        unit_return: bool,
        span: Span,
    ) -> Result<(), LoweringError> {
        let identity = self.unit(expression);
        #[cfg(test)]
        let result = {
            use super::owner_consumer_test_support::{OwnerFault, take_fault};
            if self
                .owner_plan
                .as_ref()
                .is_some_and(|p| p.return_expression == Some(identity))
            {
                if take_fault(OwnerFault::MissingOwnerReturn) {
                    return Ok(());
                }
                if take_fault(OwnerFault::WrongReturnPayload) {
                    Some(Slot(usize::MAX))
                } else {
                    result
                }
            } else {
                result
            }
        };
        let receipt = if let Some(plan) = self
            .owner_plan
            .as_ref()
            .filter(|p| p.return_expression == Some(identity))
        {
            let (selected, required_stage) = if let Some(replacement) = &plan.return_replacement {
                if !self.consumed_drops.contains(&replacement.drop_index) {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                }
                (replacement.old, 9)
            } else if let Some(binding) = &plan.return_binding {
                if !self.dead.contains(&binding.symbol) {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                }
                (binding.slot, 4)
            } else {
                (plan.root_slot, 1)
            };
            let selected = selected.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if unit_return
                || plan.stage != required_stage
                || result != Some(selected)
                || !self.dead.contains(&plan.root)
                || self.inputs.borrow_return.is_some()
                || self.inputs.carrier_return.is_some()
            {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            }
            Some(selected)
        } else {
            None
        };
        let endpoint = self.open_loop_return(identity, result, unit_return, span)?;
        let header = self.header_return(identity, result, unit_return, span)?;
        if [endpoint.is_some(), receipt.is_some(), header.is_some()]
            .into_iter()
            .filter(|v| *v)
            .count()
            > 1
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        self.finish_return(result, unit_return, span)?;
        self.open_loop_return_emitted(endpoint, span)?;
        self.header_return_emitted(header, span)?;
        if receipt.is_some() {
            self.owner_plan
                .as_mut()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                .stage = 3;
            #[cfg(test)]
            if super::owner_consumer_test_support::take_fault(
                super::owner_consumer_test_support::OwnerFault::DuplicateOwnerReturn,
            ) {
                return self.owner_return(expression, result, unit_return, span);
            }
        }
        #[cfg(test)]
        super::super::record_owner_return(
            identity,
            result.map(|s| s.0),
            receipt
                .or(endpoint.map(|e| e.slot()))
                .or(header.map(|e| e.slot()))
                .map(|s| s.0),
        );
        Ok(())
    }
}
