//! Member-call receivers in the refined CFG (SPEC-0358 step 3). A Borrow receiver is lent
//! before the arguments are evaluated; a Copyable Value receiver is copied; an Inout
//! receiver's exclusive loan is reserved during the arguments and activates at the call.
use lang_frontend::{
    source::Span,
    type_checking::{ParameterMode, UnitCallDescriptor, UnitCallReceiverOrigin, UnitExpressionId},
};

use super::{Builder, field::OwnerRef};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind},
    refine::body::{ROp, Slot},
};

/// A receiver evaluated so far: its call operand, or the owner of a reserved Inout loan.
#[derive(Clone, Debug)]
pub(super) enum Receiver {
    Ready {
        operand: Slot,
        loans: Vec<Slot>,
    },
    Reserved(OwnerRef),
    ReservedInline {
        place: Slot,
        target: crate::ssa::model::SsaTypeId,
        writeback: super::inout::Writeback,
    },
}

impl Builder<'_, '_> {
    pub(super) fn receiver_before_arguments(
        &mut self,
        call: UnitExpressionId,
        descriptor: &UnitCallDescriptor,
        span: Span,
    ) -> Result<Option<Receiver>, LoweringError> {
        let Some(receiver) = descriptor.receiver() else {
            return Ok(None);
        };
        if self
            .inputs
            .owned
            .conditional_receiver_delivery(call)
            .is_some()
        {
            let receiver_ty = self.c(receiver.ty());
            let operand = self.this_value_of(receiver_ty, span)?;
            if self.move_only(receiver_ty) {
                self.deliver_this();
            }
            return Ok(Some(Receiver::Ready {
                operand,
                loans: Vec::new(),
            }));
        }
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
                self.body.receiver_plans.enter(
                    call,
                    self.inputs.owned.receiver_fact(call),
                    &self.bindings,
                    span,
                )?;
                let begin_span = self
                    .inputs
                    .owned
                    .receiver_fact(call)
                    .map(|fact| fact.begin_span())
                    .unwrap_or(span);
                let loan = self.owner_loan(owner, LoanKind::Shared, begin_span)?;
                let loan = self.shared_operand(loan, begin_span)?;
                let mut loans = Vec::new();
                if loan.scoped {
                    loans.push(loan.loan);
                }
                let mut operand = loan.loan;
                if let Some(site) = self
                    .inputs
                    .source_plan
                    .call_site(self.inputs.source_token, call)
                {
                    for route in site.delegation() {
                        let field = self
                            .types
                            .field_indices
                            .get(&(route.outer_receiver(), route.field()))
                            .copied()
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                        let target = self.ssa_type(route.delegate_receiver(), span)?;
                        let delegate = self.value(
                            ROp::SharedHeapFieldLoan {
                                base: operand,
                                field,
                            },
                            EntityType::Loan {
                                kind: LoanKind::Shared,
                                target,
                            },
                            begin_span,
                        )?;
                        operand = delegate;
                        loans.push(delegate);
                    }
                }
                Receiver::Ready { operand, loans }
            }
            ParameterMode::Value => {
                let receiver_ty = self.c(receiver.ty());
                let operand = match owner {
                    OwnerRef::Expression(expression) => {
                        let slot = self
                            .required(expression)?
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                        if self.move_only(receiver_ty) {
                            self.deliver(expression);
                        }
                        slot
                    }
                    OwnerRef::This(_) => {
                        let slot = self.this_value_of(receiver_ty, span)?;
                        if self.move_only(receiver_ty) {
                            self.deliver_this();
                        }
                        slot
                    }
                };
                Receiver::Ready {
                    operand,
                    loans: Vec::new(),
                }
            }
            ParameterMode::Inout => {
                let receiver_ty = self.c(receiver.ty());
                if (self.is_value_class(receiver_ty) || self.is_enum(receiver_ty))
                    && let OwnerRef::Expression(mut expression) = owner
                {
                    while let (lang_frontend::parser::Expression::Group { expression: inner }, _) =
                        self.node(expression)?
                    {
                        expression = inner;
                    }
                    let (payload, expr_span) = self.node(expression)?;
                    if matches!(payload, lang_frontend::parser::Expression::Name) {
                        let symbol = self.reference(expr_span)?;
                        if let Some(&owner_slot) = self.bindings.get(&symbol) {
                            let EntityType::Value(target) = self.body.slots[owner_slot.0] else {
                                return Err(error(LoweringErrorKind::InvalidModel, span));
                            };
                            let place = self.value(
                                ROp::RootPlace { owner: owner_slot },
                                EntityType::Place(target),
                                expr_span,
                            )?;
                            return Ok(Some(Receiver::ReservedInline {
                                place,
                                target,
                                writeback: super::inout::Writeback::new(
                                    symbol,
                                    place,
                                    self.move_only(receiver_ty),
                                    span,
                                ),
                            }));
                        }
                    }
                }
                Receiver::Reserved(owner)
            }
        }))
    }

    /// The receiver operand at the call; a reserved Inout loan activates here.
    pub(super) fn receiver_at_call(
        &mut self,
        receiver: Receiver,
        span: Span,
    ) -> Result<(Slot, Vec<Slot>, Option<super::inout::Writeback>), LoweringError> {
        match receiver {
            Receiver::Ready { operand, loans } => Ok((operand, loans, None)),
            Receiver::Reserved(owner) => {
                let loan = self.owner_loan(owner, LoanKind::Exclusive, span)?;
                let mut loans = Vec::new();
                if loan.scoped {
                    loans.push(loan.loan);
                }
                Ok((loan.loan, loans, None))
            }
            Receiver::ReservedInline {
                place,
                target,
                writeback,
            } => {
                let loan = self.value(
                    ROp::BorrowBegin {
                        place,
                        kind: LoanKind::Exclusive,
                    },
                    EntityType::Loan {
                        kind: LoanKind::Exclusive,
                        target,
                    },
                    span,
                )?;
                Ok((loan, vec![loan], Some(writeback)))
            }
        }
    }
}
