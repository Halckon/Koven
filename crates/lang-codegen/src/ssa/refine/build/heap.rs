//! Class construction, field reads and `!!` in the refined CFG.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{NonNullAssertionTransferKind, UnitDropPoint},
    parser::Expression,
    source::Span,
    type_checking::{
        UnitAggregateProjectionReceiver, UnitConstructionTarget, UnitTypeId, UnitTypeKind,
    },
};

use super::{
    Builder, Lowered,
    place::{OwnerLoan, OwnerRef},
};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind},
    refine::body::{RAbort, ROp, RTerm, Slot},
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
        for argument in arguments {
            if argument.argument().source_unit() != self.inputs.source_unit {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let value_expression = argument.argument().expression();
            let Some(value) = self.required(value_expression)? else {
                return Ok(Lowered::Diverged);
            };
            let value = self.adapt(value, value_expression, self.c(argument.parameter_type()))?;
            self.deliver(value_expression);
            let slot = fields
                .get_mut(argument.parameter_index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if slot.replace(value).is_some() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
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
        let (owner, owner_type, receiver) = self.projection_owner(projection.receiver(), span)?;
        let field = self
            .types
            .field_indices
            .get(&(owner_type, projection.field()))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let result_type = self.ssa_type(self.expression_type(expression)?, span)?;
        let rc_payload = receiver.is_some_and(|receiver| {
            self.inputs
                .typed
                .rc_operation(self.unit(receiver))
                .is_some()
        });
        let view = match receiver {
            Some(receiver) if !rc_payload => self.narrowed_view(receiver)?,
            _ => None,
        };
        let loan = match (view, receiver) {
            (Some(view), _) => OwnerLoan {
                loan: view,
                scoped: false,
            },
            // `rc.value.f`: a shared loan of the payload place (SPEC-0359).
            (None, Some(receiver)) if rc_payload => self.rc_payload_loan(receiver, span)?,
            (None, _) => self.owner_loan(owner, LoanKind::Shared, span)?,
        };
        let result = self.value(
            ROp::HeapFieldRead {
                receiver: loan.loan,
                field,
            },
            EntityType::Value(result_type),
            span,
        )?;
        self.end_owner_loan(loan, span)?;
        // The receiver is read in place; its own drop point follows the read.
        if let Some(receiver) = receiver {
            self.drops(UnitDropPoint::AfterExpression(self.unit(receiver)))?;
        }
        Ok(Lowered::Value(result))
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
    /// the inner owner, consuming the operand.
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
            || plan.non_null_transfer() != NonNullAssertionTransferKind::Consume
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
        self.register_temporary(operand, owner)?;
        let mut root = operand;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let (payload, operand_span) = self.node(root)?;
        if let Expression::Name = payload {
            // The owner is consumed: no proof view of it may stay live.
            let symbol = self.reference(operand_span)?;
            self.end_views_of(symbol, span)?;
        }
        let inner = self.ssa_type(self.c(descriptor.inner_type()), span)?;
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
        self.deliver(operand);
        self.drops(UnitDropPoint::AfterExpression(self.unit(operand)))?;
        Ok(Lowered::Value(value))
    }
}
