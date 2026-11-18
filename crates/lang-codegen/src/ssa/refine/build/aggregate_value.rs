//! Aggregate construction, projection, and non-null value lowering.
use super::{field::OwnerRef, *};
use crate::ssa::{model::LoanKind, refine::body::RAbort};
use lang_frontend::{
    ownership_checking::NonNullAssertionTransferKind,
    type_checking::{RcOperationKind, UnitAggregateProjectionReceiver, UnitConstructionTarget},
};

impl Builder<'_, '_> {
    /// `Box(payload)` allocates a fresh heap box owner for the single payload argument.
    pub(super) fn box_construction(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .construction(id)
            .filter(|descriptor| descriptor.expression() == id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.target() != UnitConstructionTarget::IntrinsicBox {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let [argument] = descriptor.arguments() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if argument.argument().source_unit() != self.inputs.source_unit {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let value_expression = argument.argument().expression();
        let Some(value) = self.required(value_expression)? else {
            return Ok(Lowered::Diverged);
        };
        let payload_ty = self.c(argument.parameter_type());
        let mut payload = self.adapt(value, value_expression, payload_ty)?;
        let payload_ssa = self.ssa_type(payload_ty, span)?;
        if argument.category() == lang_frontend::type_checking::ExpressionCategory::Place
            && !self.move_only(payload_ty)
        {
            payload = self.value(
                ROp::Copy { source: payload },
                EntityType::Value(payload_ssa),
                self.span(value_expression),
            )?;
        } else {
            self.deliver(value_expression);
        }
        let owner = self.ssa_type(self.c(descriptor.result_type()), span)?;
        Ok(Lowered::Value(self.value(
            ROp::HeapAllocate { owner, payload },
            EntityType::Value(owner),
            span,
        )?))
    }

    /// Fields evaluate in source order and land in declaration order; the payload is then
    /// built and moved into a fresh heap owner.
    pub(super) fn construction(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .construction(id)
            .filter(|descriptor| descriptor.expression() == id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let UnitConstructionTarget::Nominal(_) = descriptor.target() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let mut arguments = descriptor.arguments().iter().collect::<Vec<_>>();
        arguments.sort_by_key(|argument| argument.evaluation_index());
        let mut fields: Vec<Option<Slot>> = vec![None; arguments.len()];
        let mut delivered = Vec::with_capacity(arguments.len());
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            if argument.argument().source_unit() != self.inputs.source_unit {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let value_expression = argument.argument().expression();
            let Some(value) = self.required(value_expression)? else {
                for skipped in arguments {
                    self.unreachable_drop_scopes
                        .push(self.span(skipped.argument().expression()));
                }
                return Ok(Lowered::Diverged);
            };
            let value = self.adapt(value, value_expression, self.c(argument.parameter_type()))?;
            self.repoint_temporary(value_expression, value);
            delivered.push(value_expression);
            let slot = fields
                .get_mut(argument.parameter_index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if slot.replace(value).is_some() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        for value_expression in delivered {
            self.deliver(value_expression);
        }
        let fields = fields
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let owner = self.ssa_type(self.c(descriptor.result_type()), span)?;
        let aggregate = self
            .types
            .heap_payloads
            .get(&owner)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let payload = self.value(
            ROp::AggregateConstruct { aggregate, fields },
            EntityType::Value(aggregate),
            span,
        )?;
        Ok(Lowered::Value(self.value(
            ROp::HeapAllocate { owner, payload },
            EntityType::Value(owner),
            span,
        )?))
    }

    /// A Copyable field read through a shared loan: the receiver's proof view, the loan a
    /// parameter or the current receiver arrived as, or a loan of the owned binding that
    /// ends right after the read.
    pub(super) fn projection(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let projection = self
            .inputs
            .typed
            .aggregate_projection(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let result_type = self.ssa_type(self.expression_type(expression)?, span)?;
        let is_nested = if let UnitAggregateProjectionReceiver::Expression(receiver) =
            projection.receiver()
            && receiver.source_unit() == self.inputs.source_unit
        {
            let mut ungrouped = receiver.expression();
            while let (Expression::Group { expression: inner }, _) = self.node(ungrouped)? {
                ungrouped = inner;
            }
            self.inputs
                .typed
                .aggregate_projection(self.unit(ungrouped))
                .is_some()
        } else {
            false
        };
        if is_nested {
            let (leaf_loan, created, drops) = self.field_projection_loan(expression, span)?;
            let result = self.value(
                ROp::ReadLoan { loan: leaf_loan },
                EntityType::Value(result_type),
                span,
            )?;
            for loan in created.into_iter().rev() {
                self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
            }
            for receiver_expr in drops {
                self.drops(UnitDropPoint::AfterExpression(self.unit(receiver_expr)))?;
            }
            return Ok(Lowered::Value(result));
        }
        let (owner, owner_type, receiver) = self.projection_owner(projection.receiver(), span)?;
        let field = self
            .types
            .field_indices
            .get(&(owner_type, projection.field()))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let mut rc_root = receiver;
        while let Some(root) = rc_root
            && let (Expression::Group { expression: inner }, _) = self.node(root)?
        {
            rc_root = Some(inner);
        }
        let rc_payload =
            rc_root.filter(|root| self.inputs.typed.rc_operation(self.unit(*root)).is_some());
        let view = match receiver {
            Some(receiver) if rc_payload.is_none() => self.narrowed_view(receiver)?,
            _ => None,
        };
        let (loan, created, rc_view_read) = match (view, rc_payload) {
            (Some(view), _) => (view, Vec::new(), Vec::new()),
            // `rc.value.f`: borrow the payload and any ancestors until after the read.
            (None, Some(receiver)) => self.rc_payload_loan(receiver, span)?,
            (None, _) => {
                let loan = self.owner_loan(owner, LoanKind::Shared, span)?;
                (
                    loan.loan,
                    loan.scoped.then_some(loan.loan).into_iter().collect(),
                    Vec::new(),
                )
            }
        };
        let result = self.value(
            ROp::HeapFieldRead {
                receiver: loan,
                field,
            },
            EntityType::Value(result_type),
            span,
        )?;
        for loan in created.into_iter().rev() {
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
        }
        // The receiver is read in place; its own drop point follows the read.
        for read in rc_view_read {
            self.drops(UnitDropPoint::AfterExpression(self.unit(read)))?;
        }
        if let Some(receiver) = receiver {
            self.drops(UnitDropPoint::AfterExpression(self.unit(receiver)))?;
        }
        Ok(Lowered::Value(result))
    }

    /// Borrow an Rc payload and keep its ancestor loans alive for the consumer.
    pub(super) fn rc_payload_loan(
        &mut self,
        receiver: ExpressionId,
        span: Span,
    ) -> Result<super::callable::ProjectionLoan, LoweringError> {
        let descriptor = self
            .inputs
            .typed
            .rc_operation(self.unit(receiver))
            .filter(|descriptor| descriptor.kind() == RcOperationKind::Value)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (owner, mut created, view_read) = self.rc_owner(descriptor.receiver().expression())?;
        let target = self.ssa_type(self.c(descriptor.payload_type()), span)?;
        let loan = self.shared_rc_payload_loan(owner, target, span)?;
        created.push(loan);
        Ok((loan, created, view_read))
    }

    pub(super) fn shared_rc_payload_loan(
        &mut self,
        owner: Slot,
        target: crate::ssa::model::SsaTypeId,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let op = if matches!(self.body.slots[owner.0], EntityType::Loan { .. }) {
            ROp::SharedPayloadLoan { source: owner }
        } else {
            let place = self.value(
                ROp::SharedPayloadPlace { owner },
                EntityType::Place(target),
                span,
            )?;
            ROp::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            }
        };
        self.value(
            op,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            },
            span,
        )
    }

    /// The owner a class field projection goes through, its (narrowed) class type and the
    /// explicit receiver expression, if any.
    pub(super) fn projection_owner(
        &self,
        receiver: UnitAggregateProjectionReceiver,
        span: Span,
    ) -> Result<(OwnerRef, UnitTypeId, Option<ExpressionId>), LoweringError> {
        match receiver {
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                if receiver.source_unit() != self.inputs.source_unit {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let receiver = receiver.expression();
                let owner_type = match self.inputs.typed.non_null_use(self.unit(receiver)) {
                    Some(fact) => self.c(fact.narrowed_type()),
                    None => self.expression_type(receiver)?,
                };
                Ok((OwnerRef::Expression(receiver), owner_type, Some(receiver)))
            }
            UnitAggregateProjectionReceiver::This(declaration) => {
                let receiver = self
                    .inputs
                    .receiver
                    .filter(|receiver| receiver.owner == declaration)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                Ok((OwnerRef::This(declaration), receiver.ty, None))
            }
        }
    }

    /// `operand!!`: one evaluation, a null edge that aborts and a non-null edge that takes
    /// the inner owner (or `MapResultUnwrap` for Copyable `$map.result.tX`), consuming a
    /// MoveOnly operand.
    pub(super) fn non_null_assertion(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .non_null_assertion(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let plan = self
            .inputs
            .owned
            .non_null_assertion(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if plan.descriptor() != &descriptor
            || descriptor.operand().source_unit() != self.inputs.source_unit
            || !matches!(
                plan.non_null_transfer(),
                NonNullAssertionTransferKind::Consume | NonNullAssertionTransferKind::Copy
            )
            || !matches!(
                self.inputs.typed.types().get(self.c(descriptor.nullable_type())),
                Some(UnitTypeKind::Nullable(inner)) if *inner == self.c(descriptor.inner_type())
            )
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let operand = descriptor.operand().expression();
        let owner = match self.expression_inner(operand)? {
            Lowered::Value(owner) => owner,
            Lowered::Diverged => return Ok(Lowered::Diverged),
            Lowered::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        let nullable_ssa = self.ssa_type(self.c(descriptor.nullable_type()), span)?;
        let inner = self.ssa_type(self.c(descriptor.inner_type()), span)?;
        if plan.non_null_transfer() == NonNullAssertionTransferKind::Copy
            && self.types.map_results.contains_key(&nullable_ssa)
        {
            let value = self.value(
                ROp::MapResultUnwrap { result: owner },
                EntityType::Value(inner),
                span,
            )?;
            self.drops(UnitDropPoint::AfterExpression(self.unit(operand)))?;
            return Ok(Lowered::Value(value));
        }
        self.register_temporary(operand, owner)?;
        let mut root = operand;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let (payload, operand_span) = self.node(root)?;
        if plan.non_null_transfer() == NonNullAssertionTransferKind::Consume
            && let Expression::Name = payload
        {
            // The owner is consumed: no proof view of it may stay live.
            let symbol = self.reference(operand_span)?;
            self.end_views_of(symbol, span)?;
        }
        let null_block = self.body.block();
        let non_null_block = self.body.block();
        let view = self.body.slot(EntityType::Loan {
            kind: LoanKind::Shared,
            target: inner,
        });
        self.body.blocks[non_null_block.0].view = Some(view);
        self.terminate(
            RTerm::NullBranch {
                owner,
                when_null: null_block,
                when_non_null: non_null_block,
            },
            span,
        )?;
        self.block = Some(null_block);
        self.terminate(RTerm::Abort(RAbort::NullAssertion), span)?;
        self.block = Some(non_null_block);
        let value = self.value(
            ROp::NullableTake { owner, proof: view },
            EntityType::Value(inner),
            span,
        )?;
        if plan.non_null_transfer() == NonNullAssertionTransferKind::Consume {
            self.deliver(operand);
        }
        self.drops(UnitDropPoint::AfterExpression(self.unit(operand)))?;
        Ok(Lowered::Value(value))
    }
}
