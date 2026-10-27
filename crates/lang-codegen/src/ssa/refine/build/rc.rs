//! Intrinsic `Rc<T>` in the refined CFG (SPEC-0359): construction allocates the shared
//! owner, `share()` retains it, and `value` reads the payload through its place.
use lang_frontend::{
    ast::ExpressionId,
    parser::Expression,
    source::Span,
    type_checking::{RcOperationKind, UnitConstructionTarget},
};

use super::{Builder, Lowered, place::OwnerLoan};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind},
    refine::body::{ROp, Slot},
};

impl Builder<'_, '_> {
    /// `Rc(payload)`: the payload is delivered into a fresh shared owner.
    pub(super) fn rc_construction(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .construction(id)
            .filter(|descriptor| {
                descriptor.expression() == id
                    && descriptor.target() == UnitConstructionTarget::IntrinsicRc
            })
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let [argument] = descriptor.arguments() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let value = argument.argument().expression();
        let Some(payload) = self.required(value)? else {
            return Ok(Lowered::Diverged);
        };
        self.deliver(value);
        let owner = self.ssa_type(descriptor.result_type(), span)?;
        Ok(Lowered::Value(self.value(
            ROp::SharedAllocate { owner, payload },
            EntityType::Value(owner),
            span,
        )?))
    }

    /// The shared owner an `Rc` operation reads: an owned binding.
    fn rc_owner(&self, receiver: ExpressionId) -> Result<Slot, LoweringError> {
        let (payload, span) = self.node(receiver)?;
        let Expression::Name = payload else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let symbol = self.reference(span)?;
        self.bindings
            .get(&symbol)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
    }

    /// `owner.share()` retains a new strong owner; `owner.value` of a Copyable payload is
    /// read by copy.
    pub(super) fn rc_operation(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let descriptor = self
            .inputs
            .typed
            .rc_operation(self.unit(expression))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let receiver = descriptor.receiver();
        if receiver.source_unit() != self.inputs.source_unit {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let owner = self.rc_owner(receiver.expression())?;
        let ty = self.ssa_type(self.expression_type(expression)?, span)?;
        let result = match descriptor.kind() {
            RcOperationKind::Share => {
                self.value(ROp::SharedRetain { owner }, EntityType::Value(ty), span)?
            }
            RcOperationKind::Value => {
                let place = self.value(
                    ROp::SharedPayloadPlace { owner },
                    EntityType::Place(ty),
                    span,
                )?;
                self.value(ROp::ReadPlace { place }, EntityType::Value(ty), span)?
            }
        };
        Ok(Lowered::Value(result))
    }

    /// `owner.value` as a field-projection receiver: a shared loan of the payload place,
    /// ending right after the read.
    pub(super) fn rc_payload_loan(
        &mut self,
        receiver: ExpressionId,
        span: Span,
    ) -> Result<OwnerLoan, LoweringError> {
        let descriptor = self
            .inputs
            .typed
            .rc_operation(self.unit(receiver))
            .filter(|descriptor| descriptor.kind() == RcOperationKind::Value)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let owner = self.rc_owner(descriptor.receiver().expression())?;
        let target = self.ssa_type(descriptor.payload_type(), span)?;
        let place = self.value(
            ROp::SharedPayloadPlace { owner },
            EntityType::Place(target),
            span,
        )?;
        let loan = self.value(
            ROp::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            },
            span,
        )?;
        Ok(OwnerLoan { loan, scoped: true })
    }
}
