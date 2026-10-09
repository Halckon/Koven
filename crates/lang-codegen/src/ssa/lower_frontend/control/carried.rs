//! Construct and restore explicit linear CFG storage slots.
use super::*;
impl ExpressionLowerer<'_> {
    pub(in super::super) fn linear_binding_slots(
        &self,
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        span: Span,
    ) -> Result<LinearBindings, LoweringError> {
        let mut carried = Vec::new();
        for (&symbol, &binding) in bindings {
            let declared = self
                .typed
                .symbol_type(symbol)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let concrete = self.resolve_type(declared, span)?;
            if self.typed.copyability(concrete)
                != Some(lang_frontend::type_checking::Copyability::MoveOnly)
            {
                continue;
            }
            let LoweredValue::Value(source) = binding else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let ty = self
                .function
                .entity(EntityId::Value(source))
                .map(|entity| entity.ty)
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
            if !matches!(ty, EntityType::Value(_)) {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            }
            carried.push(LinearBindingSlot::new(
                Some(symbol),
                EntityId::Value(source),
                ty,
            ));
        }
        for (&key, &owner) in &self.temporaries {
            let source = EntityId::Value(owner);
            if let Some(slot) = carried.iter_mut().find(|slot| slot.source == source) {
                slot.temporaries.push(key);
            } else {
                let ty = self
                    .function
                    .entity(source)
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty;
                let mut slot = LinearBindingSlot::new(None, source, ty);
                slot.temporaries.push(key);
                carried.push(slot);
            }
        }
        for (&key, &loan) in &self.pending_call_loans {
            let Some(loan) = loan else {
                continue;
            };
            let source = EntityId::Loan(loan);
            if let Some(slot) = carried.iter_mut().find(|slot| slot.source == source) {
                slot.loans.push(key);
            } else {
                let ty = self
                    .function
                    .entity(source)
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty;
                let mut slot = LinearBindingSlot::new(None, source, ty);
                slot.loans.push(key);
                carried.push(slot);
            }
        }
        for (&key, &loan) in &self.capture_loans {
            let source = EntityId::Loan(loan);
            if let Some(slot) = carried.iter_mut().find(|slot| slot.source == source) {
                slot.captures.push(key);
            } else {
                let ty = self
                    .function
                    .entity(source)
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty;
                let mut slot = LinearBindingSlot::new(None, source, ty);
                slot.captures.push(key);
                carried.push(slot);
            }
        }
        for (&symbol, &loan) in &self.non_null_bindings {
            let source = EntityId::Loan(loan);
            if let Some(slot) = carried.iter_mut().find(|slot| slot.source == source) {
                slot.views.push(symbol);
            } else {
                let ty = self
                    .function
                    .entity(source)
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty;
                let mut slot = LinearBindingSlot::new(None, source, ty);
                slot.views.push(symbol);
                carried.push(slot);
            }
        }
        for (&symbol, &loan) in &self.borrow_bindings {
            if self.is_entry_loan(loan) {
                continue;
            }
            let source = EntityId::Loan(loan);
            if let Some(slot) = carried.iter_mut().find(|slot| slot.source == source) {
                slot.borrow_symbols.push(symbol);
            } else {
                let ty = self
                    .function
                    .entity(source)
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .ty;
                let mut slot = LinearBindingSlot::new(None, source, ty);
                slot.borrow_symbols.push(symbol);
                carried.push(slot);
            }
        }
        for (&symbol, loans) in &self.result_source_loans {
            for (index, &loan) in loans.iter().enumerate() {
                let source = EntityId::Loan(loan);
                if let Some(slot) = carried.iter_mut().find(|slot| slot.source == source) {
                    slot.result_sources.push((symbol, index));
                } else {
                    let ty = self
                        .function
                        .entity(source)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                        .ty;
                    let mut slot = LinearBindingSlot::new(None, source, ty);
                    slot.result_sources.push((symbol, index));
                    carried.push(slot);
                }
            }
        }
        for context in &self.loops {
            let Some(ref for_data) = context.for_loop else {
                continue;
            };
            let stmt = for_data.statement.index();
            let body_source_loan = for_data.body_source;
            if !self.is_entry_loan(body_source_loan) {
                let source = EntityId::Loan(body_source_loan);
                if let Some(slot) = carried.iter_mut().find(|slot| slot.source == source) {
                    slot.for_sources.push(stmt);
                } else {
                    let ty = self
                        .function
                        .entity(source)
                        .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                        .ty;
                    let mut slot = LinearBindingSlot::new(None, source, ty);
                    slot.for_sources.push(stmt);
                    carried.push(slot);
                }
            }
            let element_loan = for_data.guarded_element.loan();
            if !self.is_entry_loan(element_loan) {
                let source = EntityId::Loan(element_loan);
                if let Some(slot) = carried.iter_mut().find(|slot| slot.source == source) {
                    slot.for_elements.push(stmt);
                } else {
                    let ty = self
                        .function
                        .entity(source)
                        .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                        .ty;
                    let mut slot = LinearBindingSlot::new(None, source, ty);
                    slot.for_elements.push(stmt);
                    carried.push(slot);
                }
            }
        }
        Ok(LinearBindings {
            slots: carried,
            forwarded_loans: self
                .pending_call_loans
                .iter()
                .filter_map(|(&key, loan)| loan.is_none().then_some(key))
                .collect(),
        })
    }

    pub(in super::super) fn add_linear_binding_block(
        &mut self,
        carried: &LinearBindings,
        span: Span,
    ) -> Result<BlockId, LoweringError> {
        self.function
            .add_block(
                carried.slots.iter().map(|slot| slot.ty).collect(),
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))
    }

    pub(in super::super) fn rebind_linear_bindings(
        &mut self,
        baseline: &BTreeMap<SymbolId, LoweredValue>,
        block: BlockId,
        carried: &LinearBindings,
        span: Span,
    ) -> Result<BTreeMap<SymbolId, LoweredValue>, LoweringError> {
        let parameters = &self
            .function
            .block(block)
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
            .parameters;
        if parameters.len() < carried.slots.len() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        // Restore the complete entry state before lowering each sibling branch.
        self.temporaries.clear();
        self.capture_loans.clear();
        self.non_null_bindings.clear();
        let entry_loans = self.entry_loans();
        self.borrow_bindings
            .retain(|_, loan| entry_loans.contains(loan));
        self.pending_call_loans = carried
            .forwarded_loans
            .iter()
            .map(|&key| (key, None))
            .collect();
        self.result_source_loans.clear();
        let mut bindings = baseline.clone();
        for (slot, &parameter) in carried.slots.iter().zip(parameters) {
            if let Some(symbol) = slot.symbol {
                bindings.insert(symbol, LoweredValue::Value(value(parameter)));
            }
            for key in &slot.temporaries {
                self.temporaries.insert(*key, value(parameter));
            }
            for key in &slot.captures {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                self.capture_loans.insert(*key, loan);
            }
            for symbol in &slot.views {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                self.non_null_bindings.insert(*symbol, loan);
            }
            for key in &slot.loans {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                self.pending_call_loans.insert(*key, Some(loan));
            }
            for symbol in &slot.borrow_symbols {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                self.borrow_bindings.insert(*symbol, loan);
            }
            for &(symbol, index) in &slot.result_sources {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                let loans = self.result_source_loans.entry(symbol).or_default();
                if loans.len() <= index {
                    loans.resize(index + 1, loan);
                }
                loans[index] = loan;
            }
            for &stmt in &slot.for_sources {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                for context in &mut self.loops {
                    if let Some(ref mut for_data) = context.for_loop
                        && for_data.statement.index() == stmt
                    {
                        for_data.rebind_source(loan);
                    }
                }
            }
            for &stmt in &slot.for_elements {
                let EntityId::Loan(loan) = parameter else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                for context in &mut self.loops {
                    if let Some(ref mut for_data) = context.for_loop
                        && for_data.statement.index() == stmt
                    {
                        for_data.guarded_element.set_loan(loan);
                    }
                }
            }
        }
        Ok(bindings)
    }
}
