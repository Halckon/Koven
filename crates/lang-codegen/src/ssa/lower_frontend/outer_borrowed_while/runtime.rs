//! Local admission, exact snapshot restoration and consumption of the two real drops.
use super::super::control::BranchExit;
use super::carrier::{Columns, instructions, transport};
use super::*;
use crate::ssa::model::SsaTypeKind;
use lang_frontend::{
    ownership_checking::IterationExitPlan, type_checking::NullComparisonDescriptor,
};

impl ExpressionLowerer<'_> {
    fn outer_borrowed_relation(&self, outer: StatementId) -> Option<Relation> {
        let mut relations = self
            .owned
            .drops()
            .iter()
            .copied()
            .filter_map(|fact| candidate(self.parsed, self.names, self.typed, self.owned, fact))
            .filter(|relation| relation.outer == outer);
        let relation = relations.next()?;
        relations.all(|other| other == relation).then_some(relation)
    }

    pub(in super::super) fn qualify_outer_borrowed_entry(
        &self,
        inner: StatementId,
        descriptor: NullComparisonDescriptor,
        operand: ExpressionId,
        span: Span,
    ) -> Result<Option<Relation>, LoweringError> {
        if !self.loops.iter().any(|context| context.for_loop.is_some()) {
            return Ok(None);
        }
        let outer = self
            .loops
            .first()
            .and_then(|context| context.for_loop.as_ref())
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?
            .statement;
        let relation = self
            .outer_borrowed_relation(outer)
            .filter(|relation| relation.inner == inner)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        if self.loops.len() != 1
            || self.typed.non_null_use(operand).is_some()
            || self.typed.expression_type(operand) != Some(relation.storage)
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_outer_borrowed_entry(&relation, descriptor, span)?;
        Ok(Some(relation))
    }

    pub(in super::super) fn validate_outer_borrowed_entry(
        &self,
        relation: &Relation,
        descriptor: NullComparisonDescriptor,
        span: Span,
    ) -> Result<(), LoweringError> {
        if descriptor.symbol() != relation.root
            || descriptor.nullable_type() != relation.storage
            || !descriptor.non_null_when_true()
            || !self.non_null_bindings.is_empty()
            || self.resolve_type(relation.storage, span)? != relation.storage
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_active_outer_borrowed_source(relation, span)?;
        self.validate_outer_borrowed_state(
            relation,
            &self.branch_exit(LoweredValue::Unit),
            false,
            span,
        )
    }

    fn validate_active_outer_borrowed_source(
        &self,
        relation: &Relation,
        span: Span,
    ) -> Result<(), LoweringError> {
        let data = provider::context(self, relation)
            .and_then(|context| context.for_loop.as_ref())
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        if data.active_source_loan != Some(data.body_source)
            || self.borrow_bindings.get(&relation.item) != Some(&data.guarded_element.loan())
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    fn validate_outer_borrowed_state(
        &self,
        relation: &Relation,
        state: &BranchExit,
        allow_view: bool,
        span: Span,
    ) -> Result<(), LoweringError> {
        let provider = provider::shape(self, relation)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let guard = provider::guard(self, relation, &provider);
        let source = *state
            .for_sources
            .get(&relation.outer.index())
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let element = *state
            .for_elements
            .get(&relation.outer.index())
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let owner = |symbol| match state.bindings.get(&symbol) {
            Some(LoweredValue::Value(value)) => Some(*value),
            _ => None,
        };
        let current =
            owner(relation.root).ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let items =
            owner(relation.items).ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let mut columns = Columns {
            block: state.block,
            entities: vec![
                EntityId::Value(current),
                EntityId::Value(items),
                EntityId::Loan(source),
                EntityId::Loan(element),
            ],
        };
        if state.for_sources.len() != 1 || state.for_elements.len() != 1
            || state.borrow_bindings.get(&relation.item) != Some(&element)
            || !state.temporaries.is_empty() || !state.loans.is_empty() || !state.capture_loans.is_empty()
            || !state.result_source_loans.is_empty() || self.readonly_receiver.is_some() || self.thunk_expression.is_some()
            || state.borrow_bindings.iter().any(|(&symbol, &loan)| symbol != relation.item
                && (!self.is_entry_loan(loan) || !matches!(self.function.entity(EntityId::Loan(loan)).map(|entity| entity.ty),
                    Some(EntityType::Loan { kind: LoanKind::Shared, target }) if self.ssa_types.get(target.index()) == Some(&SsaTypeKind::Boolean))))
            || !transport(self.function, columns.clone(), &provider.body, None, guard.as_ref())
        { return Err(error(LoweringErrorKind::UnsupportedNode, span)); }
        if allow_view {
            let guard = guard.ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            let proof = state
                .views
                .get(&relation.root)
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            self.validate_non_null_proof(relation.root, *proof, &state.bindings, span)?;
            columns.entities.push(EntityId::Loan(proof.loan));
            if state.views.len() != 1
                || proof.while_origin() != Some((relation.inner, guard.view))
                || !transport(self.function, columns, &guard.non_null, None, None)
                || self
                    .function
                    .block(state.block)
                    .is_none_or(|block| block.terminator.is_some())
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
        } else if !state.views.is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    /// Validate the snapshot before touching ambient sibling state. The exact
    /// proven source is live; rebind_source cannot revive an already ended loan.
    pub(in super::super) fn restore_outer_borrowed_state(
        &mut self,
        relation: &Relation,
        state: &BranchExit,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_outer_borrowed_state(relation, state, false, span)?;
        let source = state.for_sources[&relation.outer.index()];
        let element = state.for_elements[&relation.outer.index()];
        let data = self
            .loops
            .first_mut()
            .and_then(|context| context.for_loop.as_mut())
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        if data.statement != relation.outer
            || data.new_source_loan.is_none()
            || data.is_carried_source
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        data.rebind_source(source);
        data.guarded_element.set_loan(element);
        Ok(())
    }

    pub(in super::super) fn validate_outer_borrowed_exit_states(
        &self,
        relation: &Relation,
        states: &[BranchExit],
        span: Span,
    ) -> Result<(), LoweringError> {
        for state in states {
            self.validate_outer_borrowed_state(relation, state, false, span)?;
        }
        Ok(())
    }

    pub(in super::super) fn retire_outer_borrowed_break(
        &mut self,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let Some(relation) = self
            .loops
            .last()
            .and_then(|context| context.outer_borrowed_while.clone())
        else {
            return Ok(false);
        };
        if self.loops.len() != 2
            || self
                .parsed
                .ast()
                .expressions()
                .get(relation.breaking)
                .map(|node| node.span())
                .ok()
                != Some(span)
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_active_outer_borrowed_source(&relation, span)?;
        self.validate_outer_borrowed_state(
            &relation,
            &self.branch_exit(LoweredValue::Unit),
            true,
            span,
        )?;
        let proof = self.non_null_bindings[&relation.root];
        self.append(Operation::BorrowEnd { loan: proof.loan }, Vec::new(), span)?;
        self.non_null_bindings.remove(&relation.root);
        Ok(true)
    }

    pub(in super::super) fn preflight_outer_borrowed_iteration(
        &self,
        exit: &IterationExitPlan,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(data) = self
            .loops
            .last()
            .and_then(|context| context.for_loop.as_ref())
        else {
            return Ok(());
        };
        let Some(relation) = self.outer_borrowed_relation(data.statement) else {
            return Ok(());
        };
        if self.loops.len() != 1 || !data.plan.exits().contains(exit) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        if exit.kind() == IterationExitKind::Fallthrough {
            self.validate_active_outer_borrowed_source(&relation, span)?;
            return self.validate_outer_borrowed_state(
                &relation,
                &self.branch_exit(LoweredValue::Unit),
                false,
                span,
            );
        }
        if exit.kind() != IterationExitKind::Exhaustion {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let provider = self.checked_outer_borrowed_exhaustion(&relation, span)?;
        if data.active_source_loan != provider::loan(provider.exhausted.entities[2])
            || !instructions(self.function, self.block).is_some_and(|ops| ops.is_empty())
            || self.bindings.get(&relation.items)
                != provider::value(provider.exhausted.entities[1])
                    .as_ref()
                    .map(|owner| LoweredValue::Value(*owner))
                    .as_ref()
            || self.bindings.get(&relation.root)
                != provider::value(provider.exhausted.entities[0])
                    .as_ref()
                    .map(|owner| LoweredValue::Value(*owner))
                    .as_ref()
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    fn checked_outer_borrowed_exhaustion(
        &self,
        relation: &Relation,
        span: Span,
    ) -> Result<provider::Provider, LoweringError> {
        let provider = provider::shape(self, relation)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        if self.loops.len() != 1 || self.block != provider.exhausted.block
            || !self.non_null_bindings.is_empty() || !self.temporaries.is_empty() || !self.pending_call_loans.is_empty()
            || !self.capture_loans.is_empty() || !self.result_source_loans.is_empty() || self.readonly_receiver.is_some() || self.thunk_expression.is_some()
            || self.borrow_bindings.contains_key(&relation.item)
            || self.borrow_bindings.values().any(|&loan| !self.is_entry_loan(loan) || !matches!(self.function.entity(EntityId::Loan(loan)).map(|entity| entity.ty),
                Some(EntityType::Loan { kind: LoanKind::Shared, target }) if self.ssa_types.get(target.index()) == Some(&SsaTypeKind::Boolean)))
            || self.function.block(self.block).is_none_or(|block| block.terminator.is_some())
            || provider::completed_backedge(self, relation, &provider).is_none()
        { return Err(error(LoweringErrorKind::UnsupportedNode, span)); }
        Ok(provider)
    }

    pub(in super::super) fn emit_outer_borrowed_drop(
        &mut self,
        fact: DropFact,
    ) -> Result<bool, LoweringError> {
        let Some(relation) = candidate(self.parsed, self.names, self.typed, self.owned, fact)
        else {
            return Ok(false);
        };
        let span = fact.value_origin();
        let provider = self.checked_outer_borrowed_exhaustion(&relation, span)?;
        let source = provider::loan(provider.exhausted.entities[2])
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let items = provider::value(provider.exhausted.entities[1])
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let current = provider::value(provider.exhausted.entities[0])
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let outer_span = self
            .parsed
            .ast()
            .statements()
            .get(relation.outer)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?
            .span();
        let ops = instructions(self.function, self.block)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let expected = if fact == relation.drops[0] {
            vec![Operation::BorrowEnd { loan: source }]
        } else {
            vec![
                Operation::BorrowEnd { loan: source },
                Operation::Drop { owner: items },
            ]
        };
        if self.loops[0]
            .for_loop
            .as_ref()
            .is_none_or(|data| data.active_source_loan.is_some())
            || ops.iter().map(|op| &op.operation).ne(expected.iter())
            || ops
                .first()
                .is_none_or(|op| op.origin != Origin::Source(outer_span))
            || (fact == relation.drops[1]
                && (self.bindings.contains_key(&relation.items)
                    || ops[1].origin != Origin::Source(relation.drops[0].value_origin())))
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let (symbol, owner) = if fact == relation.drops[0] {
            (relation.items, items)
        } else {
            (relation.root, current)
        };
        if self.bindings.get(&symbol) != Some(&LoweredValue::Value(owner)) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.bindings.remove(&symbol);
        self.append(Operation::Drop { owner }, Vec::new(), span)?;
        self.release_owner_capture_loans(owner, span)?;
        Ok(true)
    }
}
