//! Local borrow binding initialization, statement loan-tag dispatch, and receipt retirement.
use super::{
    field::{OwnerLoan, OwnerRef},
    *,
};
use crate::ssa::{
    borrow_result_support::ordinary_local,
    model::LoanKind,
    refine::body::{LoanTag, RAbort, RStmt},
};
use lang_frontend::{
    ownership_checking::{BorrowBindingStorage, NonNullAssertionTransferKind},
    type_checking::{UnitAggregateProjectionReceiver, UnitConstructionTarget},
};

impl Builder<'_, '_> {
    pub(super) fn push(
        &mut self,
        op: ROp,
        results: Vec<Slot>,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.sync_dead();
        let block = self.current(span)?;
        let consumer = self.body.loan_plans.statement(&op, &results, block, span)?;
        let receiver = self
            .body
            .receiver_plans
            .statement(&op, &results, block, span)?;
        let named = self
            .body
            .named_plans
            .statement(&op, &results, block, span)?;
        let temporary = self
            .body
            .temporary_plans
            .statement(&op, &results, block, span)?;
        let binding = self
            .body
            .binding_plans
            .statement(&op, &results, block, span)?;
        let child = self
            .body
            .child_plans
            .statement(&op, &results, block, span)?;
        let loan_tag = match (consumer, receiver, named, temporary, binding, child) {
            (Some(tag), None, None, None, None, None) => Some(LoanTag::Consumer(tag)),
            (None, Some(tag), None, None, None, None) => Some(LoanTag::Receiver(tag)),
            (None, None, Some(tag), None, None, None) => Some(LoanTag::Named(tag)),
            (None, None, None, Some(tag), None, None) => Some(LoanTag::Temporary(tag)),
            (None, None, None, None, Some(tag), None) => Some(LoanTag::Binding(tag)),
            (None, None, None, None, None, Some(tag)) => Some(LoanTag::Child(tag)),
            (None, None, None, None, None, None) => None,
            _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
        };
        let forward = self
            .body
            .forward_plans
            .statement(&op, &results, block, span)?;
        let loan_tag = match (loan_tag, forward) {
            (None, Some(tag)) => Some(LoanTag::Forward(tag)),
            (tag, None) => tag,
            _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
        };
        let branch = self
            .body
            .branch_plans
            .statement(&op, &results, block, span)?;
        let loan_tag = match (loan_tag, branch) {
            (None, Some(tag)) => Some(LoanTag::Branch(tag)),
            (tag, None) => tag,
            _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
        };
        let nested = self
            .body
            .nested_plans
            .statement(&op, &results, block, span)?;
        let loan_tag = match (loan_tag, nested) {
            (None, Some(tag)) => Some(LoanTag::Nested(tag)),
            (tag, None) => tag,
            _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
        };
        let bounded = self
            .body
            .bounded_plans
            .statement(&op, &results, block, span)?;
        let loan_tag = match (loan_tag, bounded) {
            (None, Some(tag)) => Some(LoanTag::Bounded(tag)),
            (tag, None) => tag,
            _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
        };
        let single = self
            .body
            .single_plans
            .statement(&op, &results, block, span)?;
        let loan_tag = match (loan_tag, single) {
            (None, Some(tag)) => Some(LoanTag::Single(tag)),
            (tag, None) => tag,
            _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
        };
        let multi = self
            .body
            .multi_plans
            .statement(&op, &results, block, span)?;
        let loan_tag = match (loan_tag, multi) {
            (None, Some(tag)) => Some(LoanTag::Multi(tag)),
            (tag, None) => tag,
            _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
        };
        let auxiliary = self
            .body
            .aux_binding_plans
            .statement(&op, &results, block, span)?;
        let loan_tag = match (loan_tag, auxiliary) {
            (None, Some(tag)) => Some(LoanTag::AuxBinding(tag)),
            (tag, None) => tag,
            _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
        };
        let distinct_aux = self
            .body
            .distinct_aux_plans
            .statement(&op, &results, block, span)?;
        let loan_tag = match (loan_tag, distinct_aux) {
            (None, Some(tag)) => Some(LoanTag::DistinctAux(tag)),
            (tag, None) => tag,
            _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
        };
        let later_aux = self
            .body
            .later_aux_plans
            .statement(&op, &results, block, span)?;
        let loan_tag = match (loan_tag, later_aux) {
            (None, Some(tag)) => Some(LoanTag::LaterAux(tag)),
            (tag, None) => tag,
            _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
        };
        let auxiliary_sequence = self
            .body
            .auxiliary_sequence_plans
            .statement(&op, &results, block, span)?;
        let loan_tag = match (loan_tag, auxiliary_sequence) {
            (None, Some(tag)) => Some(LoanTag::AuxiliarySequence(tag)),
            (tag, None) => tag,
            _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
        };
        let interleaved_aux = self
            .body
            .interleaved_aux_plans
            .statement(&op, &results, block, span)?;
        let loan_tag = match (loan_tag, interleaved_aux) {
            (None, Some(tag)) => Some(LoanTag::InterleavedAux(tag)),
            (tag, None) => tag,
            _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
        };
        let multi_binding = self
            .body
            .multi_binding_plans
            .statement(&op, &results, block, span)?;
        let loan_tag = match (loan_tag, multi_binding) {
            (None, Some(tag)) => Some(LoanTag::MultiBinding(tag)),
            (tag, None) => tag,
            _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
        };
        let loop_tag = self.body.loop_plans.statement(&op, &results, block, span)?;
        let fresh_loop_tag = self
            .body
            .fresh_loop_plans
            .statement(&op, &results, block, span)?;
        self.body.blocks[block.0].statements.push(RStmt {
            loop_tag,
            fresh_loop_tag,
            op,
            results,
            span,
            loan_tag,
        });
        Ok(())
    }

