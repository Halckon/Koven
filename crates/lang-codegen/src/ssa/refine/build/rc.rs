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
        let owner = self.ssa_type(self.c(descriptor.result_type()), span)?;
        Ok(Lowered::Value(self.value(
            ROp::SharedAllocate { owner, payload },
            EntityType::Value(owner),
            span,
        )?))
    }

    /// The shared owner an `Rc` operation reads: an owned binding or a proven non-null view.
    fn rc_owner(
        &self,
        receiver: ExpressionId,
    ) -> Result<(Slot, Option<ExpressionId>), LoweringError> {
        let mut root = receiver;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        if let Some(view) = self.narrowed_view(root)? {
            return Ok((view, Some(receiver)));
        }
        let (payload, span) = self.node(root)?;
        let Expression::Name = payload else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let symbol = self.reference(span)?;
        let slot = self
            .bindings
            .get(&symbol)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        Ok((slot, None))
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
        let (owner, view_read) = self.rc_owner(receiver.expression())?;
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
        if let Some(read) = view_read {
            self.drops(
                lang_frontend::ownership_checking::UnitDropPoint::AfterExpression(self.unit(read)),
            )?;
        }
        Ok(Lowered::Value(result))
    }

    /// `owner.value` as a field-projection receiver: a shared loan of the payload place,
    /// ending right after the read.
    pub(super) fn rc_payload_loan(
        &mut self,
        receiver: ExpressionId,
        span: Span,
    ) -> Result<(OwnerLoan, Option<ExpressionId>), LoweringError> {
        let descriptor = self
            .inputs
            .typed
            .rc_operation(self.unit(receiver))
            .filter(|descriptor| descriptor.kind() == RcOperationKind::Value)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (owner, view_read) = self.rc_owner(descriptor.receiver().expression())?;
        let target = self.ssa_type(self.c(descriptor.payload_type()), span)?;
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
        Ok((OwnerLoan { loan, scoped: true }, view_read))
    }

    /// `owner.value` of a MoveOnly payload lent to a Borrow call parameter.
    pub(super) fn borrowed_rc_payload_loan(
        &mut self,
        expression: ExpressionId,
        parameter: lang_frontend::type_checking::UnitTypeId,
        span: Span,
    ) -> Result<Option<super::callable::LentArgument>, LoweringError> {
        if !self.move_only(parameter) {
            return Ok(None);
        }
        let mut root = expression;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let id = self.unit(root);
        let Some(descriptor) = self
            .inputs
            .typed
            .rc_operation(id)
            .filter(|descriptor| descriptor.kind() == RcOperationKind::Value)
        else {
            return Ok(None);
        };
        if self.c(descriptor.payload_type()) != parameter {
            return Ok(None);
        }
        let (owner_loan, view_read) = self.rc_payload_loan(root, span)?;
        Ok(Some((owner_loan.loan, vec![owner_loan.loan], view_read)))
    }
}
