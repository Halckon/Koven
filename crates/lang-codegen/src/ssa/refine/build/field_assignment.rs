//! Field assignment evaluates the replacement before acquiring exclusive access.
use super::{Builder, Lowered, value::field::OwnerRef};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind},
    refine::body::{ROp, Slot},
};
use lang_frontend::{ast::ExpressionId, ownership_checking::UnitDropPoint, source::Span};

impl Builder<'_, '_> {
    pub(super) fn argument_owned_class(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<Slot>, LoweringError> {
        if self.pending_call_loans.is_empty() {
            return Ok(None);
        }
        let Ok(symbol) = self.named_container_symbol(expression, span) else {
            return Ok(None);
        };
        if self.borrowed.contains_key(&symbol)
            || self.narrowed_view(expression)?.is_some()
            || !self.is_class(self.expression_type(expression)?)
        {
            return Ok(None);
        }
        Ok(self.bindings.get(&symbol).copied().filter(|slot| {
            matches!(self.body.slots[slot.0], EntityType::Value(ty)
                if self.types.heap_payloads.contains_key(&ty))
        }))
    }

    /// `owner.f = v` on a plain class owner: a local, an Inout parameter or the Inout
    /// receiver (explicit or implicit `this`).
    pub(in crate::ssa::refine::build) fn field_assignment(
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
            let replaced =
                self.collect_drops(UnitDropPoint::BeforeReplacement(self.unit(assignment)));
            self.finish_drops(&replaced);
            return Ok(Lowered::Diverged);
        };
        let replacement = self.adapt(replacement, value, self.c(projection.ty()))?;
        self.deliver(value);
        let direct = match owner {
            OwnerRef::Expression(expression) => self.argument_owned_class(expression, span)?,
            OwnerRef::This(_) => None,
        };
        if let Some(owner) = direct {
            let field_type = self.c(projection.ty());
            let target = self.ssa_type(field_type, span)?;
            let loan = self.borrow_owned_heap_field(owner, field, target, span)?;
            let op = if self.move_only(field_type) {
                ROp::LoanReplace {
                    loan,
                    value: replacement,
                }
            } else {
                ROp::LoanMutate {
                    loan,
                    value: replacement,
                }
            };
            self.push(op, Vec::new(), span)?;
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
        } else {
            let loan = self.owner_loan(owner, LoanKind::Exclusive, span)?;
            let op = if self.is_value_class(owner_type) {
                ROp::InlineFieldReplace {
                    receiver: loan.loan,
                    field,
                    value: replacement,
                }
            } else {
                ROp::HeapFieldReplace {
                    receiver: loan.loan,
                    field,
                    value: replacement,
                }
            };
            self.push(op, Vec::new(), span)?;
            self.end_owner_loan(loan, span)?;
        }
        let replaced = self.collect_drops(UnitDropPoint::BeforeReplacement(self.unit(assignment)));
        self.finish_drops(&replaced);
        Ok(Lowered::Unit)
    }
}
