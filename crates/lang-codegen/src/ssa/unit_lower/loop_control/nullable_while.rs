//! Source-qualified direct nullable guards: one real view per body entry.
mod borrowed_for;
mod entry;
pub(super) use borrowed_for::BorrowedForWhile;
pub(in super::super) use entry::DirectNullableWhileEntry;
pub(super) use entry::NullableWhileEntry;

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
    pub(in super::super) fn direct_nullable_while_condition(
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
        // This proof-producing path supports exclusive heap owners only.
        // Shared owners retain the existing ordinary while lowering.
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
        let kind = self
            .ssa_types
            .get(nullable.index())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let SsaTypeKind::NullableHandle { inner } = kind else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        match self
            .ssa_types
            .get(inner.index())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
        {
            SsaTypeKind::HeapOwner { .. } => {}
            SsaTypeKind::SharedOwner { .. } => return Ok(None),
            _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
        }
        Ok(Some((
            descriptor,
            UnitExpressionId::new(self.source_unit, subject),
        )))
    }

    pub(in super::super) fn direct_nullable_while_entry(
        &self,
        descriptor: UnitNullComparisonDescriptor,
        span: Span,
    ) -> Result<(SsaTypeId, SsaTypeId), LoweringError> {
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
        mut entry: DirectNullableWhileEntry,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if entry.scope() != UnitStatementId::new(self.source_unit, statement) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_direct_while_entry(&entry, span)?;
        let (nullable, inner) = (entry.nullable(), entry.inner());
        let symbol = entry.root();
        let inherited = entry.inherited();
        let baseline = self.bindings.clone();
        let mut context = if inherited.is_some() {
            self.create_carried_loop_context(&baseline, span)?
        } else {
            self.create_loop_context(&baseline, span)?
        };
        context.proof_scope = Some(entry.scope());
        if let Some(provider) = &mut entry.borrowed_for {
            provider.bind_header(context.header);
        }
        let entry_edge = self.carried_edge_from(
            context.header,
            &context.carried,
            &context.loans,
            &self.loop_state(),
            span,
        )?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Branch(entry_edge),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.block = context.header;
        self.bindings = self.rebind_loop(&baseline, context.header, &context, span)?;
        self.validate_direct_while_entry(&entry, span)?;
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
        let condition_state = self.loop_state();
        let mut carried = self.carried_bindings(&condition_state.bindings, span)?;
        let mut loans = self.carried_loans(&self.borrow_bindings, span)?;
        self.carry_pending_operands(&mut carried, &mut loans, span)?;
        let owner_column = self.nullable_owner_column(&carried, symbol, owner, span)?;
        let false_block = self.add_carried_control_block(&carried, &loans, span)?;
        let (body_block, view) =
            self.add_map_borrow_block(&carried, &loans, inner, self.statement_span(body)?)?;
        let when_null =
            self.carried_edge_from(false_block, &carried, &loans, &condition_state, span)?;
        let when_non_null =
            self.carried_edge_from(body_block, &carried, &loans, &condition_state, span)?;
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

        self.restore_loop_state(&condition_state);
        self.block = false_block;
        self.bindings = self.rebind_carried_control(
            &condition_state.bindings,
            false_block,
            &carried,
            &loans,
            span,
        )?;
        self.borrow_bindings =
            self.rebind_carried_loans(false_block, carried.len(), &loans, span)?;
        self.validate_direct_while_entry(&entry, span)?;
        self.emit_direct_nullable_condition_drops(condition, span)?;
        let false_exit = self.loop_state();
        if let Some(provider) = &entry.borrowed_for {
            provider.validate_exit(self, &false_exit, span)?;
        }

        self.restore_loop_state(&condition_state);
        self.block = body_block;
        self.rebind_map_borrow_control(
            &condition_state.bindings,
            body_block,
            &carried,
            &loans,
            view,
            inner,
            span,
        )?;
        self.introduce_while_proof(context.header, owner_column, &entry, span)?;
        if let Some(provider) = &entry.borrowed_for {
            let mut provider = provider.clone();
            provider.bind_proof(self, span)?;
            context.borrowed_for_while = Some(provider);
        }
        self.loops.push(context);
        self.emit_direct_nullable_condition_drops(condition, span)?;
        if self.lower_statement(body)? != LoweredValue::Diverged {
            self.record_natural_continue(span)?;
        }
        let context = self
            .loops
            .pop()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if entry.borrowed_for.is_some()
            && (!context.continues.is_empty() || context.breaks.len() != 1)
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if let Some((root, proof)) = inherited {
            for jump in &context.continues {
                self.validate_readonly_while_exit(root, proof, &jump.state, jump.span)?;
            }
        }
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
            if let Some(provider) = &entry.borrowed_for {
                provider.validate_exit(self, exit, span)?;
            }
            if let Some((root, proof)) = inherited {
                self.validate_readonly_while_exit(root, proof, exit, span)?;
            }
            self.restore_loop_state(exit);
            self.emit_loop_exit(statement)?;
            *exit = self.loop_state();
            if let Some(provider) = &entry.borrowed_for {
                provider.validate_exit(self, exit, span)?;
            }
            if let Some((root, proof)) = inherited {
                self.validate_readonly_while_exit(root, proof, exit, span)?;
            }
        }
        self.merge_loop_exits(exits, span)?;
        if let Some(provider) = &entry.borrowed_for {
            provider.validate_exit(self, &self.loop_state(), span)?;
            provider.install_outer(self, span)?;
        }
        if let Some((root, proof)) = inherited {
            self.validate_readonly_while_exit(root, proof, &self.loop_state(), span)?;
        }
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
}
