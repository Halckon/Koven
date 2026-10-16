//! Direct owned nullable guards create one real proof on each body entry.
mod entry;

use super::super::{NonNullProof, non_null_proof::ProofScope, span_key};
use super::*;
use crate::ssa::model::SsaTypeKind;
use entry::DirectNullableWhileEntry;
pub(super) use entry::NullableWhileEntry;
use lang_frontend::{
    ownership_checking::DropPoint,
    parser::{BinaryOperator, Expression, LiteralKind},
    type_checking::{Copyability, ExpressionCategory, NullComparisonDescriptor, TypeKind},
};

impl ExpressionLowerer<'_> {
    pub(super) fn direct_nullable_while_condition(
        &self,
        condition: ExpressionId,
        span: Span,
    ) -> Result<Option<(NullComparisonDescriptor, ExpressionId)>, LoweringError> {
        let expression = self.field_replace_ungroup(condition, span)?;
        let Some(descriptor) = self.typed.null_comparison(expression) else {
            return Ok(None);
        };
        if !descriptor.non_null_when_true()
            || self.borrow_bindings.contains_key(&descriptor.symbol())
            || self.typed.copyability(descriptor.nullable_type()) == Some(Copyability::Copyable)
        {
            return Ok(None);
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Binary {
            left,
            operator: BinaryOperator::NotEqual,
            right,
            ..
        } = *node.payload()
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
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
            _ => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        let leaf = self.field_replace_ungroup(subject, span)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(leaf)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(node.payload(), Expression::Name) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        if descriptor.expression() != expression
            || self.references.get(&span_key(node.span())) != Some(&descriptor.symbol())
            || self.typed.symbol_type(descriptor.symbol()) != Some(descriptor.nullable_type())
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(Some((descriptor, subject)))
    }

    /// Real exits must retain the inherited proof. A killed or replaced proof
    /// cannot be reconstructed from the loop's entry record.
    pub(in super::super) fn validate_readonly_while_exit(
        &self,
        root: SymbolId,
        inherited: NonNullProof,
        exit: &BranchExit,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(proof) = exit.views.get(&root).copied() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        if exit.views.len() != 1
            || proof.origin != inherited.origin
            || proof.inner != inherited.inner
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_non_null_proof(root, proof, &exit.bindings, span)
    }

    fn direct_nullable_while_entry(
        &self,
        descriptor: NullComparisonDescriptor,
        provider: Option<&super::super::outer_borrowed_while::Relation>,
        span: Span,
    ) -> Result<(SsaTypeId, SsaTypeId), LoweringError> {
        if let Some(provider) = provider {
            self.validate_outer_borrowed_entry(provider, descriptor, span)?;
        } else {
            self.ensure_no_unresolved_while_proof_carriers(span)?;
        }
        let symbol = descriptor.symbol();
        let nullable = self.resolve_type(descriptor.nullable_type(), span)?;
        let nullable = *self
            .type_ids
            .get(&nullable)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(SsaTypeKind::NullableHandle { inner }) = self.ssa_types.get(nullable.index())
        else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let inner = *inner;
        if !matches!(
            self.ssa_types.get(inner.index()),
            Some(SsaTypeKind::HeapOwner { .. })
        ) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let Some(LoweredValue::Value(entry_owner)) = self.bindings.get(&symbol).copied() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if self
            .function
            .entity(EntityId::Value(entry_owner))
            .map(|entity| entity.ty)
            != Some(EntityType::Value(nullable))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok((nullable, inner))
    }

    pub(super) fn lower_direct_nullable_while(
        &mut self,
        statement: StatementId,
        condition: ExpressionId,
        body: StatementId,
        entry: DirectNullableWhileEntry,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let provider = entry.provider().cloned();
        let (descriptor, inherited) = entry.into_parts(statement, span)?;
        if let Some((root, proof)) = inherited {
            self.validate_readonly_while_exit(root, proof, &self.loop_exit(), span)?;
        } else if !self.non_null_bindings.is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let (nullable, inner) =
            self.direct_nullable_while_entry(descriptor, provider.as_ref(), span)?;
        let symbol = descriptor.symbol();
        let baseline = self.bindings.clone();
        let mut context = self.create_loop_header(&baseline, span)?;
        context.outer_borrowed_while = provider.clone();
        if let Some((root, proof)) = inherited {
            self.validate_readonly_while_exit(root, proof, &self.loop_exit(), span)?;
        }
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&symbol).copied() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if self
            .function
            .entity(EntityId::Value(owner))
            .map(|entity| entity.ty)
            != Some(EntityType::Value(nullable))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        // Both successors start from the complete header state, before the new birth.
        let condition_state = self.loop_exit();
        let carried = self.linear_binding_slots(&condition_state.bindings, span)?;
        let owner_slot = carried
            .slots
            .iter()
            .position(|slot| slot.symbol == Some(symbol) && slot.source == EntityId::Value(owner))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let mut body_types = carried.slots.iter().map(|slot| slot.ty).collect::<Vec<_>>();
        let false_block = self
            .function
            .add_block(body_types.clone(), Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        body_types.push(EntityType::Loan {
            kind: LoanKind::Shared,
            target: inner,
        });
        let body_block = self
            .function
            .add_block(body_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let parameters = &self
            .function
            .block(body_block)
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
            .parameters;
        let (Some(EntityId::Value(body_owner)), Some(EntityId::Loan(view))) =
            (parameters.get(owner_slot), parameters.last())
        else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let (body_owner, view) = (*body_owner, *view);
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::NullableBranch {
                    owner,
                    when_null: loop_edge(false_block, &carried),
                    when_non_null: loop_edge(body_block, &carried),
                    view,
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;

        self.restore_direct_while_condition(&condition_state, provider.as_ref(), span)?;
        self.block = false_block;
        self.bindings =
            self.rebind_linear_bindings(&condition_state.bindings, false_block, &carried, span)?;
        if let Some((root, proof)) = inherited {
            self.validate_readonly_while_exit(root, proof, &self.loop_exit(), span)?;
        }
        self.emit_direct_while_condition_drops(condition, span)?;
        let false_exit = self.loop_exit();

        self.restore_direct_while_condition(&condition_state, provider.as_ref(), span)?;
        self.block = body_block;
        self.bindings =
            self.rebind_linear_bindings(&condition_state.bindings, body_block, &carried, span)?;
        if let Some((root, inherited_proof)) = inherited {
            self.validate_readonly_while_exit(root, inherited_proof, &self.loop_exit(), span)?;
            let outer = self
                .non_null_bindings
                .get(&root)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if outer.owner == body_owner || outer.loan == view {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        let proof = NonNullProof::introduced(view, body_owner, inner, ProofScope::While(statement));
        self.validate_non_null_proof(symbol, proof, &self.bindings, span)?;
        self.non_null_bindings.insert(symbol, proof);
        self.loops.push(context);
        self.emit_direct_while_condition_drops(condition, span)?;
        if self.lower_statement(body)? != LoweredValue::Diverged {
            // The statement already consumed its exact cleanup facts.
            self.record_continue(span)?;
        }
        let context = self
            .loops
            .pop()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if let Some((root, proof)) = inherited {
            for jump in &context.continues {
                self.validate_readonly_while_exit(root, proof, &jump.exit, jump.span)?;
            }
        }
        self.finish_loop_continues(&context)?;
        let mut exits = context.breaks;
        exits.push(false_exit);
        let result = self.finish_loop_exits(
            statement,
            exits,
            &baseline,
            inherited,
            provider.as_ref(),
            span,
        )?;
        if let Some((root, proof)) = inherited {
            self.validate_readonly_while_exit(root, proof, &self.loop_exit(), span)?;
        }
        Ok(result)
    }

    fn restore_direct_while_condition(
        &mut self,
        state: &BranchExit,
        provider: Option<&super::super::outer_borrowed_while::Relation>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if let Some(provider) = provider {
            self.restore_outer_borrowed_state(provider, state, span)?;
        } else if !state.for_sources.is_empty() || !state.for_elements.is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.block = state.block;
        self.bindings.clone_from(&state.bindings);
        self.temporaries.clone_from(&state.temporaries);
        self.pending_call_loans.clone_from(&state.loans);
        self.capture_loans.clone_from(&state.capture_loans);
        self.borrow_bindings.clone_from(&state.borrow_bindings);
        self.result_source_loans
            .clone_from(&state.result_source_loans);
        self.non_null_bindings.clone_from(&state.views);
        Ok(())
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
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        match *node.payload() {
            Expression::Group { expression } => {
                self.emit_direct_while_condition_drops(expression, span)?
            }
            Expression::Binary { left, right, .. } => {
                self.emit_null_condition_drops(left)?;
                self.emit_null_condition_drops(right)?;
                self.emit_drops(DropPoint::AfterBinaryOperands(condition))?;
            }
            _ => return Err(error(LoweringErrorKind::MissingFact, span)),
        }
        self.emit_drops(DropPoint::AfterExpression(condition))
    }
}
