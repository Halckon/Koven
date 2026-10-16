//! A bounded same-root retest inside an active direct nullable while.
use super::super::{ExpressionCategory, Statement, nullable_proof::NullableProofScope, span_key};
use super::*;
use crate::ssa::model::{LoanId, SsaTypeKind};

#[derive(Clone, Copy)]
pub(super) struct NullableIfEntry {
    pub(super) root: UnitSymbolId,
    pub(super) owner: ValueId,
    pub(super) inner: SsaTypeId,
    pub(super) scope: UnitExpressionId,
}

impl UnitExpressionLowerer<'_> {
    /// Existing standalone ifs and existing live while views keep their old path.
    pub(super) fn prepare_nullable_if_entry(
        &self,
        control: UnitExpressionId,
        condition: ExpressionId,
        result_required: bool,
        span: Span,
    ) -> Result<Option<NullableIfEntry>, LoweringError> {
        if result_required || !self.nullable_proofs.is_empty() {
            return Ok(None);
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(control.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(node.payload(), Expression::If { condition: source, else_branch: None, .. }
            if *source == condition)
        {
            return Ok(None);
        }
        let Some(while_scope) = self.current_while_proof_scope() else {
            return Ok(None);
        };
        if control.source_unit() != self.source_unit
            || while_scope.source_unit() != self.source_unit
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let while_node = self
            .parsed
            .ast()
            .statements()
            .get(while_scope.statement())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Statement::While {
            condition: while_condition,
            ..
        } = while_node.payload()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let Some((outer, _)) = self.direct_nullable_while_condition(*while_condition, span)? else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let Some((descriptor, mut operand)) =
            self.direct_nullable_while_condition(condition, span)?
        else {
            return Ok(None);
        };
        if descriptor.symbol() != outer.symbol() {
            return Ok(None);
        }
        // Even the empty source-call-frame exception belongs only to While.
        if !self.pending_call_frames.is_empty() || !self.pending_operands.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let (_, inner) = self.direct_nullable_while_entry(descriptor, span)?;
        let declared = descriptor.nullable_type();
        if self.typed.copyability(resolve_concrete_type(
            self.typed,
            declared,
            self.substitutions,
            self.static_self,
            span,
        )?) != Copyability::MoveOnly
            || !matches!(
                self.ssa_types.get(inner.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            )
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(operand.expression())
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if operand.source_unit() != self.source_unit
                || self.typed.expression_type(operand) != Some(declared)
                || self.typed.expression_category(operand) != Some(ExpressionCategory::Place)
                || self.typed.non_null_use(operand).is_some()
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            match node.payload() {
                Expression::Group { expression } => {
                    operand = UnitExpressionId::new(self.source_unit, *expression)
                }
                Expression::Name
                    if self.references.get(&span_key(node.span()))
                        == Some(&descriptor.symbol()) =>
                {
                    break;
                }
                _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            }
        }
        self.ensure_no_while_proof_carriers(span)?;
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&descriptor.symbol()).copied()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        Ok(Some(NullableIfEntry {
            root: descriptor.symbol(),
            owner,
            inner,
            scope: control,
        }))
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
