//! Field replacement in the refined CFG (SPEC-0357): the RHS is evaluated first, then the
//! field is replaced under an exclusive loan of the owner; the replacement drops
//! the old MoveOnly value, so the published `BeforeReplacement` drop is consumed by it.
use lang_frontend::{ast::ExpressionId, ownership_checking::UnitDropPoint, source::Span};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::LoanKind,
    refine::body::ROp,
};

impl Builder<'_, '_> {
    pub(super) fn is_field_target(&self, target: ExpressionId) -> bool {
        self.inputs
            .typed
            .aggregate_projection(self.unit(target))
            .is_some()
    }

    /// `owner.f = v` on a plain class owner: a local, an Inout parameter or the Inout
    /// receiver (explicit or implicit `this`).
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
        let (owner, owner_type, _) = self.projection_owner(projection.receiver(), span)?;
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
        let loan = self.owner_loan(owner, LoanKind::Exclusive, span)?;
        self.push(
            ROp::HeapFieldReplace {
                receiver: loan.loan,
                field,
                value: replacement,
            },
            Vec::new(),
            span,
        )?;
        self.end_owner_loan(loan, span)?;
        let replaced = self.collect_drops(UnitDropPoint::BeforeReplacement(self.unit(assignment)));
        self.finish_drops(&replaced);
        Ok(Lowered::Unit)
    }
}
