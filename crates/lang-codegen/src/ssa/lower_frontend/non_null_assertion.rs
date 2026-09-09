//! 已验证的 owned nullable assertion：一次求值、成功转移和封闭 Abort 边。
use super::{ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, error, value};
use crate::ssa::model::{Edge, EntityId, EntityType, LoanKind, Operation, Origin, TerminatorKind};
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{DropPoint, NonNullAssertionTransferKind},
    parser::Expression,
    source::Span,
    type_checking::{AssertionFailureEffect, NullableWhenSubjectCategory},
};

impl ExpressionLowerer<'_> {
    pub(super) fn lower_non_null_assertion(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .non_null_assertion(expression)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let plan = self
            .owned
            .non_null_assertion(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if plan.descriptor() != &descriptor || descriptor.expression() != expression {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if plan.non_null_transfer() != NonNullAssertionTransferKind::Consume
            || !matches!(
                descriptor.source_category(),
                NullableWhenSubjectCategory::OwnedRoot | NullableWhenSubjectCategory::Temporary
            )
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let AssertionFailureEffect::Abort = plan.null_effect();
        let inner = self.expression_ssa_type(expression, span)?;
        // Defer operand cleanup until take has transferred its owner, including grouped aliases.
        let mut operands = vec![descriptor.operand()];
        let mut evaluated = descriptor.operand();
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(evaluated)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            let Expression::Group { expression } = node.payload() else {
                break;
            };
            evaluated = *expression;
            operands.push(evaluated);
        }
        let owner = match self.lower_expression(evaluated)? {
            LoweredValue::Value(owner) => owner,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        if descriptor.source_category() == NullableWhenSubjectCategory::Temporary {
            self.temporaries.insert(descriptor.operand().index(), owner);
        }
        let baseline = self.bindings.clone();
        let carried = self.linear_binding_slots(&baseline, span)?;
        let owner_slot = carried
            .slots
            .iter()
            .position(|slot| slot.source == EntityId::Value(owner))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let types = carried.slots.iter().map(|slot| slot.ty).collect::<Vec<_>>();
        let null = self
            .function
            .add_block(types.clone(), Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let mut proven_types = types;
        proven_types.push(EntityType::Loan {
            kind: LoanKind::Shared,
            target: inner,
        });
        let non_null = self
            .function
            .add_block(proven_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Loan(proof) = self
            .function
            .block(non_null)
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
            .parameters[carried.slots.len()]
        else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let edge = |target| Edge {
            target,
            arguments: carried.slots.iter().map(|slot| slot.source).collect(),
        };
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::NullableBranch {
                    owner,
                    when_null: edge(null),
                    when_non_null: edge(non_null),
                    view: proof,
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        // Abort never unwinds live owners or ends pending call loans.
        self.function
            .set_terminator(
                null,
                TerminatorKind::Abort,
                Origin::Source(descriptor.operator_span()),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.block = non_null;
        self.bindings = self.rebind_linear_bindings(&baseline, non_null, &carried, span)?;
        let owner = value(
            self.function
                .block(non_null)
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                .parameters[owner_slot],
        );
        let (_, result) = self.append(
            Operation::NullableTake { owner, proof },
            vec![EntityType::Value(inner)],
            span,
        )?;
        let consumed_symbols = self
            .bindings
            .iter()
            .filter_map(|(&symbol, binding)| {
                matches!(binding, LoweredValue::Value(value) if *value == owner).then_some(symbol)
            })
            .collect::<Vec<_>>();
        for symbol in consumed_symbols {
            self.bindings.remove(&symbol);
            self.non_null_bindings.remove(&symbol);
        }
        self.temporaries.retain(|_, value| *value != owner);
        for operand in operands {
            self.emit_drops(DropPoint::AfterExpression(operand))?;
        }
        Ok(LoweredValue::Value(value(result[0])))
    }
}
