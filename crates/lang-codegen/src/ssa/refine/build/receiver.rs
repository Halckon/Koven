//! Member-call receivers in the refined CFG (SPEC-0358 step 3). A Borrow receiver is lent
//! before the arguments are evaluated; a Copyable Value receiver is copied; an Inout
//! receiver's exclusive loan is reserved during the arguments and activates at the call.
use lang_frontend::{
    source::Span,
    type_checking::{ParameterMode, UnitCallDescriptor, UnitCallReceiverOrigin},
};

use super::{
    Builder,
    place::{OwnerLoan, OwnerRef},
};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::LoanKind,
    refine::body::Slot,
};

/// A receiver evaluated so far: its call operand, or the owner of a reserved Inout loan.
#[derive(Clone, Copy, Debug)]
pub(super) enum Receiver {
    Ready {
        operand: Slot,
        loan: Option<OwnerLoan>,
    },
    Reserved(OwnerRef),
}

impl Builder<'_, '_> {
    pub(super) fn receiver_before_arguments(
        &mut self,
        descriptor: &UnitCallDescriptor,
        span: Span,
    ) -> Result<Option<Receiver>, LoweringError> {
        let Some(receiver) = descriptor.receiver() else {
            return Ok(None);
        };
        let owner = match receiver.origin() {
            UnitCallReceiverOrigin::Expression(expression) => {
                if expression.source_unit() != self.inputs.source_unit {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                OwnerRef::Expression(expression.expression())
            }
            UnitCallReceiverOrigin::ImplicitThis(declaration) => OwnerRef::This(declaration),
        };
        Ok(Some(match receiver.mode() {
            ParameterMode::Borrow => {
                let loan = self.owner_loan(owner, LoanKind::Shared, span)?;
                let loan = self.shared_operand(loan, span)?;
                Receiver::Ready {
                    operand: loan.loan,
                    loan: Some(loan),
                }
            }
            ParameterMode::Value => {
                let operand = match owner {
                    OwnerRef::Expression(expression) => self
                        .required(expression)?
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                    OwnerRef::This(_) => self.this_value_of(self.c(receiver.ty()), span)?,
                };
                Receiver::Ready {
                    operand,
                    loan: None,
                }
            }
            ParameterMode::Inout => Receiver::Reserved(owner),
        }))
    }

    /// The receiver operand at the call; a reserved Inout loan activates here.
    pub(super) fn receiver_at_call(
        &mut self,
        receiver: Receiver,
        span: Span,
    ) -> Result<(Slot, Option<OwnerLoan>), LoweringError> {
        match receiver {
            Receiver::Ready { operand, loan } => Ok((operand, loan)),
            Receiver::Reserved(owner) => {
                let loan = self.owner_loan(owner, LoanKind::Exclusive, span)?;
                Ok((loan.loan, Some(loan)))
            }
        }
    }
}
