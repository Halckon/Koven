//! Local provider admission; final cleanup rechecks the fully emitted CFG.
use super::carrier::{Birth, Columns, birth, parameter, transport};
use super::*;
use crate::ssa::model::SsaTypeKind;
use lang_frontend::ownership_checking::IterationExitPlan;

impl ExpressionLowerer<'_> {
    fn borrowed_while_plan(
        &self,
        statement: StatementId,
        span: Span,
    ) -> Result<Plan, LoweringError> {
        unique(
            self.owned
                .drops()
                .iter()
                .copied()
                .filter_map(|fact| candidate(self.parsed, self.names, self.typed, self.owned, fact))
                .filter(|plan| plan.statement == statement),
        )
        .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))
    }

    /// The initial gate is strict: this provider has not created any loans yet.
    pub(in super::super) fn qualify_borrowed_while_entry(
        &self,
        statement: StatementId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if !self
            .non_null_bindings
            .values()
            .any(|proof| proof.is_while_origin())
        {
            return Ok(());
        }
        let plan = self.borrowed_while_plan(statement, span)?;
        self.ensure_no_unresolved_while_proof_carriers(span)?;
        let (proof, birth) = self.borrowed_while_pair(&plan, span)?;
        if self.loops.len() != 1
            || self.block != birth.pair.block
            || birth.pair.entities != [EntityId::Value(proof.owner), EntityId::Loan(proof.loan)]
            || self.bindings.get(&plan.source_root)
                != Some(&LoweredValue::Value(birth.source_owner))
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    fn borrowed_while_pair(
        &self,
        plan: &Plan,
        span: Span,
    ) -> Result<(NonNullProof, Birth), LoweringError> {
        let proof = self
            .non_null_bindings
            .get(&plan.root)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        self.validate_readonly_while_exit(
            plan.root,
            proof,
            &self.branch_exit(LoweredValue::Unit),
            span,
        )?;
        let birth = birth(self, plan, proof)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        if !self.temporaries.is_empty()
            || !self.pending_call_loans.is_empty()
            || !self.capture_loans.is_empty()
            || !self.result_source_loans.is_empty()
            || self.readonly_receiver.is_some()
            || self.thunk_expression.is_some()
            || self.typed.symbol_type(plan.root) != Some(plan.storage)
            || self.resolve_type(plan.storage, span)? != plan.storage
            || self
                .function
                .block(self.block)
                .is_none_or(|block| block.terminator.is_some())
            || !transport(
                self.function,
                Columns {
                    block: self.block,
                    entities: vec![EntityId::Value(proof.owner), EntityId::Loan(proof.loan)],
                },
                &birth.pair,
                None,
            )
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok((proof, birth))
    }

    /// Do not relax the generic no-carrier gate. Only this exact active provider
    /// may keep its independently rooted source and its own element alive.
    pub(in super::super) fn preflight_borrowed_while_exit(
        &self,
        exit: &IterationExitPlan,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(context) = self.loops.last() else {
            return Ok(());
        };
        let Some(data) = &context.for_loop else {
            return Ok(());
        };
        if !context
            .entry_views
            .values()
            .any(|proof| proof.is_while_origin())
        {
            return Ok(());
        }
        let plan = self.borrowed_while_plan(data.statement, span)?;
        let (proof, birth) = self.borrowed_while_pair(&plan, span)?;
        let provider = provider::shape(self, &plan, &birth)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let inherited = context
            .entry_views
            .get(&plan.root)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let header_source = provider::loan(provider.header.entities[3])
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let header_column = |entity| parameter(self.function, entity).map(|(_, index)| index);
        if self.loops.len() != 2
            || !context.breaks.is_empty()
            || !context.continues.is_empty()
            || context.entry_views.len() != 1
            || proof.origin != inherited.origin
            || proof.inner != inherited.inner
            || !data.plan.exits().contains(exit)
            || data.statement != plan.statement
            || context.header != provider.header.block
            || data.new_source_loan != Some(provider.source)
            || data.is_carried_source
            || data.provider_header.block != provider.header.block
            || data.provider_header.source != header_source
            || data.provider_header.length != provider.length
            || data.provider_header.cursor != provider.cursor
            || data.provider_header.step != provider.step
            || header_column(EntityId::Loan(header_source)) != Some(data.source_slot)
            || header_column(EntityId::Value(provider.length)) != Some(data.length_slot)
            || header_column(EntityId::Value(provider.cursor)) != Some(data.cursor_slot)
            || data.provider_header.source_slot != data.source_slot
            || data.provider_header.length_slot != data.length_slot
            || data.provider_header.cursor_slot != data.cursor_slot
            || data.body_cursor != provider.body_cursor
            || data.guarded_element.cursor() != provider.body_cursor
            || data.guarded_element.body() != provider.body.block
            || data.guarded_element.header() != provider.header.block
            || Some(data.guarded_element.source()) != provider::loan(provider.body.entities[3])
            || data.guarded_element.step() != provider.step
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let exhausted = exit.kind() == IterationExitKind::Exhaustion;
        let element = data.guarded_element.loan();
        for (&symbol, &loan) in &self.borrow_bindings {
            if !exhausted && symbol == plan.element && loan == element {
                continue;
            }
            if !self.is_entry_loan(loan)
                || !matches!(self.function.entity(EntityId::Loan(loan)).map(|entity| entity.ty),
                Some(EntityType::Loan { kind: LoanKind::Shared, target })
                    if matches!(self.ssa_types.get(target.index()), Some(SsaTypeKind::Boolean)))
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
        }
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&plan.source_root).copied() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        if exhausted {
            let ended = provider::backedges(self, &plan, &birth, &provider)
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            if self.block != provider.exhausted.block
                || provider.exhausted.entities[..3]
                    != [
                        EntityId::Value(proof.owner),
                        EntityId::Loan(proof.loan),
                        EntityId::Value(owner),
                    ]
                || data.active_source_loan != provider::loan(provider.exhausted.entities[3])
                || self.borrow_bindings.contains_key(&plan.element)
                || !ended.contains(&(data.body_source, element))
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
        } else {
            if !matches!(
                exit.kind(),
                IterationExitKind::Continue(_) | IterationExitKind::Fallthrough
            ) || data.active_source_loan != Some(data.body_source)
                || self.borrow_bindings.get(&plan.element) != Some(&element)
                || !transport(
                    self.function,
                    Columns {
                        block: self.block,
                        entities: vec![
                            EntityId::Value(proof.owner),
                            EntityId::Loan(proof.loan),
                            EntityId::Value(owner),
                            EntityId::Loan(data.body_source),
                            EntityId::Loan(element),
                        ],
                    },
                    &provider.body,
                    None,
                )
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
        }
        Ok(())
    }

    pub(in super::super) fn preflight_nullable_borrowed_cleanup(
        &self,
        facts: &[DropFact],
    ) -> Result<(), LoweringError> {
        for &fact in facts {
            if let Some(plan) = candidate(self.parsed, self.names, self.typed, self.owned, fact) {
                self.checked_nullable_borrowed_owner(&plan)?;
            }
        }
        Ok(())
    }

    pub(in super::super) fn emit_nullable_borrowed_cleanup(
        &mut self,
        fact: DropFact,
    ) -> Result<bool, LoweringError> {
        let Some(plan) = candidate(self.parsed, self.names, self.typed, self.owned, fact) else {
            return Ok(false);
        };
        let owner = self.checked_nullable_borrowed_owner(&plan)?;
        self.end_non_null_proof(plan.root, fact.value_origin())?;
        if self.bindings.remove(&plan.root) != Some(LoweredValue::Value(owner)) {
            return Err(error(LoweringErrorKind::MissingFact, fact.value_origin()));
        }
        self.append(Operation::Drop { owner }, Vec::new(), fact.value_origin())?;
        self.release_owner_capture_loans(owner, fact.value_origin())?;
        Ok(true)
    }

    fn checked_nullable_borrowed_owner(&self, plan: &Plan) -> Result<ValueId, LoweringError> {
        let span = plan.fact.value_origin();
        self.ensure_no_unresolved_while_proof_carriers(span)?;
        let (proof, birth) = self.borrowed_while_pair(plan, span)?;
        let provider = provider::shape(self, plan, &birth)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        if self.loops.len() != 1
            || provider.exhausted.entities[..2]
                != [EntityId::Value(proof.owner), EntityId::Loan(proof.loan)]
            || provider::complete(self, plan, &birth, &provider).is_none()
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(proof.owner)
    }
}
