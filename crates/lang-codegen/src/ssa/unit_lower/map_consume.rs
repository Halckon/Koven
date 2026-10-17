//! Source-qualified owned-root delivery into a readonly Map owner.

use super::*;
use crate::ssa::model::{MapContainerKind, SsaTypeKind};
use lang_frontend::{
    ownership_checking::{UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget},
    type_checking::{IntrinsicTypeConstructor, UnitCallReceiverOrigin},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_map_consume(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .map_consume(id)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (root, storage) = descriptor
            .source()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.expression() != id
            || descriptor.receiver().source_unit() != self.source_unit
            || root.source_unit() != self.source_unit
            || self.typed.expression_type(descriptor.receiver()) != Some(descriptor.receiver_type())
            || self.typed.expression_type(id) != Some(descriptor.result_type())
            || self.typed.symbol_type(root) != Some(storage)
            || self.direct_place_symbol(descriptor.receiver().expression(), span)? != root
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let fact = self
            .owned
            .receiver_fact(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if fact.call() != id
            || fact.source() != UnitCallReceiverOrigin::Expression(descriptor.receiver())
            || fact.receiver_type() != descriptor.receiver_type()
            || fact.kind() != UnitReceiverOwnershipKind::Move
            || !matches!(fact.target(), UnitReceiverOwnershipTarget::Place(place)
                if place.is_root() && place.root() == root)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        // Nullable storage remains a separate extraction path, retaining its original facts.
        if storage != descriptor.receiver_type() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let concrete =
            |ty| resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span);
        let receiver_type = concrete(descriptor.receiver_type())?;
        let result_type = concrete(descriptor.result_type())?;
        let key = concrete(descriptor.key_type())?;
        let val = concrete(descriptor.value_type())?;
        for (ty, constructor) in [
            (receiver_type, IntrinsicTypeConstructor::MutableMap),
            (result_type, IntrinsicTypeConstructor::Map),
        ] {
            if !matches!(self.typed.types().get(ty),
                Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
                    if *actual == constructor && arguments.as_slice() == [key, val])
                || self.typed.copyability(ty) != Copyability::MoveOnly
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        let source_type = self.expression_ssa_type(descriptor.receiver().expression(), span)?;
        let map_type = self.expression_ssa_type(expression, span)?;
        let key = self
            .type_ids
            .get(&key)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let val = self
            .type_ids
            .get(&val)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        for (ty, kind) in [
            (source_type, MapContainerKind::MutableMap),
            (map_type, MapContainerKind::Map),
        ] {
            if self.ssa_types.get(ty.index())
                != Some(&SsaTypeKind::MapContainer {
                    kind,
                    key,
                    value: val,
                })
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&root).copied() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if self
            .function
            .entity(EntityId::Value(owner))
            .map(|entity| entity.ty)
            != Some(EntityType::Value(source_type))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.reject_map_consume_alias(root, owner, span)?;
        // Do not bypass the Name and transparent Group AfterExpression boundaries.
        let delivered = self.lower(descriptor.receiver().expression())?;
        if delivered != LoweredValue::Value(owner)
            || self.bindings.get(&root) != Some(&LoweredValue::Value(owner))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.reject_map_consume_alias(root, owner, span)?;
        self.take_owned_binding(root, owner, span)?;
        self.bindings
            .retain(|_, binding| *binding != LoweredValue::Value(owner));
        self.temporaries.retain(|_, temporary| *temporary != owner);
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::MapConsume { owner, map_type },
                vec![EntityType::Value(map_type)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let [EntityId::Value(result)] = results.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        self.emit_drops(UnitDropPoint::CallReturn(id))?;
        Ok(LoweredValue::Value(*result))
    }

    fn reject_map_consume_alias(
        &self,
        root: UnitSymbolId,
        owner: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.reject_live_while_proof(root, span)?;
        if self.borrow_bindings.contains_key(&root)
            || self.pending_operands.contains(&EntityId::Value(owner))
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        // Unrelated pending operands belong to their enclosing call and remain untouched.
        Ok(())
    }
}
