//! Current-block identity for an existing nullable branch proof, plus ASAP drop and call-prefix cleanup.
use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
    ownership_checking::{DropPoint, DropTarget, LoanEndPoint},
    source::Span,
};

use super::control::BranchExit;
use super::{
    EntityId, EntityType, ExpressionLowerer, LoanId, LoanKind, LoweredValue, LoweringError,
    LoweringErrorKind, Operation, SsaTypeId, ValueId, error,
};
use crate::ssa::model::{Ownership, SsaTypeKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProofScope {
    If(ExpressionId),
    When(ExpressionId),
    While(StatementId),
    Assertion(ExpressionId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ProofOrigin {
    birth: LoanId,
    introduced_by: ProofScope,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NonNullProof {
    pub(super) loan: LoanId,
    pub(super) owner: ValueId,
    pub(super) inner: SsaTypeId,
    pub(super) origin: ProofOrigin,
}

impl NonNullProof {
    pub(super) fn introduced(
        loan: LoanId,
        owner: ValueId,
        inner: SsaTypeId,
        introduced_by: ProofScope,
    ) -> Self {
        Self {
            loan,
            owner,
            inner,
            origin: ProofOrigin {
                birth: loan,
                introduced_by,
            },
        }
    }

    pub(super) fn while_origin(self) -> Option<(StatementId, LoanId)> {
        match self.origin.introduced_by {
            ProofScope::While(statement) => Some((statement, self.origin.birth)),
            _ => None,
        }
    }

    pub(super) fn if_origin(self) -> Option<(ExpressionId, LoanId)> {
        match self.origin.introduced_by {
            ProofScope::If(expression) => Some((expression, self.origin.birth)),
            _ => None,
        }
    }

    pub(super) fn is_while_origin(self) -> bool {
        matches!(self.origin.introduced_by, ProofScope::While(_))
    }
}

impl ExpressionLowerer<'_> {
    pub(super) fn finish_borrowed_call(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.finish_borrowed_call_with_sources(expression, span, Vec::new())
    }

    pub(super) fn finish_borrowed_call_with_sources(
        &mut self,
        expression: ExpressionId,
        span: Span,
        sources: Vec<LoanId>,
    ) -> Result<(), LoweringError> {
        let ending_loans = self.owned.loans_ending_at(expression).collect::<Vec<_>>();
        for fact in &ending_loans {
            self.end_pending_source_loan(
                (expression.index(), fact.argument().index()),
                fact.end_span(),
            )?;
        }
        if self
            .pending_call_loans
            .keys()
            .any(|(call, _)| *call == expression.index())
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        for loan in sources.into_iter().rev() {
            self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
        }
        self.finish_short_call_ranges(expression, span)?;
        for fact in ending_loans {
            self.emit_drops(DropPoint::AfterExpression(fact.argument()))?;
        }
        self.emit_drops(DropPoint::CallReturn(expression))
    }

    pub(super) fn emit_control_transfer_cleanup(
        &mut self,
        transfer: ExpressionId,
    ) -> Result<(), LoweringError> {
        let span = self.expression_span(transfer)?;
        let endings = self
            .owned
            .loan_ends()
            .iter()
            .filter(|fact| fact.point() == LoanEndPoint::ControlTransfer(transfer))
            .map(|fact| (fact.call().index(), fact.argument().index()))
            .collect::<Vec<_>>();
        for key in &endings {
            self.end_pending_source_loan(*key, span)?;
            self.release_primitive_unit_operand(key.0, key.1);
        }
        let ranges = self
            .owned
            .borrow_results()
            .range_uses()
            .iter()
            .filter(|fact| {
                matches!(fact.site(), lang_frontend::ownership_checking::RangeUseSite::Call(call)
                    if endings.iter().any(|key| key.0 == call.index()))
                    && self.temporaries.contains_key(&fact.expression().index())
            })
            .map(|fact| fact.expression())
            .collect::<Vec<_>>();
        for range in ranges.into_iter().rev() {
            self.finish_short_range(range, span)?;
        }
        let is_return = matches!(
            self.parsed
                .ast()
                .expressions()
                .get(transfer)
                .map(|node| node.payload()),
            Ok(lang_frontend::parser::Expression::Return { .. })
        );
        let views = self
            .non_null_bindings
            .iter()
            .filter_map(|(&symbol, proof)| {
                if proof.is_while_origin() {
                    return None;
                }
                let retained = !is_return
                    && self
                        .loops
                        .last()
                        .is_some_and(|context| context.entry_views.contains_key(&symbol));
                (!retained).then_some(symbol)
            })
            .collect::<Vec<_>>();
        for symbol in views {
            self.end_non_null_proof(symbol, span)?;
        }
        self.emit_drops(DropPoint::ControlTransfer(transfer))
    }

    pub(super) fn emit_drops(&mut self, point: DropPoint) -> Result<(), LoweringError> {
        let facts = self
            .owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.point() == point)
            .collect::<Vec<_>>();
        self.preflight_nullable_if_cleanup_groups(&facts)?;
        self.preflight_nullable_numeric_cleanup(&facts)?;
        self.preflight_nullable_borrowed_cleanup(&facts)?;
        self.end_result_bindings(point)?;
        let mut index = 0;
        while index < facts.len() {
            if let Some(consumed) = self.emit_nullable_if_cleanup_group(&facts[index..])? {
                index += consumed;
                continue;
            }
            let fact = facts[index];
            index += 1;
            if self.emit_resource_map_drop(fact)? {
                continue;
            }
            if self.emit_local_map_drop(fact)? {
                continue;
            }
            if self.emit_outer_borrowed_drop(fact)? {
                continue;
            }
            if self.emit_nullable_borrowed_cleanup(fact)? {
                continue;
            }
            if self.emit_nullable_numeric_cleanup(fact)? {
                continue;
            }
            let template = match fact.target() {
                DropTarget::Named(symbol) => Some(self.typed.symbol_type(symbol)),
                DropTarget::Temporary(expression) => Some(self.typed.expression_type(expression)),
                _ => None,
            };
            if let Some(template) = template {
                let template = template
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, fact.value_origin()))?;
                let concrete = self.resolve_type(template, fact.value_origin())?;
                if concrete != template
                    && self.typed.copyability(concrete)
                        == Some(lang_frontend::type_checking::Copyability::Copyable)
                {
                    continue;
                }
            }
            if fact.condition().is_some()
                && !super::map_loop_owner::supported_drop(
                    self.parsed,
                    self.names,
                    self.typed,
                    self.owned,
                    fact,
                )
            {
                return Err(error(
                    LoweringErrorKind::UnsupportedNode,
                    fact.value_origin(),
                ));
            }
            let owner = match fact.target() {
                DropTarget::Named(symbol) => {
                    if matches!(point, DropPoint::LoopExit(_))
                        && !self.bindings.contains_key(&symbol)
                        && (fact.condition().is_none()
                            || super::constant_presence::constant_drop(
                                self.parsed,
                                self.names,
                                self.typed,
                                self.owned,
                                fact,
                            ))
                    {
                        continue;
                    }
                    self.end_non_null_proof(symbol, fact.value_origin())?;
                    match self.bindings.remove(&symbol) {
                        Some(LoweredValue::Value(value)) => value,
                        Some(LoweredValue::Unit | LoweredValue::Diverged) | None => {
                            return Err(error(LoweringErrorKind::MissingFact, fact.value_origin()));
                        }
                    }
                }
                DropTarget::Temporary(expression) => {
                    let owner = self
                        .temporaries
                        .remove(&expression.index())
                        .ok_or_else(|| {
                            error(LoweringErrorKind::MissingFact, fact.value_origin())
                        })?;
                    self.temporaries.retain(|_, candidate| *candidate != owner);
                    owner
                }
                DropTarget::Captured { .. } => continue,
                DropTarget::RetainedSource(_) => {
                    return Err(error(
                        LoweringErrorKind::UnsupportedNode,
                        fact.value_origin(),
                    ));
                }
                DropTarget::ReplacedElement(_) => {
                    return Err(error(
                        LoweringErrorKind::UnsupportedNode,
                        fact.value_origin(),
                    ));
                }
            };
            self.record_map_loop_drop(fact, owner)?;
            self.append(Operation::Drop { owner }, Vec::new(), fact.value_origin())?;
            self.release_owner_capture_loans(owner, fact.value_origin())?;
        }
        Ok(())
    }

    pub(super) fn shared_owner_operand(
        &mut self,
        receiver: ExpressionId,
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        if let Some(proof) = self.non_null_proof_for_expression(receiver, span)? {
            return Ok(EntityId::Loan(proof.loan));
        }
        self.require_value(receiver)
            .map(EntityId::Value)
            .map_err(|mut error| {
                error.span.get_or_insert(span);
                error
            })
    }

    pub(super) fn validate_non_null_proof_types(
        &self,
        proof: NonNullProof,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(EntityType::Value(nullable)) = self
            .function
            .entity(EntityId::Value(proof.owner))
            .map(|entity| entity.ty)
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let valid_storage = match self.ssa_types.get(nullable.index()) {
            Some(SsaTypeKind::NullableHandle { inner }) => *inner == proof.inner,
            Some(SsaTypeKind::Aggregate {
                ownership: Ownership::MoveOnly,
                ..
            }) if !proof.is_while_origin() => {
                self.nullable_maps.get(&nullable) == Some(&proof.inner)
                    || self.map_results.get(&nullable) == Some(&proof.inner)
            }
            _ => false,
        };
        if !valid_storage
            || self
                .function
                .entity(EntityId::Loan(proof.loan))
                .map(|entity| entity.ty)
                != Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: proof.inner,
                })
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    pub(super) fn validate_non_null_proof(
        &self,
        symbol: SymbolId,
        proof: NonNullProof,
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if bindings.get(&symbol) != Some(&LoweredValue::Value(proof.owner)) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_non_null_proof_types(proof, span)
    }

    pub(super) fn checked_non_null_proof(
        &self,
        symbol: SymbolId,
        inner: SsaTypeId,
        span: Span,
    ) -> Result<Option<NonNullProof>, LoweringError> {
        let Some(proof) = self.non_null_bindings.get(&symbol).copied() else {
            return Ok(None);
        };
        self.validate_non_null_proof(symbol, proof, &self.bindings, span)?;
        if proof.inner != inner {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(Some(proof))
    }

    pub(super) fn end_non_null_proof(
        &mut self,
        symbol: SymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(proof) = self.non_null_bindings.get(&symbol).copied() else {
            return Ok(());
        };
        self.validate_non_null_proof(symbol, proof, &self.bindings, span)?;
        if proof.is_while_origin() {
            self.ensure_no_unresolved_while_proof_carriers(span)?;
        }
        self.append(Operation::BorrowEnd { loan: proof.loan }, Vec::new(), span)?;
        self.non_null_bindings.remove(&symbol);
        Ok(())
    }

    pub(super) fn non_null_proof_for_expression(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<NonNullProof>, LoweringError> {
        let Some(non_null) = self.typed.non_null_use(expression) else {
            return Ok(None);
        };
        if !self.non_null_bindings.contains_key(&non_null.symbol()) {
            return Ok(None);
        }
        self.checked_non_null_proof(
            non_null.symbol(),
            self.expression_ssa_type(expression, span)?,
            span,
        )
    }

    pub(super) fn validate_non_null_exit_proofs(
        &self,
        exits: &[BranchExit],
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(first) = exits.first() else {
            return Ok(());
        };
        for exit in exits {
            if exit.views.keys().ne(first.views.keys()) {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            for (&symbol, &proof) in &exit.views {
                self.validate_non_null_proof(symbol, proof, &exit.bindings, span)?;
                if proof.inner != first.views[&symbol].inner
                    || proof.origin != first.views[&symbol].origin
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
            }
        }
        Ok(())
    }

    pub(super) fn merged_non_null_proofs(
        &self,
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        parameters: Vec<(SymbolId, LoanId, NonNullProof)>,
        span: Span,
    ) -> Result<BTreeMap<SymbolId, NonNullProof>, LoweringError> {
        let mut views = BTreeMap::new();
        for (symbol, loan, source) in parameters {
            let Some(LoweredValue::Value(owner)) = bindings.get(&symbol).copied() else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let proof = NonNullProof {
                loan,
                owner,
                ..source
            };
            self.validate_non_null_proof(symbol, proof, bindings, span)?;
            views.insert(symbol, proof);
        }
        Ok(views)
    }
}

#[cfg(test)]
mod guard_tests;
