//! 前端 null comparison 描述符驱动的非消费 presence 读取，以及已验证的 owned nullable assertion。
use super::*;
use crate::ssa::model::{Edge, EntityId, EntityType, LoanKind, Operation, Origin, TerminatorKind};
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{DropPoint, DropTarget, NonNullAssertionTransferKind},
    parser::{BinaryOperator as Operator, Expression},
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
        let nullable = self.resolve_type(descriptor.nullable_type(), span)?;
        if let Some(&result) = self.type_ids.get(&nullable)
            && self.map_results.get(&result).copied().is_some()
            && plan.non_null_transfer() == NonNullAssertionTransferKind::Copy
        {
            if plan.non_null_transfer() != NonNullAssertionTransferKind::Copy
                || descriptor.copyability() != lang_frontend::type_checking::Copyability::Copyable
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let operand = self.require_value(descriptor.operand())?;
            return self.unwrap_map_result(operand, descriptor.operator_span());
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
        self.validate_non_null_proof_types(
            super::NonNullProof::introduced(
                proof,
                owner,
                inner,
                super::non_null_proof::ProofScope::Assertion(expression),
            ),
            span,
        )?;
        let consumed_symbols = self
            .bindings
            .iter()
            .filter_map(|(&symbol, binding)| {
                matches!(binding, LoweredValue::Value(value) if *value == owner).then_some(symbol)
            })
            .collect::<Vec<_>>();
        for &symbol in &consumed_symbols {
            self.reject_live_while_proof(symbol, span)?;
            if let Some(&existing) = self.non_null_bindings.get(&symbol) {
                self.validate_non_null_proof(symbol, existing, &self.bindings, span)?;
            }
        }
        let (_, result) = self.append(
            Operation::NullableTake { owner, proof },
            vec![EntityType::Value(inner)],
            span,
        )?;
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

    pub(super) fn lower_nullable_comparison(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let Some(d) = self.typed.null_comparison(expression) else {
            return self.lower_temporary_nullable_comparison(expression, span);
        };
        let ty = self.resolve_type(d.nullable_type(), span)?;
        let target = self
            .type_ids
            .get(&ty)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let aggregate_presence = self.map_results.contains_key(&target);
        let operation = if let Some(&source) = self.borrow_bindings.get(&d.symbol()) {
            if self.nullable_maps.contains_key(&target) {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            if aggregate_presence {
                return Ok(None);
            }
            Operation::NullableLoanIsNull { source }
        } else if let Some(LoweredValue::Value(owner)) = self.bindings.get(&d.symbol()).copied() {
            if aggregate_presence {
                Operation::AggregateProject {
                    aggregate: owner,
                    field: 0,
                }
            } else {
                Operation::NullableIsNull { owner }
            }
        } else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let boolean = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(operation, vec![EntityType::Value(boolean)], span)?;
        let mut result = value(results[0]);
        if d.non_null_when_true() != aggregate_presence {
            let (_, results) = self.append(
                Operation::BooleanNot { operand: result },
                vec![EntityType::Value(boolean)],
                span,
            )?;
            result = value(results[0]);
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Binary { left, right, .. } = *node.payload() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        self.emit_null_condition_drops(left)?;
        self.emit_null_condition_drops(right)?;
        self.emit_drops(DropPoint::AfterBinaryOperands(expression))?;
        Ok(Some(LoweredValue::Value(result)))
    }

    pub(super) fn emit_null_condition_drops(
        &mut self,
        expression: ExpressionId,
    ) -> Result<(), LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        match node.payload().clone() {
            Expression::Binary { left, right, .. } => {
                self.emit_null_condition_drops(left)?;
                self.emit_null_condition_drops(right)?;
            }
            Expression::Group { expression } => self.emit_null_condition_drops(expression)?,
            _ => {}
        }
        self.emit_drops(DropPoint::AfterExpression(expression))
    }

    fn lower_temporary_nullable_comparison(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let expressions = self.parsed.ast().expressions();
        let node = expressions
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Binary {
            left,
            operator,
            right,
            ..
        } = *node.payload()
        else {
            return Ok(None);
        };
        if !matches!(operator, Operator::Equal | Operator::NotEqual) {
            return Ok(None);
        }
        let operand = if self.is_null_literal(right) {
            left
        } else if self.is_null_literal(left) {
            right
        } else {
            return Ok(None);
        };
        let mut leaf = operand;
        let origin = loop {
            let node = expressions
                .get(leaf)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            if let Expression::Group { expression } = *node.payload() {
                leaf = expression;
            } else {
                break node.span();
            }
        };
        let point = DropPoint::AfterBinaryOperands(expression);
        let target = DropTarget::Temporary(leaf);
        let mut facts = self
            .owned
            .drops()
            .iter()
            .filter(|fact| fact.point() == point && fact.target() == target);
        let Some(fact) = facts.next() else {
            return Ok(None);
        };
        if facts.next().is_some() || fact.value_origin() != origin {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let target = self.expression_ssa_type(leaf, origin)?;
        let aggregate_presence = self.map_results.contains_key(&target);
        if !aggregate_presence
            && !self.nullable_maps.contains_key(&target)
            && !matches!(
                self.ssa_types.get(target.index()),
                Some(crate::ssa::model::SsaTypeKind::NullableHandle { .. })
            )
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner = self.require_value(operand)?;
        let operation = if aggregate_presence {
            Operation::AggregateProject {
                aggregate: owner,
                field: 0,
            }
        } else {
            Operation::NullableIsNull { owner }
        };
        let boolean = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(operation, vec![EntityType::Value(boolean)], span)?;
        let mut result = value(results[0]);
        if (operator == Operator::NotEqual) != aggregate_presence {
            let (_, results) = self.append(
                Operation::BooleanNot { operand: result },
                vec![EntityType::Value(boolean)],
                span,
            )?;
            result = value(results[0]);
        }
        self.emit_drops(point)?;
        Ok(Some(LoweredValue::Value(result)))
    }
}