    /// Appends `op` producing one fresh slot of `ty`.
    pub(super) fn value(
        &mut self,
        op: ROp,
        ty: EntityType,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let slot = self.body.slot(ty);
        self.push(op, vec![slot], span)?;
        Ok(slot)
    }

    pub(super) fn finish_loan_receipts(&self) -> Result<(), LoweringError> {
        self.body
            .aux_binding_plans
            .finish(&self.consumed_drops, self.inputs.span)?;
        self.body
            .distinct_aux_plans
            .finish(&self.consumed_drops, self.inputs.span)?;
        self.body
            .later_aux_plans
            .finish(&self.consumed_drops, self.inputs.span)?;
        self.body
            .auxiliary_sequence_plans
            .finish(&self.consumed_drops, self.inputs.span)?;
        self.body
            .interleaved_aux_plans
            .finish(&self.consumed_drops, self.inputs.span)?;
        self.body
            .multi_binding_plans
            .finish(&self.consumed_drops, self.inputs.span)?;
        self.body.forward_plans.finish(self.inputs.span)?;
        self.body.branch_plans.finish(self.inputs.span)?;
        self.body.nested_plans.finish(self.inputs.span)?;
        self.body.bounded_plans.finish(self.inputs.span)?;
        self.body.loop_plans.finish(self.inputs.span)?;
        self.body.fresh_loop_plans.finish(self.inputs.span)?;
        self.body.single_plans.finish(self.inputs.span)?;
        self.body.multi_plans.finish(self.inputs.span)?;
        self.body
            .loan_plans
            .finish(&self.consumed_drops, self.inputs.span)?;
        self.body
            .receiver_plans
            .finish(&self.consumed_drops, self.inputs.span)?;
        self.body
            .named_plans
            .finish(&self.consumed_drops, self.inputs.span)?;
        self.body
            .temporary_plans
            .finish(&self.consumed_drops, self.inputs.span)?;
        self.body
            .binding_plans
            .finish(&self.consumed_drops, self.inputs.span)?;
        self.body
            .child_plans
            .finish(&self.consumed_drops, self.inputs.span)
    }

    pub(super) fn authorize_loan_drops(
        &mut self,
        index: usize,
        target: UnitDropTarget,
        slot: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.body
            .aux_binding_plans
            .drop_fact(index, target, slot, span)?;
        self.body
            .distinct_aux_plans
            .drop_fact(index, target, slot, span)?;
        self.body
            .later_aux_plans
            .drop_fact(index, target, slot, span)?;
        self.body
            .auxiliary_sequence_plans
            .drop_fact(index, target, slot, span)?;
        self.body
            .interleaved_aux_plans
            .drop_fact(index, target, slot, span)?;
        self.body
            .multi_binding_plans
            .drop_fact(index, target, slot, span)?;
        self.body.loan_plans.drop_fact(index, target, slot, span)?;
        self.body
            .receiver_plans
            .drop_fact(index, target, slot, span)?;
        self.body.named_plans.drop_fact(index, target, slot, span)?;
        self.body
            .temporary_plans
            .drop_fact(index, target, slot, span)?;
        self.body
            .binding_plans
            .drop_fact(index, target, slot, span)?;
        self.body.child_plans.drop_fact(index, target, slot, span)
    }

