//! Hooks at actual successful entry binding producer sites, never reconstructed from final SSA.
use super::*;
use crate::ssa::refine::entry_lifecycle::Position;
impl Builder<'_, '_> {
    fn entry_position(&self) -> Option<Position> {
        let block = self.block?;
        Some((block, self.body.blocks.get(block.0)?.statements.len()))
    }
    pub(super) fn entry_commit(&mut self, symbol: UnitSymbolId) {
        let position = self.entry_position();
        for entry in &mut self.body.entry_borrows {
            if let Some(p) = &mut entry.lifecycle
                && p.binding() == symbol
            {
                let event = position
                    .zip(self.borrowed.get(&symbol).copied())
                    .zip(self.result_source_loans.get(&symbol).cloned())
                    .map(|((pos, result), sources)| (pos, result, sources));
                p.invalid |= p.commit.is_some() || event.is_none();
                p.commit = event;
            }
        }
    }
    pub(super) fn entry_argument(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        loan: Slot,
    ) {
        let position = self.entry_position();
        let argument = self.unit(argument);
        for entry in &mut self.body.entry_borrows {
            if let Some(p) = &mut entry.lifecycle
                && p.evidence.uses().iter().any(|u| u.consumer() == call)
            {
                p.invalid |= p.arguments.len() != p.consumers.len()
                    || p.evidence.uses().get(p.arguments.len()).is_none_or(|u| {
                        u.consumer() != call
                            || u.returned().is_none_or(|f| f.argument() != argument)
                    })
                    || self.borrowed.get(&p.binding()) != Some(&loan);
                if let Some(position) = position {
                    p.arguments.push((call, position, loan));
                } else {
                    p.invalid = true;
                }
            }
        }
    }
    pub(super) fn entry_consumer(&mut self, call: UnitExpressionId) {
        let position = self
            .entry_position()
            .and_then(|(b, i)| i.checked_sub(1).map(|i| (b, i)));
        for entry in &mut self.body.entry_borrows {
            if let Some(p) = &mut entry.lifecycle
                && p.evidence.uses().iter().any(|u| u.consumer() == call)
            {
                p.invalid |= p.arguments.len() != p.consumers.len() + 1
                    || p.evidence
                        .uses()
                        .get(p.consumers.len())
                        .is_none_or(|u| u.consumer() != call);
                if let Some(position) = position {
                    p.consumers.push((call, position));
                } else {
                    p.invalid = true;
                }
            }
        }
    }
    pub(super) fn entry_retire(
        &mut self,
        symbol: UnitSymbolId,
        point: UnitDropPoint,
        loan: Option<Slot>,
        transfer: bool,
    ) {
        let position = self.entry_position();
        for entry in &mut self.body.entry_borrows {
            if let Some(p) = &mut entry.lifecycle
                && p.binding() == symbol
            {
                p.invalid |= transfer
                    || p.retirement.is_some()
                    || p.evidence.transaction().point() != point
                    || self.borrowed.contains_key(&symbol);
                p.retirement = position
                    .zip(loan)
                    .zip(self.result_source_loans.get(&symbol).cloned())
                    .map(|((pos, result), sources)| (pos, result, sources));
            }
        }
    }
    pub(super) fn entry_sources_retired(
        &mut self,
        symbol: UnitSymbolId,
        sources: Option<&[Slot]>,
        transfer: bool,
    ) {
        let position = self.entry_position();
        for entry in &mut self.body.entry_borrows {
            if let Some(p) = &mut entry.lifecycle
                && p.binding() == symbol
            {
                p.invalid |= transfer
                    || p.source_retirement.is_some()
                    || self.result_source_loans.contains_key(&symbol);
                p.source_retirement = position.zip(sources.map(<[Slot]>::to_vec));
            }
        }
    }
    pub(super) fn entry_drop(&mut self, index: usize, slot: Slot) {
        let position = self
            .entry_position()
            .and_then(|(b, i)| i.checked_sub(1).map(|i| (b, i)));
        let fact = self.inputs.owned.drops().get(index).copied();
        for entry in &mut self.body.entry_borrows {
            if let Some(p) = &mut entry.lifecycle
                && let Some(fact) = fact
                && fact.point() == p.evidence.transaction().point()
            {
                p.invalid |= p.drop.is_some();
                p.drop = position.map(|pos| (pos, fact, slot));
            }
        }
    }
}
