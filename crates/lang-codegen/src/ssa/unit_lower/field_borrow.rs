//! Owned local class field loans use validated projection identity without loading the field.
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::SymbolKind,
    ownership_checking::UnitOwnershipPlace,
    source::Span,
    type_checking::{
        NominalKind, UnitAggregateProjectionKind, UnitAggregateProjectionReceiver,
        UnitExpressionId, UnitTypeKind,
    },
};

use super::{LoweredValue, UnitExpressionLowerer, lowering_error};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityId, EntityType, Operation, Origin, PlaceId, SsaTypeId},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_owned_field_borrow_place(
        &mut self,
        expression: ExpressionId,
        place: &UnitOwnershipPlace,
        target: SsaTypeId,
        span: Span,
    ) -> Result<PlaceId, LoweringError> {
        let [symbol] = place.fields() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if place.element().is_some() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let id = UnitExpressionId::new(self.source_unit, expression);
        let projection = self
            .typed
            .aggregate_projection(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if projection.expression() != id
            || projection.kind() != UnitAggregateProjectionKind::Field
            || projection.field() != *symbol
            || self.typed.expression_type(id) != Some(projection.ty())
            || self.type_ids.get(&projection.ty()).copied() != Some(target)
            || receiver.source_unit() != self.source_unit
            || place.root().source_unit() != self.source_unit
            || self.direct_place_symbol(receiver.expression(), span)? != place.root()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let root = place.root();
        let root_symbol = self.names.names().source_units()[root.source_unit().index()]
            .resolution()
            .symbols()
            .get(root.symbol().index())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if root_symbol.kind() != SymbolKind::Variable {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner_type = self
            .typed
            .symbol_type(root)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self.typed.expression_type(receiver) != Some(owner_type) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.typed.types().get(owner_type)
        else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let nominal = self
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if nominal.kind() != NominalKind::Class
            || !arguments.is_empty()
            || !nominal.type_parameters().is_empty()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&root).copied() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        // SSA currently aliases sibling fields through their parent. Preserve the existing gate.
        if self.pending_call_frames.iter().any(|frame| {
            frame.field_replace_owner.is_some_and(|slot| {
                self.pending_operands.get(slot) == Some(&EntityId::Value(owner))
            })
        }) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner_ssa = self
            .type_ids
            .get(&owner_type)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self
            .function
            .entity(EntityId::Value(owner))
            .map(|entity| entity.ty)
            != Some(EntityType::Value(owner_ssa))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let payload = self
            .heap_payloads
            .get(&owner_ssa)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let field = self
            .field_indices
            .get(&(owner_type, *symbol))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if nominal
            .fields()
            .get(field)
            .is_none_or(|field| field.symbol() != *symbol || field.ty() != projection.ty())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let (_, roots) = self
            .function
            .append_instruction(
                self.block,
                Operation::HeapPayloadPlace { owner },
                vec![EntityType::Place(payload)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Place(base) = roots[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        let (_, fields) = self
            .function
            .append_instruction(
                self.block,
                Operation::FieldPlace { base, field },
                vec![EntityType::Place(target)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Place(field) = fields[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        self.pending_call_frames
            .last_mut()
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .shared_field_roots
            .push(root);
        Ok(field)
    }
}
