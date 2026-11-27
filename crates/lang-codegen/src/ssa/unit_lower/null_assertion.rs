//! Compatibility lowering for null-only assertions and narrowed Char reads.

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::NonNullAssertionTransferKind,
    source::Span,
    type_checking::{BuiltinType, UnitExpressionId, UnitTypeKind},
};

use super::{
    LoweredValue, UnitExpressionLowerer, cfg::carried_control_edge, lowering_error,
    resolve_concrete_type,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    lowering_support::{NULL_ASSERTION_MESSAGE, is_null_only},
    model::{AbortReason, EntityId, EntityType, Operation, Origin, SsaTypeKind, TerminatorKind},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_narrowed_char_name(
        &mut self,
        expression: ExpressionId,
        symbol: UnitSymbolId,
        value: LoweredValue,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let Some(fact) = self.typed.non_null_use(id) else {
            return Ok(value);
        };
        if self.typed.types().get(fact.narrowed_type())
            != Some(&UnitTypeKind::Builtin(BuiltinType::Char))
        {
            return Ok(value);
        }
        let inner = self.expression_ssa_type(expression, span)?;
        let declared = fact.declared_type();
        let Some(&storage) = self.type_ids.get(&declared) else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let LoweredValue::Value(owner) = value else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if fact.expression() != id
            || fact.symbol() != symbol
            || self.typed.symbol_type(symbol) != Some(declared)
            || self.typed.expression_type(id) != Some(fact.narrowed_type())
            || self.typed.types().get(declared)
                != Some(&UnitTypeKind::Nullable(fact.narrowed_type()))
            || self
                .function
                .entity(EntityId::Value(owner))
                .map(|entity| entity.ty)
                != Some(EntityType::Value(storage))
            || self.ssa_types.get(storage.index())
                != Some(&SsaTypeKind::InlineNullable {
                    inner,
                    niche: false,
                })
            || self.ssa_types.get(inner.index()) != Some(&SsaTypeKind::Char)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }

        // The frontend proves this read is non-null. Establish a local SSA
        // proof for the current storage value before reading its payload.
        let baseline = self.bindings.clone();
        let mut carried = self.carried_bindings(&baseline, span)?;
        let mut loans = self.carried_loans(&self.borrow_bindings, span)?;
        self.carry_pending_operands(&mut carried, &mut loans, span)?;
        let (present, proof) = self.add_map_borrow_block(&carried, &loans, inner, span)?;
        let absent = self.add_carried_control_block(&carried, &loans, span)?;
        self.function
            .set_abort_terminator(
                absent,
                AbortReason::Text("non-null Char proof violated".to_owned()),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::NullableBranch {
                    owner,
                    when_null: carried_control_edge(absent, &carried, &loans),
                    when_non_null: carried_control_edge(present, &carried, &loans),
                    view: proof,
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.block = present;
        self.rebind_map_borrow_control(&baseline, present, &carried, &loans, proof, inner, span)?;
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&symbol).copied() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        self.append_scalar(Operation::NullableTake { owner, proof }, inner, span)
    }

    pub(super) fn lower_null_only_assertion(
        &mut self,
        expression: ExpressionId,
        operand: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .non_null_assertion(id)
            .filter(|descriptor| {
                descriptor.operand() == UnitExpressionId::new(self.source_unit, operand)
            })
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let nullable = resolve_concrete_type(
            self.typed,
            descriptor.nullable_type(),
            self.substitutions,
            self.static_self,
            span,
        )?;
        if !is_null_only(self.typed, nullable) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let plan = self.owned.non_null_assertion(id);
        if plan.is_some_and(|plan| {
            plan.descriptor() != &descriptor
                || plan.non_null_transfer() != NonNullAssertionTransferKind::Copy
        }) || (plan.is_none()
            && self.typed.expression_falls_through(descriptor.operand()) != Some(false))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }

        // Only the successful assertion edge would complete the operand's outer cleanup.
        let LoweredValue::Value(value) = (match self.lower_expression(operand)? {
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            value => value,
        }) else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let ty = self.expression_ssa_type(operand, span)?;
        if plan.is_none()
            || self.function.entity(EntityId::Value(value)).map(|e| e.ty)
                != Some(EntityType::Value(ty))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.function
            .set_abort_terminator(
                self.block,
                AbortReason::Text(NULL_ASSERTION_MESSAGE.to_owned()),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Diverged)
    }
}
