//! Bounded nullable entries: root Borrow and Rc/heap Ifs.
#[cfg(test)]
mod field_tests;
mod heap;
mod rc;

use super::super::unwrap_modified;
use super::super::{ExpressionCategory, Statement, nullable_proof::NullableProofScope, span_key};
use super::*;
use crate::ssa::model::{LoanId, SsaTypeKind};
use lang_frontend::{
    ownership_checking::{LoanKind as FrontendLoanKind, UnitLoanTarget},
    parser::{BinaryOperator, FunctionBody, FunctionForm, Item},
    type_checking::{IntrinsicTypeConstructor, UnitCallableTarget, UnitExpressionId, UnitTypeKind},
};

#[derive(Clone, Copy)]
pub(super) struct NullableIfEntry {
    pub(super) root: UnitSymbolId,
    pub(super) owner: ValueId,
    pub(super) inner: SsaTypeId,
    pub(super) scope: UnitExpressionId,
    pub(super) non_null_when_true: bool,
}

impl UnitExpressionLowerer<'_> {
    /// Reuse the proof entry only for a required root Borrow or Rc/heap If.
    pub(super) fn prepare_nullable_if_entry(
        &self,
        control: UnitExpressionId,
        condition: ExpressionId,
        result_required: bool,
        span: Span,
    ) -> Result<Option<NullableIfEntry>, LoweringError> {
        if !self.nullable_proofs.is_empty() || result_required {
            return Ok(None);
        }
        // Nullable `String?` ifs lower only in `ssa::refine` (SPEC-0398); a mixed
        // compatibility body keeps its older inline-nullable handling.
        if !self.migrated_family_compatibility
            && self
                .typed
                .null_comparison(UnitExpressionId::new(self.source_unit, condition))
                .is_some_and(|fact| {
                    matches!(
                        self.typed.types().get(fact.nullable_type()),
                        Some(UnitTypeKind::Nullable(inner))
                            if matches!(
                                self.typed.types().get(*inner),
                                Some(UnitTypeKind::Builtin(
                                    lang_frontend::type_checking::BuiltinType::String
                                ))
                            )
                    )
                })
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if let Some(entry) = self.prepare_rc_nullable_if_entry(control, condition, span)? {
            return Ok(Some(entry));
        }
        if self.migrated_family_compatibility
            && let Some(entry) = self.prepare_heap_nullable_if_entry(control, condition, span)?
        {
            return Ok(Some(entry));
        }
        Ok(None)
    }

    /// Follow the actual current source instance to its own top-level block elements.
    pub(in crate::ssa::unit_lower) fn is_direct_function_body_if(
        &self,
        control: UnitExpressionId,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let key = self
            .source_plan
            .source(self.source_token)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self.function_ids.get(key) != Some(&self.function.id) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if key.deinit_owner().is_some() {
            return Ok(false);
        }
        let UnitCallableTarget::Declaration(target) = key.target() else {
            return Ok(false);
        };
        let index = self.names.names().index();
        let declaration = index
            .declarations()
            .get(target.index())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let source = index
            .source_units()
            .get(self.source_unit.index())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if control.source_unit() != self.source_unit
            || declaration.id() != target
            || declaration.source_unit() != self.source_unit
            || source.id() != self.source_unit
            || source.source_id() != self.parsed.source_id()
            || declaration.name_span().source_id() != self.parsed.source_id()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let (_, item, _) = unwrap_modified(self.parsed, declaration.root())?;
        let Item::Function { form, .. } = item else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let body = match form {
            FunctionForm::ImplicitUnitBlock(body)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(body),
                ..
            } => body,
            _ => return Ok(false),
        };
        let node = self
            .parsed
            .ast()
            .statements()
            .get(body)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Statement::Block { elements } = node.payload() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let mut hits = 0;
        for &statement in elements {
            let node = self
                .parsed
                .ast()
                .statements()
                .get(statement)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if let Statement::Expression { expression } = node.payload()
                && self.ungroup_map_if_expression(*expression, span)? == control.expression()
            {
                hits += 1;
            }
        }
        if hits > 1 {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(hits == 1)
    }

    pub(in crate::ssa::unit_lower) fn ungroup_map_if_expression(
        &self,
        mut expression: ExpressionId,
        span: Span,
    ) -> Result<ExpressionId, LoweringError> {
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression: inner } => expression = *inner,
                _ => return Ok(expression),
            }
        }
    }

    pub(in crate::ssa::unit_lower) fn map_if_body_is_empty(
        &self,
        body: Option<StatementId>,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let Some(body) = body else { return Ok(true) };
        let node = self
            .parsed
            .ast()
            .statements()
            .get(body)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        Ok(matches!(node.payload(), Statement::ControlBody { elements } if elements.is_empty()))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn enter_nullable_if_branch(
        &mut self,
        entry: NullableIfEntry,
        header: BlockId,
        block: BlockId,
        view: LoanId,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        carried: &[super::super::cfg::CarriedBinding],
        loans: &[super::super::cfg::CarriedAccess],
        condition: ExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.block = block;
        self.rebind_map_borrow_control(baseline, block, carried, loans, view, entry.inner, span)?;
        let column = self.nullable_owner_column(carried, entry.root, entry.owner, span)?;
        self.introduce_nullable_proof(
            header,
            column,
            entry.root,
            entry.inner,
            NullableProofScope::If(entry.scope),
            span,
        )?;
        self.emit_direct_nullable_condition_drops(condition, span)
    }

    pub(in super::super) fn emit_direct_nullable_condition_drops(
        &mut self,
        condition: ExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(condition)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        match *node.payload() {
            Expression::Group { expression } => {
                self.emit_direct_nullable_condition_drops(expression, span)?
            }
            Expression::Binary { left, right, .. } => {
                self.emit_null_condition_drops(left)?;
                self.emit_null_condition_drops(right)?;
                self.emit_drops(UnitDropPoint::AfterBinaryOperands(UnitExpressionId::new(
                    self.source_unit,
                    condition,
                )))?;
            }
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        }
        self.emit_drops(UnitDropPoint::AfterExpression(UnitExpressionId::new(
            self.source_unit,
            condition,
        )))
    }

    /// Preflight every retirement against its own complete live exit before emission.
    /// Same-root origin disagreements remain errors, even if another root is absent.
    pub(super) fn retire_if_exit_proofs(
        &mut self,
        control: UnitExpressionId,
        exits: &mut [BranchExit],
        span: Span,
    ) -> Result<(), LoweringError> {
        let mut identities = BTreeMap::new();
        for exit in exits.iter() {
            self.validate_nullable_proofs(&exit.bindings, &exit.nullable_proofs, span)?;
            for (&root, proof) in &exit.nullable_proofs {
                if identities
                    .insert(root, (proof.origin, proof.inner))
                    .is_some_and(|previous| previous != (proof.origin, proof.inner))
                {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
        }
        let retirements = exits
            .iter()
            .map(|exit| {
                exit.nullable_proofs
                    .iter()
                    .filter_map(|(&root, proof)| {
                        (proof.if_scope() == Some(control)
                            || exits
                                .iter()
                                .any(|other| !other.nullable_proofs.contains_key(&root)))
                        .then_some(root)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for (exit, roots) in exits.iter().zip(&retirements) {
            if !roots.is_empty()
                && self
                    .function
                    .block(exit.block)
                    .is_none_or(|block| block.terminator.is_some())
            {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            }
            for root in roots {
                let proof = exit
                    .nullable_proofs
                    .get(root)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                if proof.if_scope() != Some(control) {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
            self.restore_loop_state(exit);
            for &root in roots {
                self.validate_nullable_proof_retirement(root, span)?;
            }
        }
        for (exit, roots) in exits.iter_mut().zip(retirements) {
            self.restore_loop_state(exit);
            for root in roots {
                self.end_nullable_proof(root, span)?;
            }
            // BorrowEnd changes only the proof map. Owners and all pending state stay exact.
            exit.nullable_proofs.clone_from(&self.nullable_proofs);
        }
        Ok(())
    }
}