    pub(super) fn local_borrow_binding(
        &mut self,
        symbol: UnitSymbolId,
        initializer: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(initializer);
        let fact = self
            .inputs
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .find(|fact| fact.binding() == symbol && fact.initializer() == id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if fact.storage() == BorrowBindingStorage::NewRangeDescriptor {
            let Some((view, loan, created)) = self.lower_range_call(initializer, span)? else {
                return Ok(Lowered::Diverged);
            };
            self.range_views.insert(symbol, view);
            self.borrowed.insert(symbol, loan);
            self.result_source_loans.insert(symbol, created);
            return Ok(Lowered::Unit);
        }
        if fact.storage() == BorrowBindingStorage::BorrowedCarrierMetadata
            && fact.source_loan().is_none()
            && let Ok(root) = self.ungrouped_expr(initializer)
            && let Ok(parent_sym) = self.reference(self.span(root))
            && let Some(&view) = self.range_views.get(&parent_sym)
        {
            if fact.parent() != Some(parent_sym) {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let target = self.ssa_type(self.expression_type(initializer)?, span)?;
            let place = self.value(
                ROp::RootPlace { owner: view },
                EntityType::Place(target),
                span,
            )?;
            let loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                },
                span,
            )?;
            self.borrowed.insert(symbol, loan);
            return Ok(Lowered::Unit);
        }
        let Some(source) = fact.source_loan() else {
            self.child_initializer(symbol, initializer, fact.parent(), span)?;
            let Some((loan, created)) = self.lower_result_place(initializer, span)? else {
                return Ok(Lowered::Diverged);
            };
            self.borrowed.insert(symbol, loan);
            self.result_source_loans.insert(symbol, created);
            self.body.child_plans.child_commit(
                symbol,
                &self.borrowed,
                &self.result_source_loans,
                span,
            )?;
            return Ok(Lowered::Unit);
        };
        self.body.binding_plans.activate(symbol, id, source, span)?;
        self.body
            .aux_binding_plans
            .activate(symbol, id, source, span)?;
        self.body
            .distinct_aux_plans
            .activate(symbol, id, source, span)?;
        self.body
            .later_aux_plans
            .activate(symbol, id, source, span)?;
        self.body
            .auxiliary_sequence_plans
            .activate(symbol, id, source, span)?;
        self.body
            .interleaved_aux_plans
            .activate(symbol, id, source, span)?;
        self.body
            .multi_binding_plans
            .activate(symbol, id, source, span)?;
        self.body.child_plans.activate(symbol, id, source, span)?;
        let local_policy = ordinary_local::policy(
            self.inputs.parsed,
            self.inputs.typed,
            self.inputs.owned,
            &fact,
        );
        let Some((loan, created)) =
            self.lower_result_call(initializer, source, span, local_policy)?
        else {
            return Ok(Lowered::Diverged);
        };
        self.borrowed.insert(symbol, loan);
        self.result_source_loans.insert(symbol, created);
        self.body
            .binding_plans
            .commit(symbol, &self.borrowed, &self.result_source_loans, span)?;
        self.body.aux_binding_plans.commit(
            symbol,
            &self.borrowed,
            &self.result_source_loans,
            span,
        )?;
        self.body.distinct_aux_plans.commit(
            symbol,
            &self.borrowed,
            &self.result_source_loans,
            span,
        )?;
        self.body.later_aux_plans.commit(
            symbol,
            &self.borrowed,
            &self.result_source_loans,
            span,
        )?;
        self.body.auxiliary_sequence_plans.commit(
            symbol,
            &self.borrowed,
            &self.result_source_loans,
            span,
        )?;
        self.body.interleaved_aux_plans.commit(
            symbol,
            &self.borrowed,
            &self.result_source_loans,
            span,
        )?;
        self.body.multi_binding_plans.commit(
            symbol,
            &self.borrowed,
            &self.result_source_loans,
            span,
        )?;
        self.body
            .child_plans
            .commit(symbol, &self.borrowed, &self.result_source_loans, span)?;
        Ok(Lowered::Unit)
    }

