//! Actual Drop emission, extracted without changing its authorization order.
use super::*;
impl Builder<'_, '_> {
    /// Emits the drops on the current path; a point reached by several edges emits once
    /// per edge before [`Self::finish_drops`] consumes the facts.
    pub(super) fn emit_drops(
        &mut self,
        facts: &[(usize, UnitDropTarget, Span)],
    ) -> Result<(), LoweringError> {
        self.sync_dead();
        for (index, target, span) in facts {
            let slot = match target {
                UnitDropTarget::Named(symbol) => {
                    if let Some(ty) = self.symbol_type(*symbol)
                        && !self.move_only(ty)
                    {
                        continue;
                    }
                    if !self.dead.insert(*symbol) {
                        // This path already dropped the owner (inside a loop it left).
                        continue;
                    }
                    self.end_views_of(*symbol, *span)?;
                    self.bindings.get(symbol).copied()
                }
                // ADR-0009: a closure's drop glue releases its owned captures; the fact is
                // satisfied by the closure's own drop at the same point.
                UnitDropTarget::Captured { .. } => continue,
                UnitDropTarget::This(owner) => {
                    if self.this_dead {
                        continue;
                    }
                    self.this_dead = true;
                    (self.inputs.receiver.map(|receiver| receiver.owner) == Some(*owner))
                        .then_some(self.receiver)
                        .flatten()
                }
                UnitDropTarget::Temporary(expression) => {
                    if let Some(ty) = self.expression_type_of(*expression)
                        && !self.move_only(ty)
                    {
                        continue;
                    }
                    // A moved named argument: its owner is gone on this path.
                    if let Some(symbol) = self.pending_symbol(*expression) {
                        self.dead.insert(symbol);
                    }
                    self.temporaries.get(expression).copied()
                }
                _ => None,
            }
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, *span))?;
            self.authorize_loan_drops(*index, *target, slot, *span)?;
            let receipt = self.owner_drop_authorization(*index, *target, slot, *span)?;
            let header = self.header_bind_drop(*index, *target, slot, *span)?;
            let counter = self.counter_drop_authorization(*index, *target, slot, *span)?;
            let bridge = self.bridge_drop_authorization(*index, *target, slot, *span)?;
            let bridge_exit = self.bridge_exit_drop_authorization(*index, *target, slot, *span)?;
            let suffix = self.bridge_suffix_drop(*index, *target, slot, *span)?;
            if (suffix.is_some()
                && (bridge_exit.is_some()
                    || bridge.is_some()
                    || counter.is_some()
                    || receipt.is_some()
                    || header.is_some()))
                || (bridge_exit.is_some()
                    && (bridge.is_some()
                        || counter.is_some()
                        || receipt.is_some()
                        || header.is_some()))
                || (bridge.is_some()
                    && (counter.is_some() || receipt.is_some() || header.is_some()))
                || (receipt.is_some() && header.is_some())
                || (counter.is_some() && (receipt.is_some() || header.is_some()))
            {
                return Err(error(LoweringErrorKind::InvalidModel, *span));
            }
            self.push(ROp::Drop { owner: slot }, Vec::new(), *span)?;
            #[cfg(feature = "d10-catalog-experiment")]
            self.entry_drop(*index, slot);
            self.counter_drop_emitted(counter, *span)?;
            self.bridge_emitted(bridge, None, *span)?;
            self.bridge_exit_drop_emitted(bridge_exit, *span)?;
            self.bridge_suffix_emitted(suffix, None, *span)?;
            self.owner_drop_emitted(receipt, *span)?;
            self.header_bind_drop_emitted(header, *span)?;
            #[cfg(test)]
            super::super::record_owner_drop(
                *target,
                *index,
                slot.0,
                receipt
                    .map(owner_consumer::DropReceipt::witness)
                    .or(header.map(|r| r.witness())),
            );
        }
        Ok(())
    }
}
