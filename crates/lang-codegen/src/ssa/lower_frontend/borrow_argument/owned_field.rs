//! A single ordinary owned-local class field as a synchronous Borrow argument.
use lang_frontend::{
    ownership_checking::{LoanKind as FrontendLoanKind, OwnershipPlace},
    type_checking::{AggregateProjectionKind, AggregateProjectionReceiver},
};

use super::super::*;
use crate::ssa::model::SsaTypeKind;

impl ExpressionLowerer<'_> {
    pub(super) fn lower_owned_field_borrow_place(
        &mut self,
        expression: ExpressionId,
        path: &OwnershipPlace,
        target: SsaTypeId,
        span: Span,
    ) -> Result<PlaceId, LoweringError> {
        let [field_symbol] = path.fields() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        if !path.elements().is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let projection = self
            .typed
            .aggregate_projection(expression)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let AggregateProjectionReceiver::Expression(mut receiver) = projection.receiver() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        if projection.expression() != expression
            || projection.kind() != AggregateProjectionKind::Field
            || projection.field() != *field_symbol
            || self.typed.expression_type(expression) != Some(projection.ty())
            || self.type_ids.get(&projection.ty()) != Some(&target)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let root = path.root();
        let symbol = self
            .names
            .symbols()
            .get(root.index())
            .filter(|symbol| symbol.id() == root)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if symbol.kind() != SymbolKind::Variable
            || self.typed.parameter_mode(root).is_some()
            || self.borrow_bindings.contains_key(&root)
            || self.non_null_bindings.contains_key(&root)
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner_type = self
            .typed
            .symbol_type(root)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        loop {
            if self.typed.non_null_use(receiver).is_some() {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            if self.typed.expression_type(receiver) != Some(owner_type) {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(receiver)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression } => receiver = *expression,
                Expression::Name => {
                    if self.references.get(&span_key(node.span())) != Some(&root) {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    break;
                }
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
        let Some(TypeKind::Nominal { nominal, arguments }) = self.typed.types().get(owner_type)
        else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let nominal = self
            .typed
            .nominals()
            .iter()
            .find(|declaration| declaration.id() == *nominal)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if nominal.kind() != NominalKind::Class
            || !arguments.is_empty()
            || !nominal.type_parameters().is_empty()
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        if self.typed.symbol_type(*field_symbol) != Some(projection.ty()) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let field = nominal
            .fields()
            .iter()
            .position(|candidate| candidate == field_symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&root).copied() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        // Source sibling fields can be disjoint; the SSA alias model is still parent-wide.
        if self.owned.loans().iter().any(|active| {
            self.pending_call_loans
                .contains_key(&(active.call().index(), active.argument().index()))
                && active.kind() == FrontendLoanKind::Exclusive
                && matches!(active.target(), LoanTarget::Place(place) if place.root() == root)
        }) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner_ssa = self
            .type_ids
            .get(&owner_type)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if self
            .function
            .entity(EntityId::Value(owner))
            .map(|entity| entity.ty)
            != Some(EntityType::Value(owner_ssa))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let Some(SsaTypeKind::HeapOwner {
            payload: Some(payload),
            ..
        }) = self.ssa_types.get(owner_ssa.index())
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if self.heap_payloads.get(&owner_ssa) != Some(payload)
            || !matches!(self.ssa_types.get(payload.index()), Some(SsaTypeKind::Aggregate { fields, .. }) if fields.get(field) == Some(&target))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        // Read the existing binding directly: evaluating the receiver could run its
        // AfterExpression drop before the call. The caller owns the sole loan lifecycle.
        let (_, roots) = self.append(
            Operation::HeapPayloadPlace { owner },
            vec![EntityType::Place(*payload)],
            span,
        )?;
        let (_, fields) = self.append(
            Operation::FieldPlace {
                base: place(roots[0]),
                field,
            },
            vec![EntityType::Place(target)],
            span,
        )?;
        Ok(place(fields[0]))
    }
}
