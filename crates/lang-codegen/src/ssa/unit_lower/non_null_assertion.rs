//! Source-qualified assertion facts 驱动的一次求值、成功移交与直接 Abort。

use std::collections::BTreeMap;

use super::{LoweredValue, UnitExpressionLowerer, lowering_error, require_value};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{Edge, EntityId, EntityType, LoanKind, Operation, Origin, TerminatorKind},
};
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{NonNullAssertionTransferKind, UnitDropPoint},
    parser::Expression,
    source::Span,
    type_checking::{AssertionFailureEffect, NullableWhenSubjectCategory, UnitExpressionId},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_non_null_assertion(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .types()
            .non_null_assertion(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let plan = self
            .owned
            .ownership()
            .non_null_assertion(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if plan.descriptor() != &descriptor
            || descriptor.operand().source_unit() != self.source_unit
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if plan.non_null_transfer() != NonNullAssertionTransferKind::Consume
            || !matches!(
                descriptor.source_category(),
                NullableWhenSubjectCategory::OwnedRoot | NullableWhenSubjectCategory::Temporary
            )
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let AssertionFailureEffect::Abort = plan.null_effect();
        let inner = self.expression_ssa_type(expression, span)?;
        let mut evaluated = descriptor.operand().expression();
        let mut operands = vec![evaluated];
        while let Expression::Group { expression } = self
            .parsed
            .ast()
            .expressions()
            .get(evaluated)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
            .payload()
        {
            evaluated = *expression;
            operands.push(evaluated);
        }
        // Group cleanup follows the take so no alias can drop the operand prematurely.
        let owner = match self.lower_expression(evaluated)? {
            LoweredValue::Value(owner) => owner,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        let mut carried = std::collections::BTreeSet::from([EntityId::Value(owner)]);
        carried.extend(self.bindings.values().filter_map(|binding| match binding {
            LoweredValue::Value(value) => Some(EntityId::Value(*value)),
            _ => None,
        }));
        carried.extend(self.temporaries.values().copied().map(EntityId::Value));
        carried.extend(self.borrow_bindings.values().copied().map(EntityId::Loan));
        carried.extend(self.current_receiver.map(|receiver| receiver.entity));
        carried.extend(self.pending_operands.iter().copied());
        let carried = carried.into_iter().collect::<Vec<_>>();
        let types = carried
            .iter()
            .map(|entity| {
                self.function
                    .entity(*entity)
                    .map(|data| data.ty)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let null = self
            .function
            .add_block(types.clone(), Origin::Source(span))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let mut types = types;
        types.push(EntityType::Loan {
            kind: LoanKind::Shared,
            target: inner,
        });
        let non_null = self
            .function
            .add_block(types, Origin::Source(span))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let parameters = self
            .function
            .block(non_null)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .parameters
            .clone();
        let EntityId::Loan(proof) = parameters[carried.len()] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::NullableBranch {
                    owner,
                    when_null: Edge {
                        target: null,
                        arguments: carried.clone(),
                    },
                    when_non_null: Edge {
                        target: non_null,
                        arguments: carried.clone(),
                    },
                    view: proof,
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.function
            .set_terminator(
                null,
                TerminatorKind::Abort,
                Origin::Source(descriptor.operator_span()),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let rebound = carried
            .into_iter()
            .zip(parameters)
            .collect::<BTreeMap<_, _>>();
        let owner = require_value(rebound[&EntityId::Value(owner)], span)?;
        for binding in self.bindings.values_mut() {
            if let LoweredValue::Value(value) = binding {
                *value = require_value(rebound[&EntityId::Value(*value)], span)?;
            }
        }
        for value in self.temporaries.values_mut() {
            *value = require_value(rebound[&EntityId::Value(*value)], span)?;
        }
        for loan in self.borrow_bindings.values_mut() {
            let EntityId::Loan(replacement) = rebound[&EntityId::Loan(*loan)] else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            *loan = replacement;
        }
        if let Some(receiver) = &mut self.current_receiver {
            receiver.entity = rebound[&receiver.entity];
        }
        for entity in &mut self.pending_operands {
            *entity = rebound[entity];
        }
        self.block = non_null;
        let (_, results) = self
            .function
            .append_instruction(
                non_null,
                Operation::NullableTake { owner, proof },
                vec![EntityType::Value(inner)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.bindings
            .retain(|_, binding| *binding != LoweredValue::Value(owner));
        self.temporaries.retain(|_, value| *value != owner);
        for operand in operands {
            self.emit_drops(UnitDropPoint::AfterExpression(UnitExpressionId::new(
                self.source_unit,
                operand,
            )))?;
        }
        Ok(LoweredValue::Value(require_value(results[0], span)?))
    }
}
