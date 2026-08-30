//! compilation-unit CFG 中跨 edge 携带 binding 的共享基元。

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{name_resolution::UnitSymbolId, source::Span, type_checking::Copyability};

use super::{LoweredValue, UnitExpressionLowerer, lowering_error, resolve_concrete_type};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{BlockId, Edge, EntityId, EntityType, Origin, ValueId},
};

pub(super) struct CarriedBinding {
    symbol: UnitSymbolId,
    source: ValueId,
    ty: EntityType,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn move_only_carried_bindings(
        &self,
        bindings: &BTreeMap<UnitSymbolId, LoweredValue>,
        span: Span,
    ) -> Result<Vec<CarriedBinding>, LoweringError> {
        let mut move_only = BTreeMap::new();
        for (symbol, binding) in bindings {
            let ty = self
                .typed
                .types()
                .symbol_type(*symbol)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let ty = resolve_concrete_type(self.typed, ty, self.substitutions, span)?;
            if self.typed.types().copyability(ty) == Copyability::MoveOnly {
                move_only.insert(*symbol, *binding);
            }
        }
        self.carried_bindings(&move_only, span)
    }

    pub(super) fn carried_bindings(
        &self,
        bindings: &BTreeMap<UnitSymbolId, LoweredValue>,
        span: Span,
    ) -> Result<Vec<CarriedBinding>, LoweringError> {
        let mut carried = Vec::new();
        for (symbol, binding) in bindings {
            let source = match binding {
                LoweredValue::Unit => continue,
                LoweredValue::Value(value) => *value,
                LoweredValue::Diverged => {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                }
            };
            let ty = self
                .function
                .entity(EntityId::Value(source))
                .map(|entity| entity.ty)
                .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            carried.push(CarriedBinding {
                symbol: *symbol,
                source,
                ty,
            });
        }
        Ok(carried)
    }

    pub(super) fn add_carried_block(
        &mut self,
        carried: &[CarriedBinding],
        span: Span,
    ) -> Result<BlockId, LoweringError> {
        self.function
            .add_block(
                carried.iter().map(|binding| binding.ty).collect(),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))
    }

    pub(super) fn rebind_carried(
        &self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        block: BlockId,
        carried: &[CarriedBinding],
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoweredValue>, LoweringError> {
        let parameters = &self
            .function
            .block(block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .parameters;
        if parameters.len() != carried.len() {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        }
        let mut rebound = baseline.clone();
        for (slot, parameter) in carried.iter().zip(parameters) {
            let EntityId::Value(value) = parameter else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            rebound.insert(slot.symbol, LoweredValue::Value(*value));
        }
        Ok(rebound)
    }

    pub(super) fn carried_edge_from(
        &self,
        target: BlockId,
        carried: &[CarriedBinding],
        bindings: &BTreeMap<UnitSymbolId, LoweredValue>,
        span: Span,
    ) -> Result<Edge, LoweringError> {
        let arguments = carried
            .iter()
            .map(|slot| match bindings.get(&slot.symbol) {
                Some(LoweredValue::Value(value)) => Ok(EntityId::Value(*value)),
                Some(LoweredValue::Unit | LoweredValue::Diverged) | None => {
                    Err(lowering_error(LoweringErrorKind::MissingFact, span))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Edge { target, arguments })
    }

    pub(super) fn discard_non_entry_bindings(
        &mut self,
        entry_symbols: &BTreeSet<UnitSymbolId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let locals = self
            .bindings
            .keys()
            .filter(|symbol| !entry_symbols.contains(symbol))
            .copied()
            .collect::<Vec<_>>();
        for symbol in locals {
            let ty = self
                .typed
                .types()
                .body_symbol_types()
                .get(&symbol)
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if self.typed.types().copyability(ty) != Copyability::Copyable {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            self.bindings.remove(&symbol);
        }
        Ok(())
    }
}

pub(super) fn carried_edge(target: BlockId, carried: &[CarriedBinding]) -> Edge {
    Edge {
        target,
        arguments: carried
            .iter()
            .map(|binding| EntityId::Value(binding.source))
            .collect(),
    }
}
