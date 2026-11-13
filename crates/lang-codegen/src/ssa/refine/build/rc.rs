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
                                let EntityType::Value(target) = self.body.slots[owner_slot.0]
                                else {
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
        ) -> Result<(Slot, Vec<Slot>, Option<super::super::inout::Writeback>), LoweringError>
        {
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

#[cfg(test)]
pub(in crate::ssa) mod map_temporary_cleanup {
    //! Exact temporary owner, lexical Guard and complete point sequences from Phase 3.
    use crate::ssa::lower_frontend::map_iteration::*;
    use lang_frontend::ownership_checking::{
        CleanupOwnerValue, DropFact, DropTarget, IterationPhiBoundary, IterationPhiIncomingKind,
    };

    pub(in crate::ssa) fn prove(
        parsed: &ParsedFile,
        owned: &OwnershipCheckedFile,
        plan: &MapIterationOwnershipPlan,
        body: StatementId,
        kind: IterationExitKind,
        guard: Option<guard::Local>,
        span: Span,
    ) -> Result<source_owner::Proof, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
        let descriptor = plan.descriptor();
        let statement = descriptor.statement();
        let lang_frontend::ownership_checking::IterationSource::BorrowedSource(
            LoanTarget::Temporary(source),
        ) = plan.source_kind()
        else {
            return Err(missing());
        };
        let source = *source;
        let origin = parsed
            .ast()
            .expressions()
            .get(source)
            .map_err(|_| missing())?
            .span();
        if kind == IterationExitKind::Fallthrough {
            return normal(owned, plan, body, origin, span);
        }
        let body_point = exit_shape::point(kind, statement, body);
        let exhaustion_point = DropPoint::LoopExit(statement);
        if !plan.closure_phis().is_empty()
            || !plan.closure_flow().header().is_empty()
            || !plan.closure_flow().exit().is_empty()
            || !plan.capture_graph().nodes().is_empty()
        {
            return Err(unsupported());
        }
        let conditions = owned.cleanup_conditions();
        let mut incomings = vec![(
            IterationPhiIncomingKind::Entry,
            DropPoint::AfterExpression(descriptor.source()),
            IterationPhiBoundary::Header,
        )];
        match kind {
            IterationExitKind::Fallthrough => incomings.push((
                IterationPhiIncomingKind::Fallthrough,
                body_point,
                IterationPhiBoundary::Header,
            )),
            IterationExitKind::Continue(expression) => incomings.push((
                IterationPhiIncomingKind::Continue(expression),
                body_point,
                IterationPhiBoundary::Header,
            )),
            IterationExitKind::Break(expression) => incomings.push((
                IterationPhiIncomingKind::Break(expression),
                body_point,
                IterationPhiBoundary::Exit,
            )),
            IterationExitKind::Return(_) => {}
            IterationExitKind::Exhaustion => return Err(missing()),
        }
        incomings.push((
            IterationPhiIncomingKind::Exhaustion,
            exhaustion_point,
            IterationPhiBoundary::Exit,
        ));
        for &(expected_kind, point, boundary) in &incomings {
            let incoming = unique(
                plan.closure_phi_incomings()
                    .iter()
                    .filter(|incoming| incoming.kind() == expected_kind),
            )
            .ok_or_else(missing)?;
            if incoming.point() != point
                || incoming.boundary() != boundary
                || conditions.get(incoming.condition()) != Some(&CleanupCondition::Always)
                || !incoming.bindings().is_empty()
            {
                return Err(missing());
            }
        }
        if plan.closure_phi_incomings().len() != incomings.len() {
            return Err(missing());
        }
        let source_drops = owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.target() == DropTarget::Temporary(source))
            .collect::<Vec<_>>();
        let drop = unique(
            source_drops
                .iter()
                .copied()
                .filter(|fact| fact.point() == exhaustion_point),
        )
        .ok_or_else(missing)?;
        let owner = drop.owner().ok_or_else(missing)?;
        if conditions.owner_value(owner)
            != Some(&CleanupOwnerValue::Expression {
                expression: source,
                origin,
            })
            || source_drops.iter().any(|fact| {
                fact.owner() != Some(owner) || fact.value_origin() != origin || !ordinary(*fact)
            })
        {
            return Err(missing());
        }
        let terminal_body = exit_shape::terminal(kind);
        let transfer = if terminal_body {
            Some(
                unique(
                    source_drops
                        .iter()
                        .copied()
                        .filter(|fact| fact.point() == body_point),
                )
                .ok_or_else(missing)?,
            )
        } else {
            None
        };
        if source_drops.iter().any(|fact| {
            fact.point() != exhaustion_point && (!terminal_body || fact.point() != body_point)
        }) {
            return Err(missing());
        }
        if source_drops.len() != 1 + usize::from(terminal_body)
            || owned
                .drops()
                .iter()
                .filter(|fact| fact.owner() == Some(owner))
                .copied()
                .collect::<Vec<_>>()
                != source_drops
        {
            return Err(missing());
        }
        let local_drop = guard
            .map(|guard| guard_drop(owned, guard, body_point, span))
            .transpose()?;
        if (kind == IterationExitKind::Fallthrough) != guard.is_none()
            || local_drop.is_some_and(|fact| fact.owner() == Some(owner))
        {
            return Err(missing());
        }
        exit_shape::cleanup(owned, plan, body, kind, span)?;
        let terminal = vec![
            IterationCleanupAction::FinishProvider(statement),
            IterationCleanupAction::EndSource(statement),
            IterationCleanupAction::Drop(drop),
        ];
        let mut body_actions = local_drop
            .into_iter()
            .map(IterationCleanupAction::Drop)
            .collect::<Vec<_>>();
        body_actions.extend([
            IterationCleanupAction::EndBinding {
                statement,
                symbol: descriptor.value_binding(),
            },
            IterationCleanupAction::EndBinding {
                statement,
                symbol: descriptor.key_binding(),
            },
        ]);
        if let Some(transfer) = transfer {
            body_actions.extend([
                IterationCleanupAction::FinishProvider(statement),
                IterationCleanupAction::EndSource(statement),
                IterationCleanupAction::Drop(transfer),
            ]);
        }
        for (expected_kind, point, actions) in [
            (kind, body_point, body_actions),
            (IterationExitKind::Exhaustion, exhaustion_point, terminal),
        ] {
            let exit = unique(
                plan.exits()
                    .iter()
                    .filter(|exit| exit.kind() == expected_kind),
            )
            .ok_or_else(missing)?;
            if exit.actions() != actions {
                if kind == IterationExitKind::Fallthrough && expected_kind == kind {
                    return Err(unsupported());
                }
                return Err(missing());
            }
            let expected = actions
                .iter()
                .filter_map(|action| match action {
                    IterationCleanupAction::Drop(fact) => Some(*fact),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let actual = owned
                .drops()
                .iter()
                .copied()
                .filter(|fact| fact.point() == point)
                .collect::<Vec<_>>();
            if actual.len() != expected.len() || expected.iter().any(|fact| !actual.contains(fact))
            {
                return Err(missing());
            }
        }
        Ok(source_owner::Proof {
            flow: None,
            disposition: source_owner::Disposition::Drop(drop),
            transfer,
            conditional_return: None,
        })
    }

    fn ordinary(fact: DropFact) -> bool {
        fact.condition().is_none()
            && fact.capture_slot().is_none()
            && fact.instance_address().is_none()
    }

    fn guard_drop(
        owned: &OwnershipCheckedFile,
        guard: guard::Local,
        point: DropPoint,
        span: Span,
    ) -> Result<DropFact, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let fact = unique(
            owned
                .drops()
                .iter()
                .copied()
                .filter(|fact| fact.target() == DropTarget::Named(guard.symbol)),
        )
        .ok_or_else(missing)?;
        let owner = fact.owner().ok_or_else(missing)?;
        if fact.point() != point
            || fact.value_origin() != guard.origin
            || !ordinary(fact)
            || owned.cleanup_conditions().owner_value(owner)
                != Some(&CleanupOwnerValue::Expression {
                    expression: guard.initializer,
                    origin: guard.initializer_span,
                })
            || !matches!(owned.drops().iter().filter(|fact| fact.owner() == Some(owner))
                .collect::<Vec<_>>().as_slice(), [actual] if **actual == fact)
        {
            return Err(missing());
        }
        Ok(fact)
    }

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa) fn emit_temporary_map_local_drop(
            &mut self,
            fact: DropFact,
            point: DropPoint,
            span: Span,
        ) -> Result<(), LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let DropTarget::Named(symbol) = fact.target() else {
                return Err(missing());
            };
            let Some(LoweredValue::Value(owner)) = self.bindings.get(&symbol).copied() else {
                return Err(missing());
            };
            let Some(CleanupOwnerValue::Expression { expression, origin }) = fact
                .owner()
                .and_then(|owner| self.owned.cleanup_conditions().owner_value(owner))
            else {
                return Err(missing());
            };
            let construction = self.typed.construction(*expression).ok_or_else(missing)?;
            let ty = self.expression_ssa_type(*expression, span)?;
            let resolved = self.resolve_type(construction.result_type(), span)?;
            let birth =
                carrier::definition(self.function, EntityId::Value(owner)).ok_or_else(missing)?;
            let Operation::HeapAllocate {
                owner: owner_type,
                payload,
            } = birth.operation
            else {
                return Err(missing());
            };
            let payload =
                carrier::definition(self.function, EntityId::Value(payload)).ok_or_else(missing)?;
            let aggregate = self.heap_payloads.get(&ty).ok_or_else(missing)?;
            if fact.point() != point
                || !ordinary(fact)
                || construction.expression() != *expression
                || Some(construction.result_type()) != self.typed.symbol_type(symbol)
                || birth.block != self.block
                || birth.origin != Origin::Source(*origin)
                || owner_type != ty
                || self.type_ids.get(&resolved) != Some(&ty)
                || self
                    .function
                    .entity(EntityId::Value(owner))
                    .map(|data| data.ty)
                    != Some(EntityType::Value(ty))
                || payload.block != self.block
                || payload.origin != birth.origin
                || !matches!(&payload.operation, Operation::AggregateConstruct { aggregate: actual, fields }
                    if actual == aggregate && fields.is_empty())
                || unique(self.function.instructions.iter().filter(|instruction| {
                    instruction.origin == birth.origin
                        && matches!(instruction.operation, Operation::HeapAllocate { .. })
                }))
                .map(|instruction| instruction.id)
                    != Some(birth.id)
            {
                return Err(missing());
            }
            let before = self
                .function
                .block(self.block)
                .ok_or_else(missing)?
                .instructions
                .len();
            self.emit_iteration_cleanup_action(&IterationCleanupAction::Drop(fact), span)?;
            let block = self.function.block(self.block).ok_or_else(missing)?;
            let [instruction] = block.instructions.get(before..).ok_or_else(missing)? else {
                return Err(missing());
            };
            let instruction = self
                .function
                .instruction(*instruction)
                .ok_or_else(missing)?;
            if instruction.operation != (Operation::Drop { owner })
                || instruction.origin != Origin::Source(fact.value_origin())
                || self.bindings.contains_key(&symbol)
                || block
                    .instructions
                    .iter()
                    .filter(|id| {
                        self.function.instruction(**id).is_some_and(|instruction| {
                            instruction.operation == (Operation::Drop { owner })
                        })
                    })
                    .count()
                    != 1
            {
                return Err(missing());
            }
            Ok(())
        }
    }

    fn normal(
        owned: &OwnershipCheckedFile,
        plan: &MapIterationOwnershipPlan,
        body: StatementId,
        origin: Span,
        span: Span,
    ) -> Result<source_owner::Proof, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
        let descriptor = plan.descriptor();
        let statement = descriptor.statement();
        let lang_frontend::ownership_checking::IterationSource::BorrowedSource(
            LoanTarget::Temporary(source),
        ) = plan.source_kind()
        else {
            return Err(missing());
        };
        let source = *source;
        if !plan.closure_phis().is_empty()
            || !plan.closure_flow().header().is_empty()
            || !plan.closure_flow().exit().is_empty()
            || !plan.capture_graph().nodes().is_empty()
        {
            return Err(unsupported());
        }
        let conditions = owned.cleanup_conditions();
        let incoming_shapes = [
            (
                IterationPhiIncomingKind::Entry,
                DropPoint::AfterExpression(descriptor.source()),
                IterationPhiBoundary::Header,
            ),
            (
                IterationPhiIncomingKind::Fallthrough,
                DropPoint::AfterStatement(body),
                IterationPhiBoundary::Header,
            ),
            (
                IterationPhiIncomingKind::Exhaustion,
                DropPoint::LoopExit(statement),
                IterationPhiBoundary::Exit,
            ),
        ];
        for (kind, point, boundary) in incoming_shapes {
            let incoming = unique(
                plan.closure_phi_incomings()
                    .iter()
                    .filter(|incoming| incoming.kind() == kind),
            )
            .ok_or_else(missing)?;
            if incoming.point() != point
                || incoming.boundary() != boundary
                || conditions.get(incoming.condition()) != Some(&CleanupCondition::Always)
                || !incoming.bindings().is_empty()
            {
                return Err(missing());
            }
        }
        if plan.closure_phi_incomings().len() != incoming_shapes.len() {
            return Err(unsupported());
        }
        let drop = unique(
            owned
                .drops()
                .iter()
                .copied()
                .filter(|fact| fact.target() == DropTarget::Temporary(source)),
        )
        .ok_or_else(missing)?;
        if drop.point() != DropPoint::LoopExit(statement)
            || drop.value_origin() != origin
            || drop.condition().is_some()
            || drop.capture_slot().is_some()
            || drop.instance_address().is_some()
        {
            return Err(missing());
        }
        let owner = drop.owner().ok_or_else(missing)?;
        if conditions.owner_value(owner)
            != Some(&CleanupOwnerValue::Expression {
                expression: source,
                origin,
            })
            || owned
                .drops()
                .iter()
                .any(|fact| fact.owner() == Some(owner) && *fact != drop)
        {
            return Err(missing());
        }
        exit_shape::cleanup(owned, plan, body, IterationExitKind::Fallthrough, span)?;
        let body_plan = unique(
            plan.exits()
                .iter()
                .filter(|exit| exit.kind() == IterationExitKind::Fallthrough),
        )
        .ok_or_else(missing)?;
        let exhaustion = unique(
            plan.exits()
                .iter()
                .filter(|exit| exit.kind() == IterationExitKind::Exhaustion),
        )
        .ok_or_else(missing)?;
        let body_actions = [
            IterationCleanupAction::EndBinding {
                statement,
                symbol: descriptor.value_binding(),
            },
            IterationCleanupAction::EndBinding {
                statement,
                symbol: descriptor.key_binding(),
            },
        ];
        if body_plan.actions() != body_actions {
            return Err(unsupported());
        }
        if exhaustion.actions()
            != [
                IterationCleanupAction::FinishProvider(statement),
                IterationCleanupAction::EndSource(statement),
                IterationCleanupAction::Drop(drop),
            ]
        {
            return Err(missing());
        }
        Ok(source_owner::Proof {
            flow: None,
            disposition: source_owner::Disposition::Drop(drop),
            transfer: None,
            conditional_return: None,
        })
    }
}
