//! CFG rebinding, including one exactly typed nullable proof suffix.
use super::*;
use crate::ssa::model::{LoanKind, SsaTypeId};

impl UnitExpressionLowerer<'_> {
    pub(in super::super) fn rebind_carried_control(
        &mut self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        block: BlockId,
        bindings: &[CarriedBinding],
        loans: &[CarriedAccess],
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoweredValue>, LoweringError> {
        self.rebind_carried_control_suffix(baseline, block, bindings, loans, 0, span)
    }

    pub(in super::super) fn rebind_carried_loans(
        &mut self,
        block: BlockId,
        binding_count: usize,
        loans: &[CarriedAccess],
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoanId>, LoweringError> {
        self.rebind_carried_loans_suffix(block, binding_count, loans, 0, span)
    }

    pub(in super::super) fn add_map_borrow_block(
        &mut self,
        bindings: &[CarriedBinding],
        loans: &[CarriedAccess],
        inner: SsaTypeId,
        span: Span,
    ) -> Result<(BlockId, LoanId), LoweringError> {
        let mut types = bindings
            .iter()
            .map(|slot| slot.ty)
            .chain(loans.iter().map(|slot| slot.ty))
            .collect::<Vec<_>>();
        types.push(EntityType::Loan {
            kind: LoanKind::Shared,
            target: inner,
        });
        let block = self
            .function
            .add_block(types, Origin::Source(span))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let Some(EntityId::Loan(proof)) = self
            .function
            .block(block)
            .and_then(|block| block.parameters.last())
            .copied()
        else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        Ok((block, proof))
    }

    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn rebind_map_borrow_control(
        &mut self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        block: BlockId,
        bindings: &[CarriedBinding],
        loans: &[CarriedAccess],
        proof: LoanId,
        inner: SsaTypeId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let parameters = &self
            .function
            .block(block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .parameters;
        if parameters.len() != bindings.len() + loans.len() + 1
            || parameters.last() != Some(&EntityId::Loan(proof))
            || self
                .function
                .entity(EntityId::Loan(proof))
                .map(|data| data.ty)
                != Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: inner,
                })
        {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        }
        self.bindings =
            self.rebind_carried_control_suffix(baseline, block, bindings, loans, 1, span)?;
        self.borrow_bindings =
            self.rebind_carried_loans_suffix(block, bindings.len(), loans, 1, span)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn rebind_carried_control_suffix(
        &mut self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        block: BlockId,
        bindings: &[CarriedBinding],
        loans: &[CarriedAccess],
        suffix: usize,
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoweredValue>, LoweringError> {
        let rebound = self.rebind_carried_prefix(
            baseline,
            block,
            bindings,
            bindings.len() + loans.len() + suffix,
            span,
        )?;
        let parameters = &self
            .function
            .block(block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .parameters;
        let mut pending = BTreeMap::new();
        for (indices, parameter) in bindings
            .iter()
            .map(|slot| &slot.pending)
            .chain(loans.iter().map(|slot| &slot.pending))
            .zip(parameters)
        {
            for index in indices {
                pending.insert(*index, *parameter);
            }
        }
        if pending.keys().copied().ne(0..pending.len()) {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        }
        self.pending_operands = pending.into_values().collect();
        self.temporaries.clear();
        for (slot, parameter) in bindings.iter().zip(parameters) {
            let EntityId::Value(value) = parameter else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            carry::apply_temporary_column(&slot.temporaries, *value, &mut self.temporaries);
        }
        let source_parameters = parameters[bindings.len()..].to_vec();
        self.rebind_range_sources(loans, &source_parameters, span)?;
        self.rebind_carried_nullable_proofs(block, &rebound, bindings, loans, span)?;
        self.validate_rebound_pending_children(block, &rebound, bindings, loans, span)?;
        Ok(rebound)
    }

    fn rebind_carried_prefix(
        &mut self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        block: BlockId,
        carried: &[CarriedBinding],
        expected_parameter_count: usize,
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoweredValue>, LoweringError> {
        let parameters = &self
            .function
            .block(block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .parameters;
        if parameters.len() != expected_parameter_count {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        }
        let mut rebound = baseline.clone();
        for (slot, parameter) in carried.iter().zip(parameters) {
            let EntityId::Value(value) = parameter else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            if let Some(symbol) = slot.symbol {
                rebound.insert(symbol, LoweredValue::Value(*value));
            } else if let Some(mut receiver) = slot.receiver {
                receiver.entity = EntityId::Value(*value);
                self.current_receiver = Some(receiver);
                self.consumed_receiver = None;
            }
        }
        Ok(rebound)
    }

    fn rebind_carried_loans_suffix(
        &mut self,
        block: BlockId,
        binding_count: usize,
        loans: &[CarriedAccess],
        suffix: usize,
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoanId>, LoweringError> {
        let parameters = &self
            .function
            .block(block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .parameters;
        if parameters.len() != binding_count + loans.len() + suffix {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        }
        let mut rebound = BTreeMap::new();
        self.capture_loans.clear();
        for (slot, parameter) in loans.iter().zip(parameters.iter().skip(binding_count)) {
            if slot.symbol.is_none() && slot.receiver.is_none() && slot.captures.is_empty() {
                continue;
            }
            let EntityId::Loan(loan) = parameter else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            for key in &slot.captures {
                self.capture_loans.insert(*key, *loan);
            }
            if let Some(symbol) = slot.symbol {
                rebound.insert(symbol, *loan);
            } else if let Some(mut receiver) = slot.receiver {
                receiver.entity = EntityId::Loan(*loan);
                self.current_receiver = Some(receiver);
                self.consumed_receiver = None;
            }
        }
        Ok(rebound)
    }
}
