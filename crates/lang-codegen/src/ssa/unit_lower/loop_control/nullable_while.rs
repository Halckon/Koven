//! Source-qualified direct nullable guards: one real view per body entry.
use super::super::{
    EntityId, EntityType, Expression, LiteralKind, SsaTypeId, nullable_proof::NullableProof,
    resolve_concrete_type, span_key,
};
use super::*;
use crate::ssa::model::SsaTypeKind;
use lang_frontend::{
    parser::BinaryOperator,
    type_checking::{Copyability, ExpressionCategory, UnitNullComparisonDescriptor, UnitTypeKind},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn direct_nullable_while_condition(
        &self,
        condition: ExpressionId,
        span: Span,
    ) -> Result<Option<(UnitNullComparisonDescriptor, UnitExpressionId)>, LoweringError> {
        let expression = self.ungroup_while_operand(condition, span)?;
        let id = UnitExpressionId::new(self.source_unit, expression);
        let Some(descriptor) = self.typed.null_comparison(id) else {
            return Ok(None);
        };
        if !descriptor.non_null_when_true()
            || self.borrow_bindings.contains_key(&descriptor.symbol())
            || self.typed.copyability(descriptor.nullable_type()) == Copyability::Copyable
        {
            return Ok(None);
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Binary {
            left,
            operator: BinaryOperator::NotEqual,
            right,
            ..
        } = *node.payload()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let is_null = |operand| {
            self.parsed
                .ast()
                .expressions()
                .get(operand)
                .is_ok_and(|node| matches!(node.payload(), Expression::Literal(LiteralKind::Null)))
        };
        let subject = match (is_null(left), is_null(right)) {
            (true, false) => right,
            (false, true) => left,
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        let leaf = self.ungroup_while_operand(subject, span)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(leaf)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(node.payload(), Expression::Name) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if descriptor.expression() != id
            || self.references.get(&span_key(node.span())) != Some(&descriptor.symbol())
            || self.typed.symbol_type(descriptor.symbol()) != Some(descriptor.nullable_type())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(Some((
            descriptor,
            UnitExpressionId::new(self.source_unit, subject),
        )))
    }

    /// Source-qualified operand facts distinguish inheritance from a head kill.
    /// No existing view is ended until every transparent layer has qualified.
    pub(super) fn prepare_nested_nullable_while(
        &mut self,
        descriptor: UnitNullComparisonDescriptor,
        mut operand: UnitExpressionId,
        span: Span,
    ) -> Result<Option<(UnitSymbolId, NullableProof)>, LoweringError> {
        let root = descriptor.symbol();
        let Some(proof) = self.nullable_proofs.get(&root).copied() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if self.nullable_proofs.len() != 1
            || !self
                .loops
                .iter()
                .any(|context| context.proof_scope == Some(proof.scope()))
            || !self.temporaries.is_empty()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        self.ensure_no_while_proof_carriers(span)?;
        let declared = resolve_concrete_type(
            self.typed,
            descriptor.nullable_type(),
            self.substitutions,
            self.static_self,
            span,
        )?;
        let nullable = self
            .type_ids
            .get(&declared)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self.typed.copyability(declared) != Copyability::MoveOnly
            || self.borrow_bindings.contains_key(&root)
            || !matches!(
                self.ssa_types.get(proof.inner.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            )
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if self.ssa_types.get(nullable.index())
            != Some(&SsaTypeKind::NullableHandle { inner: proof.inner })
            || self
                .function
                .entity(EntityId::Value(proof.owner))
                .map(|data| data.ty)
                != Some(EntityType::Value(nullable))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let storage = descriptor.nullable_type();
        let Some(UnitTypeKind::Nullable(narrowed)) = self.typed.types().get(storage) else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let narrowed = *narrowed;
        let concrete_inner = resolve_concrete_type(
            self.typed,
            narrowed,
            self.substitutions,
            self.static_self,
            span,
        )?;
        if self.type_ids.get(&concrete_inner) != Some(&proof.inner) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if self.typed.copyability(concrete_inner) != Copyability::MoveOnly {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if operand.source_unit() != self.source_unit {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let operand_type = self
            .typed
            .expression_type(operand)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if operand_type != narrowed && operand_type != storage {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let inherited = operand_type == narrowed;
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(operand.expression())
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if self.typed.expression_type(operand) != Some(operand_type)
                || self.typed.expression_category(operand) != Some(ExpressionCategory::Place)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            match (inherited, self.typed.non_null_use(operand)) {
                (true, Some(fact))
                    if fact.expression() == operand
                        && fact.symbol() == root
                        && fact.declared_type() == storage
                        && fact.narrowed_type() == narrowed => {}
                (false, None) => {}
                _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            }
            match node.payload() {
                Expression::Group { expression } => {
                    operand = UnitExpressionId::new(self.source_unit, *expression)
                }
                Expression::Name => {
                    if self.references.get(&span_key(node.span())) != Some(&root) {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                    break;
                }
                _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
        if inherited {
            return Ok(Some((root, proof)));
        }
        // The next context must contain neither the old loan nor its scope.
        // Keep the original nullable storage for the direct guard's new proof.
        self.direct_nullable_while_entry(descriptor, span)?;
        self.end_while_proof(root, span)?;
        Ok(None)
    }

    /// Require the actual path's surviving current pair, not the entry pair IDs.
    pub(super) fn validate_readonly_while_exit(
        &self,
        root: UnitSymbolId,
        inherited: NullableProof,
        exit: &BranchExit,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(proof) = exit.nullable_proofs.get(&root) else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if exit.nullable_proofs.len() != 1
            || proof.origin != inherited.origin
            || proof.inner != inherited.inner
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_nullable_proofs(&exit.bindings, &exit.nullable_proofs, span)
    }

    fn direct_nullable_while_entry(
        &self,
        descriptor: UnitNullComparisonDescriptor,
        span: Span,
    ) -> Result<(SsaTypeId, SsaTypeId), LoweringError> {
        self.ensure_no_while_proof_carriers(span)?;
        if !self.temporaries.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let symbol = descriptor.symbol();
        let nullable = resolve_concrete_type(
            self.typed,
            descriptor.nullable_type(),
            self.substitutions,
            self.static_self,
            span,
        )?;
        let nullable = *self
            .type_ids
            .get(&nullable)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Some(SsaTypeKind::NullableHandle { inner }) = self.ssa_types.get(nullable.index())
        else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let inner = *inner;
        if !matches!(
            self.ssa_types.get(inner.index()),
            Some(SsaTypeKind::HeapOwner { .. })
        ) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let Some(LoweredValue::Value(entry_owner)) = self.bindings.get(&symbol).copied() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if self
            .function
            .entity(EntityId::Value(entry_owner))
            .map(|data| data.ty)
            != Some(EntityType::Value(nullable))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok((nullable, inner))
    }

    pub(super) fn lower_direct_nullable_while(
        &mut self,
        statement: StatementId,
        condition: ExpressionId,
        body: StatementId,
        descriptor: UnitNullComparisonDescriptor,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        self.reject_nested_while_proof(span)?;
        let (nullable, inner) = self.direct_nullable_while_entry(descriptor, span)?;
        let symbol = descriptor.symbol();
        let baseline = self.bindings.clone();
        let mut context = self.create_loop_context(&baseline, span)?;
        context.proof_scope = Some(UnitStatementId::new(self.source_unit, statement));
        let entry = self.carried_edge_from(
            context.header,
            &context.carried,
            &context.loans,
            &self.loop_state(),
            span,
        )?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Branch(entry),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.block = context.header;
        self.bindings = self.rebind_loop(&baseline, context.header, &context, span)?;
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&symbol).copied() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if self
            .function
            .entity(EntityId::Value(owner))
            .map(|data| data.ty)
            != Some(EntityType::Value(nullable))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let condition_bindings = self.bindings.clone();
        let condition_closures = self.closure_bindings.clone();
        let mut carried = self.carried_bindings(&condition_bindings, span)?;
        let mut loans = self.carried_loans(&self.borrow_bindings, span)?;
        self.carry_pending_operands(&mut carried, &mut loans, span)?;
        let owner_column = self.nullable_owner_column(&carried, symbol, owner, span)?;
        let false_block = self.add_carried_control_block(&carried, &loans, span)?;
        let (body_block, view) =
            self.add_map_borrow_block(&carried, &loans, inner, self.statement_span(body)?)?;
        let state = self.loop_state();
        let when_null = self.carried_edge_from(false_block, &carried, &loans, &state, span)?;
        let when_non_null = self.carried_edge_from(body_block, &carried, &loans, &state, span)?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::NullableBranch {
                    owner,
                    when_null,
                    when_non_null,
                    view,
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;

        self.block = false_block;
        self.bindings =
            self.rebind_carried_control(&condition_bindings, false_block, &carried, &loans, span)?;
        self.borrow_bindings =
            self.rebind_carried_loans(false_block, carried.len(), &loans, span)?;
        self.emit_direct_while_condition_drops(condition, span)?;
        let false_exit = self.loop_state();

        self.block = body_block;
        self.rebind_map_borrow_control(
            &condition_bindings,
            body_block,
            &carried,
            &loans,
            view,
            inner,
            span,
        )?;
        self.closure_bindings = condition_closures;
        self.introduce_while_proof(
            context.header,
            symbol,
            owner_column,
            inner,
            UnitStatementId::new(self.source_unit, statement),
            span,
        )?;
        self.loops.push(context);
        self.emit_direct_while_condition_drops(condition, span)?;
        if self.lower_statement(body)? != LoweredValue::Diverged {
            self.record_natural_continue(span)?;
        }
        let context = self
            .loops
            .pop()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        self.finish_continues(&context, context.header)?;
        let mut exits = context
            .breaks
            .into_iter()
            .map(branch_exit)
            .collect::<Vec<_>>();
        exits.push(false_exit);
        // Break and false may have consumed different owners. Discharge each actual
        // exit's original LoopExit facts before the existing exact-state merge.
        for exit in &mut exits {
            self.restore_loop_state(exit);
            self.emit_loop_exit(statement)?;
            *exit = self.loop_state();
        }
        self.merge_loop_exits(exits, span)?;
        Ok(LoweredValue::Unit)
    }

    fn ungroup_while_operand(
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

    fn emit_direct_while_condition_drops(
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
                self.emit_direct_while_condition_drops(expression, span)?
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
}