    pub(super) fn end_result_bindings(
        &mut self,
        point: UnitDropPoint,
    ) -> Result<(), LoweringError> {
        let ends: Vec<_> = self
            .inputs
            .owned
            .borrow_results()
            .ends()
            .iter()
            .filter(|fact| fact.point() == point)
            .copied()
            .collect();
        // A control transfer ends the bindings on its own path only; the fall-through path
        // still holds them (SPEC-0393).
        let transfer = matches!(point, UnitDropPoint::ControlTransfer(_));
        for fact in ends {
            let loan = if transfer {
                self.borrowed.get(&fact.binding()).copied()
            } else {
                self.borrowed.remove(&fact.binding())
            };
            self.body.binding_plans.retire(
                fact.binding(),
                point,
                loan,
                self.result_source_loans
                    .get(&fact.binding())
                    .map(Vec::as_slice),
                self.inputs.span,
            )?;
            self.body.aux_binding_plans.retire(
                fact.binding(),
                point,
                loan,
                self.result_source_loans
                    .get(&fact.binding())
                    .map(Vec::as_slice),
                self.inputs.span,
            )?;
            self.body.distinct_aux_plans.retire(
                fact.binding(),
                point,
                loan,
                self.result_source_loans
                    .get(&fact.binding())
                    .map(Vec::as_slice),
                self.inputs.span,
            )?;
            self.body.later_aux_plans.retire(
                fact.binding(),
                point,
                loan,
                self.result_source_loans
                    .get(&fact.binding())
                    .map(Vec::as_slice),
                self.inputs.span,
            )?;
            self.body.auxiliary_sequence_plans.retire(
                fact.binding(),
                point,
                loan,
                self.result_source_loans
                    .get(&fact.binding())
                    .map(Vec::as_slice),
                self.inputs.span,
            )?;
            self.body.interleaved_aux_plans.retire(
                fact.binding(),
                point,
                loan,
                self.result_source_loans
                    .get(&fact.binding())
                    .map(Vec::as_slice),
                self.inputs.span,
            )?;
            self.body.multi_binding_plans.retire(
                fact.binding(),
                point,
                loan,
                self.result_source_loans
                    .get(&fact.binding())
                    .map(Vec::as_slice),
                self.inputs.span,
            )?;
            self.body.child_plans.retire(
                fact.binding(),
                point,
                loan,
                self.result_source_loans
                    .get(&fact.binding())
                    .map(Vec::as_slice),
                self.inputs.span,
            )?;
            let Some(loan) = loan else {
                continue;
            };
            let span = self
                .inputs
                .owned
                .borrow_results()
                .bindings()
                .iter()
                .find(|b| b.binding() == fact.binding())
                .map_or(self.inputs.span, |b| b.marker_span());
            let view = if transfer {
                self.range_views.get(&fact.binding()).copied()
            } else {
                self.range_views.remove(&fact.binding())
            };
            if let Some(view) = view {
                self.push(ROp::RangeEnd { view, source: loan }, Vec::new(), span)?;
            } else {
                self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
            }
            let sources = if transfer {
                self.result_source_loans.get(&fact.binding()).cloned()
            } else {
                self.result_source_loans.remove(&fact.binding())
            };
            if let Some(sources) = sources {
                for source in sources.into_iter().rev() {
                    self.push(ROp::BorrowEnd { loan: source }, Vec::new(), span)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn named_argument_loan(
        &mut self,
        call: UnitExpressionId,
        value_expression: ExpressionId,
        value: Slot,
        parameter_type: UnitTypeId,
        call_arg_span: Span,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let value = self.adapt_borrowed(value, value_expression, parameter_type)?;
        self.body.later_aux_plans.later_operand(
            call,
            self.unit(value_expression),
            value,
            &self.bindings,
            span,
        )?;
        self.body.auxiliary_sequence_plans.sequence_operand(
            call,
            self.unit(value_expression),
            value,
            &self.bindings,
            span,
        )?;
        self.body.interleaved_aux_plans.sequence_operand(
            call,
            self.unit(value_expression),
            value,
            &self.bindings,
            (&self.borrowed, &self.result_source_loans),
            span,
        )?;
        self.body.multi_binding_plans.sequence_operand(
            call,
            self.unit(value_expression),
            value,
            &self.bindings,
            (&self.borrowed, &self.result_source_loans),
            span,
        )?;
        self.body.named_plans.enter(
            call,
            self.unit(value_expression),
            value,
            &self.bindings,
            span,
        )?;
        self.body.temporary_plans.enter(
            call,
            self.unit(value_expression),
            value,
            &self.temporaries,
            span,
        )?;
        let target = self.body.slots[value.0];
        let EntityType::Value(target) = target else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        // The loan originates at the borrowed argument; it ends at the call.
        let argument_span = self.span(value_expression);
        let place = self.value(
            ROp::RootPlace { owner: value },
            EntityType::Place(target),
            call_arg_span,
        )?;
        let loan = self.value(
            ROp::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            },
            argument_span,
        )?;
        Ok(loan)
    }

    pub(super) fn statement(&mut self, statement: StatementId) -> Result<Lowered, LoweringError> {
        let id = UnitStatementId::new(self.inputs.source_unit, statement);
        self.child_entry(id)?;
        if self.body.binding_plans.has_entry(id) {
            self.binding_quiescent(self.inputs.span)?;
            if !self.dead.is_empty()
                || self.this_dead
                || self.bindings.values().any(|slot| {
                    let Some(EntityType::Value(ty)) = self.body.slots.get(slot.0) else {
                        return true;
                    };
                    self.types.ssa_types.get(ty.index())
                        != Some(&crate::ssa::model::SsaTypeKind::StringOwner)
                })
            {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let block = self.current(self.inputs.span)?;
            self.body
                .binding_plans
                .enter(id, &self.bindings, block, self.inputs.span)?;
        }
        if self.body.aux_binding_plans.has_entry(id) {
            self.binding_quiescent(self.inputs.span)?;
            if !self.dead.is_empty()
                || self.this_dead
                || self.bindings.values().any(|slot| {
                    let Some(EntityType::Value(ty)) = self.body.slots.get(slot.0) else {
                        return true;
                    };
                    self.types.ssa_types.get(ty.index())
                        != Some(&crate::ssa::model::SsaTypeKind::StringOwner)
                })
            {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let block = self.current(self.inputs.span)?;
            self.body
                .aux_binding_plans
                .enter(id, &self.bindings, block, self.inputs.span)?;
        }
        if self.body.distinct_aux_plans.has_entry(id) {
            self.binding_quiescent(self.inputs.span)?;
            if !self.dead.is_empty()
                || self.this_dead
                || self.bindings.values().any(|slot| {
                    let Some(EntityType::Value(ty)) = self.body.slots.get(slot.0) else {
                        return true;
                    };
                    self.types.ssa_types.get(ty.index())
                        != Some(&crate::ssa::model::SsaTypeKind::StringOwner)
                })
            {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let block = self.current(self.inputs.span)?;
            self.body
                .distinct_aux_plans
                .enter(id, &self.bindings, block, self.inputs.span)?;
        }
        if self.body.later_aux_plans.has_entry(id) {
            self.binding_quiescent(self.inputs.span)?;
            if !self.dead.is_empty()
                || self.this_dead
                || self.bindings.values().any(|slot| {
                    let Some(EntityType::Value(ty)) = self.body.slots.get(slot.0) else {
                        return true;
                    };
                    self.types.ssa_types.get(ty.index())
                        != Some(&crate::ssa::model::SsaTypeKind::StringOwner)
                })
            {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let block = self.current(self.inputs.span)?;
            self.body
                .later_aux_plans
                .enter(id, &self.bindings, block, self.inputs.span)?;
        }
        if self.body.auxiliary_sequence_plans.has_entry(id) {
            self.binding_quiescent(self.inputs.span)?;
            if !self.dead.is_empty()
                || self.this_dead
                || self.bindings.values().any(|slot| {
                    let Some(EntityType::Value(ty)) = self.body.slots.get(slot.0) else {
                        return true;
                    };
                    self.types.ssa_types.get(ty.index())
                        != Some(&crate::ssa::model::SsaTypeKind::StringOwner)
                })
            {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let block = self.current(self.inputs.span)?;
            self.body.auxiliary_sequence_plans.enter(
                id,
                &self.bindings,
                block,
                self.inputs.span,
            )?;
        }
        if self.body.interleaved_aux_plans.has_entry(id) {
            self.binding_quiescent(self.inputs.span)?;
            if !self.dead.is_empty()
                || self.this_dead
                || self.bindings.values().any(|slot| {
                    let Some(EntityType::Value(ty)) = self.body.slots.get(slot.0) else {
                        return true;
                    };
                    self.types.ssa_types.get(ty.index())
                        != Some(&crate::ssa::model::SsaTypeKind::StringOwner)
                })
            {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let block = self.current(self.inputs.span)?;
            self.body
                .interleaved_aux_plans
                .enter(id, &self.bindings, block, self.inputs.span)?;
        }
        if self.body.multi_binding_plans.has_entry(id) {
            self.binding_quiescent(self.inputs.span)?;
            if !self.dead.is_empty()
                || self.this_dead
                || self.bindings.values().any(|slot| {
                    let Some(EntityType::Value(ty)) = self.body.slots.get(slot.0) else {
                        return true;
                    };
                    self.types.ssa_types.get(ty.index())
                        != Some(&crate::ssa::model::SsaTypeKind::StringOwner)
                })
            {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let block = self.current(self.inputs.span)?;
            self.body
                .multi_binding_plans
                .enter(id, &self.bindings, block, self.inputs.span)?;
        }
        let result = self.statement_inner(statement)?;
        let point = UnitDropPoint::AfterStatement(id);
        if result != Lowered::Diverged {
            self.drops(point)?;
        } else {
            self.unreachable_drop_points.push(point);
        }
        if self.body.binding_plans.has_completion(id) {
            self.binding_quiescent(self.inputs.span)?;
            if result == Lowered::Diverged {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let block = self.current(self.inputs.span)?;
            self.body.binding_plans.complete(
                id,
                &self.bindings,
                &self.dead,
                &self.consumed_drops,
                block,
                self.inputs.span,
            )?;
        }
        if self.body.aux_binding_plans.has_completion(id) {
            self.binding_quiescent(self.inputs.span)?;
            if result == Lowered::Diverged {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let block = self.current(self.inputs.span)?;
            self.body.aux_binding_plans.complete(
                id,
                &self.bindings,
                &self.dead,
                &self.consumed_drops,
                block,
                self.inputs.span,
            )?;
        }
        if self.body.distinct_aux_plans.has_completion(id) {
            self.binding_quiescent(self.inputs.span)?;
            if result == Lowered::Diverged {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let block = self.current(self.inputs.span)?;
            self.body.distinct_aux_plans.complete(
                id,
                &self.bindings,
                &self.dead,
                &self.consumed_drops,
                block,
                self.inputs.span,
            )?;
        }
        if self.body.later_aux_plans.has_completion(id) {
            self.binding_quiescent(self.inputs.span)?;
            if result == Lowered::Diverged {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let block = self.current(self.inputs.span)?;
            self.body.later_aux_plans.complete(
                id,
                &self.bindings,
                &self.dead,
                &self.consumed_drops,
                block,
                self.inputs.span,
            )?;
        }
        if self.body.auxiliary_sequence_plans.has_completion(id) {
            self.binding_quiescent(self.inputs.span)?;
            if result == Lowered::Diverged {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let block = self.current(self.inputs.span)?;
            self.body.auxiliary_sequence_plans.complete(
                id,
                &self.bindings,
                &self.dead,
                &self.consumed_drops,
                block,
                self.inputs.span,
            )?;
        }
        if self.body.interleaved_aux_plans.has_completion(id) {
            self.binding_quiescent(self.inputs.span)?;
            if result == Lowered::Diverged {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let block = self.current(self.inputs.span)?;
            self.body.interleaved_aux_plans.complete(
                id,
                &self.bindings,
                &self.dead,
                &self.consumed_drops,
                block,
                self.inputs.span,
            )?;
        }
        if self.body.multi_binding_plans.has_completion(id) {
            self.binding_quiescent(self.inputs.span)?;
            if result == Lowered::Diverged {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let block = self.current(self.inputs.span)?;
            self.body.multi_binding_plans.complete(
                id,
                &self.bindings,
                &self.dead,
                &self.consumed_drops,
                block,
                self.inputs.span,
            )?;
        }
        self.child_completion(id, result)?;
        self.fresh_source_completion(id, result)?;
        Ok(result)
    }
    pub(super) fn binding_quiescent(&self, span: Span) -> Result<(), LoweringError> {
        if !self.borrowed.is_empty()
            || !self.result_source_loans.is_empty()
            || !self.pending_call_loans.is_empty()
            || !self.range_views.is_empty()
            || !self.short_ranges.is_empty()
            || !self.capture_loans.is_empty()
            || !self.pending_views.is_empty()
            || !self.active_views.is_empty()
            || !self.loops.is_empty()
            || !self.temporaries.is_empty()
            || !self.narrowed.is_empty()
            || self.receiver.is_some()
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(())
    }

    pub(super) fn binding_consumer_argument(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        loan: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.fresh_source_argument(call, argument, loan, span)?;
        if !self.body.binding_plans.has_consumer(call)
            && !self.body.aux_binding_plans.has_consumer(call)
            && !self.body.distinct_aux_plans.has_consumer(call)
            && !self.body.later_aux_plans.has_consumer(call)
            && !self.body.auxiliary_sequence_plans.has_consumer(call)
            && !self.body.interleaved_aux_plans.has_consumer(call)
            && !self.body.multi_binding_plans.has_consumer(call)
        {
            return Ok(());
        }
        let (Expression::Name, name_span) = self.node(argument)? else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        // Resolve this actual Name through existing frontend names, never its canonical owner root.
        let binding = self.reference(name_span)?;
        if self.borrowed.get(&binding).copied() != Some(loan) {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        self.body.binding_plans.consumer_argument(
            call,
            self.unit(argument),
            binding,
            loan,
            span,
        )?;
        self.body.aux_binding_plans.consumer_argument(
            call,
            self.unit(argument),
            binding,
            loan,
            span,
        )?;
        self.body.distinct_aux_plans.consumer_argument(
            call,
            self.unit(argument),
            binding,
            loan,
            span,
        )?;
        self.body.later_aux_plans.consumer_argument(
            call,
            self.unit(argument),
            binding,
            loan,
            span,
        )?;
        self.body.auxiliary_sequence_plans.consumer_argument(
            call,
            self.unit(argument),
            binding,
            loan,
            span,
        )?;
        self.body.interleaved_aux_plans.consumer_argument(
            call,
            self.unit(argument),
            binding,
            loan,
            span,
        )?;
        self.body.multi_binding_plans.consumer_argument(
            call,
            self.unit(argument),
            binding,
            loan,
            span,
        )
    }

    /// `Box(payload)` allocates a fresh heap box owner for the single payload argument.
    pub(super) fn box_construction(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .construction(id)
            .filter(|descriptor| descriptor.expression() == id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.target() != UnitConstructionTarget::IntrinsicBox {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let [argument] = descriptor.arguments() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if argument.argument().source_unit() != self.inputs.source_unit {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let value_expression = argument.argument().expression();
        let Some(value) = self.required(value_expression)? else {
            return Ok(Lowered::Diverged);
        };
        let payload_ty = self.c(argument.parameter_type());
        let mut payload = self.adapt(value, value_expression, payload_ty)?;
        let payload_ssa = self.ssa_type(payload_ty, span)?;
        if argument.category() == lang_frontend::type_checking::ExpressionCategory::Place
            && !self.move_only(payload_ty)
        {
            payload = self.value(
                ROp::Copy { source: payload },
                EntityType::Value(payload_ssa),
                self.span(value_expression),
            )?;
        } else {
            self.deliver(value_expression);
        }
        let owner = self.ssa_type(self.c(descriptor.result_type()), span)?;
        Ok(Lowered::Value(self.value(
            ROp::HeapAllocate { owner, payload },
            EntityType::Value(owner),
            span,
        )?))
    }

    /// Fields evaluate in source order and land in declaration order; the payload is then
    /// built and moved into a fresh heap owner.
    pub(super) fn construction(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .construction(id)
            .filter(|descriptor| descriptor.expression() == id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let UnitConstructionTarget::Nominal(_) = descriptor.target() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let mut arguments = descriptor.arguments().iter().collect::<Vec<_>>();
        arguments.sort_by_key(|argument| argument.evaluation_index());
        let mut fields: Vec<Option<Slot>> = vec![None; arguments.len()];
        let mut delivered = Vec::with_capacity(arguments.len());
        for argument in arguments {
            if argument.argument().source_unit() != self.inputs.source_unit {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let value_expression = argument.argument().expression();
            let Some(value) = self.required(value_expression)? else {
                return Ok(Lowered::Diverged);
            };
            let value = self.adapt(value, value_expression, self.c(argument.parameter_type()))?;
            self.repoint_temporary(value_expression, value);
            delivered.push(value_expression);
            let slot = fields
                .get_mut(argument.parameter_index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if slot.replace(value).is_some() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        for value_expression in delivered {
            self.deliver(value_expression);
        }
        let fields = fields
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let owner = self.ssa_type(self.c(descriptor.result_type()), span)?;
        let aggregate = self
            .types
            .heap_payloads
            .get(&owner)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let payload = self.value(
            ROp::AggregateConstruct { aggregate, fields },
            EntityType::Value(aggregate),
            span,
        )?;
        Ok(Lowered::Value(self.value(
            ROp::HeapAllocate { owner, payload },
            EntityType::Value(owner),
            span,
        )?))
    }

    /// A Copyable field read through a shared loan: the receiver's proof view, the loan a
    /// parameter or the current receiver arrived as, or a loan of the owned binding that
    /// ends right after the read.
    pub(super) fn projection(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let projection = self
            .inputs
            .typed
            .aggregate_projection(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let result_type = self.ssa_type(self.expression_type(expression)?, span)?;
        let is_nested = if let UnitAggregateProjectionReceiver::Expression(receiver) =
            projection.receiver()
            && receiver.source_unit() == self.inputs.source_unit
        {
            let mut ungrouped = receiver.expression();
            while let (Expression::Group { expression: inner }, _) = self.node(ungrouped)? {
                ungrouped = inner;
            }
            self.inputs
                .typed
                .aggregate_projection(self.unit(ungrouped))
                .is_some()
        } else {
            false
        };
        if is_nested {
            let (leaf_loan, created, drops) = self.field_projection_loan(expression, span)?;
            let result = self.value(
                ROp::ReadLoan { loan: leaf_loan },
                EntityType::Value(result_type),
                span,
            )?;
            for loan in created.into_iter().rev() {
                self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
            }
            for receiver_expr in drops {
                self.drops(UnitDropPoint::AfterExpression(self.unit(receiver_expr)))?;
            }
            return Ok(Lowered::Value(result));
        }
        let (owner, owner_type, receiver) = self.projection_owner(projection.receiver(), span)?;
        let field = self
            .types
            .field_indices
            .get(&(owner_type, projection.field()))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let rc_payload = receiver.is_some_and(|receiver| {
            self.inputs
                .typed
                .rc_operation(self.unit(receiver))
                .is_some()
        });
        let view = match receiver {
            Some(receiver) if !rc_payload => self.narrowed_view(receiver)?,
            _ => None,
        };
        let (loan, rc_view_read) = match (view, receiver) {
            (Some(view), _) => (
                OwnerLoan {
                    loan: view,
                    scoped: false,
                },
                None,
            ),
            // `rc.value.f`: a shared loan of the payload place (SPEC-0359).
            (None, Some(receiver)) if rc_payload => self.rc_payload_loan(receiver, span)?,
            (None, _) => (self.owner_loan(owner, LoanKind::Shared, span)?, None),
        };
        let result = self.value(
            ROp::HeapFieldRead {
                receiver: loan.loan,
                field,
            },
            EntityType::Value(result_type),
            span,
        )?;
        self.end_owner_loan(loan, span)?;
        // The receiver is read in place; its own drop point follows the read.
        if let Some(read) = rc_view_read {
            self.drops(UnitDropPoint::AfterExpression(self.unit(read)))?;
        }
        if let Some(receiver) = receiver {
            self.drops(UnitDropPoint::AfterExpression(self.unit(receiver)))?;
        }
        Ok(Lowered::Value(result))
    }

    /// The owner a class field projection goes through, its (narrowed) class type and the
    /// explicit receiver expression, if any.
    pub(super) fn projection_owner(
        &self,
        receiver: UnitAggregateProjectionReceiver,
        span: Span,
    ) -> Result<(OwnerRef, UnitTypeId, Option<ExpressionId>), LoweringError> {
        match receiver {
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                if receiver.source_unit() != self.inputs.source_unit {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let receiver = receiver.expression();
                let owner_type = match self.inputs.typed.non_null_use(self.unit(receiver)) {
                    Some(fact) => self.c(fact.narrowed_type()),
                    None => self.expression_type(receiver)?,
                };
                Ok((OwnerRef::Expression(receiver), owner_type, Some(receiver)))
            }
            UnitAggregateProjectionReceiver::This(declaration) => {
                let receiver = self
                    .inputs
                    .receiver
                    .filter(|receiver| receiver.owner == declaration)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                Ok((OwnerRef::This(declaration), receiver.ty, None))
            }
        }
    }

    /// `operand!!`: one evaluation, a null edge that aborts and a non-null edge that takes
    /// the inner owner (or `MapResultUnwrap` for Copyable `$map.result.tX`), consuming a
    /// MoveOnly operand.
    pub(super) fn non_null_assertion(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .non_null_assertion(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let plan = self
            .inputs
            .owned
            .non_null_assertion(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if plan.descriptor() != &descriptor
            || descriptor.operand().source_unit() != self.inputs.source_unit
            || !matches!(
                plan.non_null_transfer(),
                NonNullAssertionTransferKind::Consume | NonNullAssertionTransferKind::Copy
            )
            || !matches!(
                self.inputs.typed.types().get(self.c(descriptor.nullable_type())),
                Some(UnitTypeKind::Nullable(inner)) if *inner == self.c(descriptor.inner_type())
            )
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let operand = descriptor.operand().expression();
        let owner = match self.expression_inner(operand)? {
            Lowered::Value(owner) => owner,
            Lowered::Diverged => return Ok(Lowered::Diverged),
            Lowered::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        let nullable_ssa = self.ssa_type(self.c(descriptor.nullable_type()), span)?;
        let inner = self.ssa_type(self.c(descriptor.inner_type()), span)?;
        if plan.non_null_transfer() == NonNullAssertionTransferKind::Copy
            && self.types.map_results.contains_key(&nullable_ssa)
        {
            let value = self.value(
                ROp::MapResultUnwrap { result: owner },
                EntityType::Value(inner),
                span,
            )?;
            self.drops(UnitDropPoint::AfterExpression(self.unit(operand)))?;
            return Ok(Lowered::Value(value));
        }
        self.register_temporary(operand, owner)?;
        let mut root = operand;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let (payload, operand_span) = self.node(root)?;
        if plan.non_null_transfer() == NonNullAssertionTransferKind::Consume
            && let Expression::Name = payload
        {
            // The owner is consumed: no proof view of it may stay live.
            let symbol = self.reference(operand_span)?;
            self.end_views_of(symbol, span)?;
        }
        let null_block = self.body.block();
        let non_null_block = self.body.block();
        let view = self.body.slot(EntityType::Loan {
            kind: LoanKind::Shared,
            target: inner,
        });
        self.body.blocks[non_null_block.0].view = Some(view);
        self.terminate(
            RTerm::NullBranch {
                owner,
                when_null: null_block,
                when_non_null: non_null_block,
            },
            span,
        )?;
        self.block = Some(null_block);
        self.terminate(RTerm::Abort(RAbort::NullAssertion), span)?;
        self.block = Some(non_null_block);
        let value = self.value(
            ROp::NullableTake { owner, proof: view },
            EntityType::Value(inner),
            span,
        )?;
        if plan.non_null_transfer() == NonNullAssertionTransferKind::Consume {
            self.deliver(operand);
        }
        self.drops(UnitDropPoint::AfterExpression(self.unit(operand)))?;
        Ok(Lowered::Value(value))
    }
}
