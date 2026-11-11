//! Intrinsic `Rc<T>` (SPEC-0359), `RangeView` reads, and member-call receivers in the refined CFG.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{RangeUseSite, UnitDropPoint},
    parser::Expression,
    source::Span,
    type_checking::{
        ParameterMode, RcOperationKind, UnitCallDescriptor, UnitCallReceiverOrigin,
        UnitConstructionTarget, UnitExpressionId,
    },
};

use super::{
    Builder, Lowered,
    field::{OwnerLoan, OwnerRef},
};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind, SsaTypeId},
    refine::body::{ROp, Slot},
};

pub(in crate::ssa::refine::build) mod receiver {
    use super::*;

    #[derive(Clone, Debug)]
    pub(in crate::ssa::refine::build) enum Receiver {
        Ready {
            operand: Slot,
            loans: Vec<Slot>,
        },
        Reserved(OwnerRef),
        ReservedInline {
            place: Slot,
            target: SsaTypeId,
            writeback: super::super::inout::Writeback,
        },
    }

    impl Builder<'_, '_> {
        pub(in crate::ssa::refine::build) fn receiver_before_arguments(
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
                        while let (Expression::Group { expression: inner }, _) =
                            self.node(expression)?
                        {
                            expression = inner;
                        }
                        let (payload, expr_span) = self.node(expression)?;
                        if matches!(payload, Expression::Name) {
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
                                    writeback: super::super::inout::Writeback::new(
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

        pub(in crate::ssa::refine::build) fn receiver_at_call(
            &mut self,
            receiver: Receiver,
            span: Span,
        ) -> Result<(Slot, Vec<Slot>, Option<super::super::inout::Writeback>), LoweringError> {
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
}

impl Builder<'_, '_> {
    pub(super) fn range_iteration_loan(
        &mut self,
        source: ExpressionId,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<Option<Slot>, LoweringError> {
        let actual = self.ungrouped_expr(source)?;
        let id = self.unit(actual);
        if self
            .inputs
            .owned
            .borrow_results()
            .range_uses()
            .iter()
            .any(|fact| fact.expression() == id && fact.site() == RangeUseSite::Iteration)
        {
            let Some((view, protecting, continued)) = self.lower_range_call(actual, span)? else {
                return Ok(None);
            };
            self.short_ranges.insert(id, (view, protecting, continued));
            let place = self.value(ROp::RootPlace { owner: view }, EntityType::Place(ty), span)?;
            let loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                },
                span,
            )?;
            return Ok(Some(loan));
        }
        let symbol = self.named_container_symbol(source, span)?;
        if let Some(&view) = self.range_views.get(&symbol) {
            let place = self.value(ROp::RootPlace { owner: view }, EntityType::Place(ty), span)?;
            let loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                },
                span,
            )?;
            return Ok(Some(loan));
        }
        let caller_loan = *self
            .borrowed
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let loan = self.value(
            ROp::SharedReborrow {
                source: caller_loan,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: ty,
            },
            span,
        )?;
        Ok(Some(loan))
    }

    pub(super) fn lower_range_size(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .range_size(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let receiver = descriptor.receiver().expression();
        let view = if let Some(loan) = self.this_receiver_loan(receiver)? {
            loan
        } else {
            let symbol = self.named_container_symbol(receiver, span)?;
            self.range_views
                .get(&symbol)
                .or_else(|| self.borrowed.get(&symbol))
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
        };
        let result_ty = self.ssa_type(self.expression_type(expression)?, span)?;
        let result = self.value(
            ROp::RangeLength { view },
            EntityType::Value(result_ty),
            span,
        )?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Value(result))
    }

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
            self.drops(UnitDropPoint::AfterExpression(self.unit(read)))?;
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
