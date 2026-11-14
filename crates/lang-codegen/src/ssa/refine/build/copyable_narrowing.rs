//! Per-edge availability of lowered Copyable nullable payloads.
use lang_frontend::{name_resolution::UnitSymbolId, source::Span};

use super::Builder;
use crate::ssa::{
    lowering_support::LoweringError,
    refine::body::{BlockRef, ROp, RTerm, Slot},
};

impl Builder<'_, '_> {
    pub(super) fn narrowed_value(&self, symbol: UnitSymbolId) -> Option<Slot> {
        let available = self.narrowed.get(&self.block?)?;
        if available.contains(&symbol) {
            self.narrow_slots.get(&symbol).copied()
        } else {
            None
        }
    }

    pub(super) fn forget_narrowing(&mut self, symbol: UnitSymbolId) {
        if let Some(block) = self.block
            && let Some(narrowed) = self.narrowed.get_mut(&block)
        {
            narrowed.remove(&symbol);
        }
    }

    pub(super) fn propagate_narrowing(&mut self, block: BlockRef, terminator: &RTerm) {
        let narrowed = self.narrowed.remove(&block).unwrap_or_default();
        for successor in terminator.successors() {
            if self.body.blocks[successor.0].has_predecessor {
                // Only a symbol proved on every incoming edge is available at a join.
                if let Some(common) = self.narrowed.get_mut(&successor) {
                    common.retain(|symbol| narrowed.contains(symbol));
                }
            } else if !narrowed.is_empty() {
                self.narrowed.insert(successor, narrowed.clone());
            }
        }
    }

    pub(super) fn record_narrowing(
        &mut self,
        symbol: UnitSymbolId,
        value: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        let block = self.current(span)?;
        let target = *self
            .narrow_slots
            .entry(symbol)
            .or_insert_with(|| self.body.slot(self.body.slots[value.0]));
        // Different proofs of the same symbol define the same logical slot.
        // SSA construction inserts a block parameter when those definitions meet.
        self.push(ROp::Alias { source: value }, vec![target], span)?;
        self.narrowed.entry(block).or_default().insert(symbol);
        Ok(())
    }
}
