//! Loans of class owners for field access and receivers (SPEC-0358 step 3): a local owner
//! is lent for the access only; a Borrow/Inout parameter or the current receiver already
//! arrives as a loan of the right kind.
use lang_frontend::{
    ast::ExpressionId, name_resolution::DeclarationId, parser::Expression, source::Span,
};

use super::Builder;
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind},
    refine::body::{ROp, Slot},
};

/// The owner a field access or receiver goes through.
#[derive(Clone, Copy, Debug)]
pub(super) enum OwnerRef {
    /// An explicit receiver expression: a name or `this`.
    Expression(ExpressionId),
    /// The implicit `this` of the current member of `DeclarationId`.
    This(DeclarationId),
}

/// A loan of an owner; `scoped` loans were begun for this access and end right after it.
#[derive(Clone, Copy, Debug)]
pub(super) struct OwnerLoan {
    pub(super) loan: Slot,
    pub(super) scoped: bool,
}

impl Builder<'_, '_> {
    /// A loan of `owner` of at least `kind`: an exclusive loan also serves a read.
    pub(super) fn owner_loan(
        &mut self,
        owner: OwnerRef,
        kind: LoanKind,
        span: Span,
    ) -> Result<OwnerLoan, LoweringError> {
        let existing = match owner {
            OwnerRef::This(declaration) => {
                if self.inputs.receiver.map(|receiver| receiver.owner) != Some(declaration) {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                self.receiver
            }
            OwnerRef::Expression(expression) => {
                let (payload, expression_span) = self.node(expression)?;
                match payload {
                    Expression::This => self.receiver,
                    Expression::Name => {
                        if let Some(view) = self.narrowed_view(expression)? {
                            if kind == LoanKind::Shared {
                                return Ok(OwnerLoan {
                                    loan: view,
                                    scoped: false,
                                });
                            }
                            return Err(error(LoweringErrorKind::InvalidModel, span));
                        }
                        let symbol = self.reference(expression_span)?;
                        match self.borrowed.get(&symbol).copied() {
                            Some(loan) => Some(loan),
                            None => {
                                let owner = *self.bindings.get(&symbol).ok_or_else(|| {
                                    error(LoweringErrorKind::MissingFact, expression_span)
                                })?;
                                return self.lend(owner, kind, span);
                            }
                        }
                    }
                    _ => {
                        let owner = self
                            .required(expression)?
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                        return self.lend(owner, kind, span);
                    }
                }
            }
        };
        let loan = existing.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        match (self.body.slots[loan.0], kind) {
            (EntityType::Loan { kind: held, .. }, wanted)
                if held == wanted || held == LoanKind::Exclusive =>
            {
                Ok(OwnerLoan {
                    loan,
                    scoped: false,
                })
            }
            _ => Err(error(LoweringErrorKind::MissingFact, span)),
        }
    }

    /// Lends an owned binding's root place for one access.
    pub(super) fn lend(
        &mut self,
        owner: Slot,
        kind: LoanKind,
        span: Span,
    ) -> Result<OwnerLoan, LoweringError> {
        let EntityType::Value(target) = self.body.slots[owner.0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let place = self.value(ROp::RootPlace { owner }, EntityType::Place(target), span)?;
        let loan = self.value(
            ROp::BorrowBegin { place, kind },
            EntityType::Loan { kind, target },
            span,
        )?;
        Ok(OwnerLoan { loan, scoped: true })
    }

    /// A shared call operand: an exclusive loan held by this function is narrowed to a
    /// call-scoped shared reborrow.
    pub(super) fn shared_operand(
        &mut self,
        loan: OwnerLoan,
        span: Span,
    ) -> Result<OwnerLoan, LoweringError> {
        let EntityType::Loan { kind, target } = self.body.slots[loan.loan.0] else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        if kind == LoanKind::Shared {
            return Ok(loan);
        }
        let reborrow = self.value(
            ROp::SharedReborrow { source: loan.loan },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            },
            span,
        )?;
        Ok(OwnerLoan {
            loan: reborrow,
            scoped: true,
        })
    }

    /// Ends a loan begun for one access.
    pub(super) fn end_owner_loan(
        &mut self,
        loan: OwnerLoan,
        span: Span,
    ) -> Result<(), LoweringError> {
        if loan.scoped {
            self.push(ROp::BorrowEnd { loan: loan.loan }, Vec::new(), span)?;
        }
        Ok(())
    }
}
