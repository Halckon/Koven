//! Hooks at actual successful entry binding producer sites, never reconstructed from final SSA.
use super::*;
use crate::ssa::refine::entry_lifecycle::Position;
impl Builder<'_, '_> {
    pub(super) fn entry_position(&self) -> Option<Position> {
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

impl Builder<'_, '_> {
    pub(super) fn entry_scalar(
        &mut self,
        declaration: lang_frontend::ast::ItemId,
        initializer: ExpressionId,
        symbol: UnitSymbolId,
        value: Slot,
        binding: Slot,
        start: Option<Position>,
    ) {
        let end = self.entry_position();
        let initializer = self.unit(initializer);
        let declaration =
            lang_frontend::type_checking::UnitItemId::new(self.inputs.source_unit, declaration);
        let emitted = start.zip(end).and_then(|(start, end)| {
            if start.0 != end.0 || end.1 != start.1 + 2 {
                return None;
            }
            let original_type = self.inputs.typed.symbol_type(symbol)?;
            if self.inputs.typed.types().get(original_type)
                != Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(
                    lang_frontend::type_checking::BuiltinType::Int,
                ))
            {
                return None;
            }
            let expected_type = EntityType::Value(*self.types.type_ids.get(&original_type)?);
            if self.body.slots[value.0] != expected_type
                || self.body.slots[binding.0] != expected_type
            {
                return None;
            }
            let statements = &self.body.blocks[start.0.0].statements;
            let literal = statements.get(start.1)?;
            let alias = statements.get(start.1 + 1)?;
            let ROp::Constant(crate::ssa::model::ScalarConstant::Integer(actual)) = literal.op
            else {
                return None;
            };
            (literal.results == [value]
                && alias.results == [binding]
                && matches!(alias.op, ROp::Alias {source} if source==value)
                && self.bindings.get(&symbol) == Some(&binding))
            .then_some((start, value, binding, self.body.slots[value.0], actual))
        });
        for entry in &mut self.body.entry_borrows {
            if let Some(p) = &mut entry.lifecycle
                && let Some(expected) = p.evidence.scalar()
                && expected.declaration() == declaration
            {
                p.invalid |= expected.initializer() != initializer
                    || expected.symbol() != symbol
                    || p.scalar.is_some()
                    || emitted
                        .as_ref()
                        .is_none_or(|(_, _, _, _, actual)| *actual != expected.value());
                p.scalar = emitted.map(|(pos, value, binding, ty, _)| (pos, value, binding, ty));
            }
        }
    }
}
