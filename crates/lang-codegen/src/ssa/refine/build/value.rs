//! Copyable `value class` in the refined CFG (SPEC-0361): construction builds the inline
//! aggregate and a field read projects a copy out of the owner's value.
use lang_frontend::{
    ast::ExpressionId,
    parser::Expression,
    source::Span,
    type_checking::{NominalKind, UnitAggregateProjectionReceiver, UnitTypeId, UnitTypeKind},
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind},
    refine::body::{ROp, Slot},
};

impl Builder<'_, '_> {
    /// Whether `ty` is a value class (the subset admits only Copyable scalar ones).
    pub(super) fn is_value_class(&self, ty: UnitTypeId) -> bool {
        let Some(UnitTypeKind::Nominal { declaration, .. }) = self.inputs.typed.types().get(ty)
        else {
            return false;
        };
        self.inputs
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .is_some_and(|nominal| nominal.kind() == NominalKind::ValueClass)
    }

    /// Fields evaluate in source order and land in declaration order in the aggregate.
    pub(super) fn value_construction(
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
        let mut arguments = descriptor.arguments().iter().collect::<Vec<_>>();
        arguments.sort_by_key(|argument| argument.evaluation_index());
        let mut fields: Vec<Option<Slot>> = vec![None; arguments.len()];
        for argument in arguments {
            if argument.argument().source_unit() != self.inputs.source_unit {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let Some(value) = self.required(argument.argument().expression())? else {
                return Ok(Lowered::Diverged);
            };
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
        let aggregate = self.ssa_type(self.c(descriptor.result_type()), span)?;
        Ok(Lowered::Value(self.value(
            ROp::AggregateConstruct { aggregate, fields },
            EntityType::Value(aggregate),
            span,
        )?))
    }

    /// Whether the projection reads a field of a value class.
    pub(super) fn is_value_projection(&self, expression: ExpressionId) -> bool {
        let Some(projection) = self
            .inputs
            .typed
            .aggregate_projection(self.unit(expression))
        else {
            return false;
        };
        let owner = match projection.receiver() {
            UnitAggregateProjectionReceiver::This(_) => {
                self.inputs.receiver.map(|receiver| receiver.ty)
            }
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                self.expression_type_of(receiver)
            }
        };
        owner.is_some_and(|owner| self.is_value_class(owner))
    }

    /// Copies a field out of the owner's value: a name (a Borrow loan is read by copy) or
    /// the current receiver.
    pub(super) fn value_projection(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let projection = self
            .inputs
            .typed
            .aggregate_projection(self.unit(expression))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        // A loan owner (Borrow receiver or parameter) is never copied whole: one shared
        // field loan is read and ended. An owned value projects its field directly.
        let (owner_type, owner) = match projection.receiver() {
            UnitAggregateProjectionReceiver::This(declaration) => {
                let receiver = self
                    .inputs
                    .receiver
                    .filter(|receiver| receiver.owner == declaration)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let slot = self
                    .receiver
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                (receiver.ty, slot)
            }
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                if receiver.source_unit() != self.inputs.source_unit {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let mut receiver = receiver.expression();
                while let (Expression::Group { expression: inner }, _) = self.node(receiver)? {
                    receiver = inner;
                }
                let owner_type = self.expression_type(receiver)?;
                let (payload, receiver_span) = self.node(receiver)?;
                let owner = match payload {
                    Expression::This => self
                        .receiver
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                    Expression::Name => {
                        let symbol = self.reference(receiver_span)?;
                        match self.borrowed.get(&symbol).copied() {
                            Some(loan) => loan,
                            None => self
                                .required(receiver)?
                                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                        }
                    }
                    _ => self
                        .required(receiver)?
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                };
                (owner_type, owner)
            }
        };
        let field = self
            .types
            .field_indices
            .get(&(owner_type, projection.field()))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let result = self.ssa_type(self.expression_type(expression)?, span)?;
        if let EntityType::Loan { kind, .. } = self.body.slots[owner.0] {
            if kind != LoanKind::Shared {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let loan = self.value(
                ROp::SharedFieldLoan { base: owner, field },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: result,
                },
                span,
            )?;
            let value = self.value(ROp::ReadLoan { loan }, EntityType::Value(result), span)?;
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
            return Ok(Lowered::Value(value));
        }
        Ok(Lowered::Value(self.value(
            ROp::AggregateProject {
                aggregate: owner,
                field,
            },
            EntityType::Value(result),
            span,
        )?))
    }
}
