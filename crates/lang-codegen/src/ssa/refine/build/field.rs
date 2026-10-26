//! Field replacement in the refined CFG (SPEC-0357 step 2): the RHS is evaluated first,
//! then the field is replaced under an exclusive loan of the owner; the replacement drops
//! the old MoveOnly value, so the published `BeforeReplacement` drop is consumed by it.
use lang_frontend::{
    ast::ExpressionId, ownership_checking::UnitDropPoint, parser::Expression, source::Span,
    type_checking::UnitAggregateProjectionReceiver,
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind},
    refine::body::ROp,
};

impl Builder<'_, '_> {
    pub(super) fn is_field_target(&self, target: ExpressionId) -> bool {
        self.inputs
            .typed
            .aggregate_projection(self.unit(target))
            .is_some()
    }

    /// `owner.f = v` on a local plain class owner.
    pub(super) fn field_assignment(
        &mut self,
        assignment: ExpressionId,
        target: ExpressionId,
        value: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let projection = self
            .inputs
            .typed
            .aggregate_projection(self.unit(target))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let (payload, receiver_span) = self.node(receiver.expression())?;
        let Expression::Name = payload else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let symbol = self.reference(receiver_span)?;
        let owner_type = self
            .inputs
            .typed
            .symbol_type(symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, receiver_span))?;
        let field = self
            .types
            .field_indices
            .get(&(owner_type, projection.field()))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        // Guide: the RHS is evaluated before the old value is dropped and replaced.
        let Some(replacement) = self.required(value)? else {
            return Ok(Lowered::Diverged);
        };
        let replacement = self.adapt(replacement, value, projection.ty())?;
        self.deliver(value);
        let owner = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, receiver_span))?;
        let target_type = self.ssa_type(owner_type, span)?;
        let place = self.value(
            ROp::RootPlace { owner },
            EntityType::Place(target_type),
            span,
        )?;
        let loan = self.value(
            ROp::BorrowBegin {
                place,
                kind: LoanKind::Exclusive,
            },
            EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: target_type,
            },
            span,
        )?;
        self.push(
            ROp::HeapFieldReplace {
                receiver: loan,
                field,
                value: replacement,
            },
            Vec::new(),
            span,
        )?;
        self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
        let replaced = self.collect_drops(UnitDropPoint::BeforeReplacement(self.unit(assignment)));
        self.finish_drops(&replaced);
        Ok(Lowered::Unit)
    }
}
