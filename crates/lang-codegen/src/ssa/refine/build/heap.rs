//! Class construction, field reads and `!!` in the refined CFG.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{NonNullAssertionTransferKind, UnitDropPoint},
    parser::Expression,
    source::Span,
    type_checking::{UnitAggregateProjectionReceiver, UnitConstructionTarget, UnitTypeKind},
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind},
    refine::body::{RAbort, ROp, RTerm, Slot},
};

impl Builder<'_, '_> {
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
            let value = self.adapt(value, value_expression, argument.parameter_type())?;
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
        let owner = self.ssa_type(descriptor.result_type(), span)?;
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

    /// A Copyable field read through a shared loan: the receiver's proof view, or a loan
    /// of the owned binding that ends right after the read.
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
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        if receiver.source_unit() != self.inputs.source_unit {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let receiver = receiver.expression();
        let owner_type = match self.inputs.typed.non_null_use(self.unit(receiver)) {
            Some(fact) => fact.narrowed_type(),
            None => self.expression_type(receiver)?,
        };
        let field = self
            .types
            .field_indices
            .get(&(owner_type, projection.field()))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let result_type = self.ssa_type(self.expression_type(expression)?, span)?;
        let (loan, scoped) = if let Some(view) = self.narrowed_view(receiver)? {
            (view, false)
        } else {
            let (payload, receiver_span) = self.node(receiver)?;
            let Expression::Name = payload else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            let symbol = self.reference(receiver_span)?;
            let owner = *self
                .bindings
                .get(&symbol)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, receiver_span))?;
            let target = self.ssa_type(owner_type, span)?;
            let place = self.value(ROp::RootPlace { owner }, EntityType::Place(target), span)?;
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
            (loan, true)
        };
        let result = self.value(
            ROp::HeapFieldRead {
                receiver: loan,
                field,
            },
            EntityType::Value(result_type),
            span,
        )?;
        if scoped {
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
        }
        // The receiver is read in place; its own drop point follows the read.
        self.drops(UnitDropPoint::AfterExpression(self.unit(receiver)))?;
        Ok(Lowered::Value(result))
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
                self.inputs.typed.types().get(descriptor.nullable_type()),
                Some(UnitTypeKind::Nullable(inner)) if *inner == descriptor.inner_type()
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
        let (payload, operand_span) = self.node(operand)?;
        if let Expression::Name = payload {
            // The owner is consumed: no proof view of it may stay live.
            let symbol = self.reference(operand_span)?;
            self.end_views_of(symbol, span)?;
        }
        let inner = self.ssa_type(descriptor.inner_type(), span)?;
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
