//! Entry ABI values transport the original Parameter producer, never a fresh literal.
use super::*;
impl Builder<'_, '_> {
    pub(in crate::ssa::refine) fn header_parameter_bound(
        &mut self,
        symbol: UnitSymbolId,
        mode: ParameterMode,
        slot: Slot,
    ) -> Result<(), LoweringError> {
        let Some(p) = &mut self.header_return_plan else {
            return Ok(());
        };
        let Some((parameter, index)) = p.parameter else {
            return Ok(());
        };
        if symbol != parameter {
            return Ok(());
        }
        if mode != ParameterMode::Value
            || self.block != Some(BlockRef(0))
            || self.body.parameters.get(index) != Some(&slot)
            || self.bindings.get(&symbol) != Some(&slot)
            || !matches!(self.body.slots[slot.0], EntityType::Value(_))
            || p.root.is_some()
            || p.entry_read
            || p.produced[0].replace(slot).is_some()
        {
            return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
        }
        Ok(())
    }
    pub(super) fn header_parameter_result(
        &mut self,
        expression: UnitExpressionId,
        slot: Slot,
    ) -> Result<bool, LoweringError> {
        let parameter = self.header_return_plan.as_ref().and_then(|p| {
            (p.boundary.initial() == expression)
                .then_some(p.parameter)
                .flatten()
        });
        let Some((parameter, _)) = parameter else {
            return Ok(false);
        };
        self.sync_dead();
        let named = self.pending_symbol(expression);
        let p = self
            .header_return_plan
            .as_mut()
            .expect("active parameter header");
        if named != Some(parameter)
            || self.block != Some(BlockRef(0))
            || self.bindings.get(&parameter) != Some(&slot)
            || self.dead.contains(&parameter)
            || p.produced[0] != Some(slot)
            || p.root.is_some()
            || std::mem::replace(&mut p.entry_read, true)
        {
            return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
        }
        #[cfg(test)]
        if super::super::owner_consumer_test_support::take_fault(
            super::super::owner_consumer_test_support::OwnerFault::ForgedHeaderParameterLiteral,
        ) {
            let EntityType::Value(string) = self.body.slots[slot.0] else {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            };
            self.push(
                ROp::StringLiteral {
                    string,
                    bytes: b"forged".to_vec(),
                },
                vec![slot],
                self.inputs.span,
            )?;
        }
        Ok(true)
    }
}
