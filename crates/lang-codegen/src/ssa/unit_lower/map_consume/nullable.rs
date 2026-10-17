//! Consume-first ordinary If extraction from the original nullable branch view only.
#[cfg(test)]
mod guard_tests;

mod cleanup;
mod source;

use super::*;
use crate::ssa::{
    lowering_support::callable_instances::SourceToken,
    model::{Definition, Edge},
};
use lang_frontend::{
    parser::{BinaryOperator, VariableKind},
    type_checking::UnitCallableTarget,
};
use source::NullableMapConsumeSource;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ssa::unit_lower) struct NullableMapConsumeWitness {
    source: SourceToken,
    function: FunctionId,
    control: UnitExpressionId,
    call: UnitExpressionId,
    pub(in crate::ssa::unit_lower) root: UnitSymbolId,
    storage: UnitTypeId,
    pub(in crate::ssa::unit_lower) nullable: SsaTypeId,
    pub(in crate::ssa::unit_lower) inner: SsaTypeId,
    non_null_when_true: bool,
}

impl UnitExpressionLowerer<'_> {
    /// Recomputed both at the producer and at consumption; no transported CFG witness.
    pub(in crate::ssa::unit_lower) fn nullable_map_consume_witness(
        &self,
        control: UnitExpressionId,
        span: Span,
    ) -> Result<Option<NullableMapConsumeWitness>, LoweringError> {
        if !self.is_direct_function_body_map_if(control, span)?
            || self.expression_builtin_type(control.expression(), span)? != Some(BuiltinType::Unit)
        {
            return Ok(None);
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(control.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::If {
            condition,
            then_branch,
            else_branch,
            ..
        } = *node.payload()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let Some(condition_fact) = self.map_borrow_condition(condition, span)? else {
            return Ok(None);
        };
        let comparison = self.ungroup_map_if_expression(condition, span)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(comparison)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Binary {
            left,
            operator,
            right,
            ..
        } = *node.payload()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let non_null_when_true = match operator {
            BinaryOperator::NotEqual => true,
            BinaryOperator::Equal => false,
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        let is_null =
            |id| -> Result<bool, LoweringError> {
                let id = self.ungroup_map_if_expression(id, span)?;
                Ok(self.parsed.ast().expressions().get(id).is_ok_and(|node| {
                    matches!(node.payload(), Expression::Literal(LiteralKind::Null))
                }))
            };
        let subject = match (is_null(left)?, is_null(right)?) {
            (true, false) => right,
            (false, true) => left,
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        if condition_fact.expression() != UnitExpressionId::new(self.source_unit, comparison)
            || condition_fact.non_null_when_true() != non_null_when_true
            || self.direct_place_symbol(subject, span)? != condition_fact.symbol()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let mut operand = subject;
        loop {
            let id = UnitExpressionId::new(self.source_unit, operand);
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(operand)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if self.typed.expression_type(id) != Some(condition_fact.nullable_type())
                || self.typed.expression_category(id) != Some(ExpressionCategory::Place)
                || self.typed.non_null_use(id).is_some()
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            match node.payload() {
                Expression::Group { expression } => operand = *expression,
                Expression::Name
                    if self.references.get(&span_key(node.span()))
                        == Some(&condition_fact.symbol()) =>
                {
                    break;
                }
                _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            }
        }
        let (body, null_body) = if non_null_when_true {
            (Some(then_branch), else_branch)
        } else {
            (else_branch, Some(then_branch))
        };
        let Some(body) = body else { return Ok(None) };
        if !self.map_if_body_is_empty(null_body, span)? {
            return Ok(None);
        }
        let node = self
            .parsed
            .ast()
            .statements()
            .get(body)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Statement::ControlBody { elements } = node.payload() else {
            return Ok(None);
        };
        let Some(first) = elements.first() else {
            return Ok(None);
        };
        let node = self
            .parsed
            .ast()
            .statements()
            .get(*first)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let expression = match node.payload() {
            Statement::Expression { expression } => *expression,
            Statement::LocalVariable { declaration } => {
                let (_, item, _) = unwrap_modified(self.parsed, *declaration)?;
                let Item::Variable {
                    initializer,
                    kind: VariableKind::Val | VariableKind::Var,
                    ..
                } = item
                else {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                };
                initializer
            }
            _ => return Ok(None),
        };
        let call = UnitExpressionId::new(
            self.source_unit,
            self.ungroup_map_if_expression(expression, span)?,
        );
        let Some(source) = self.qualify_nullable_map_consume(call, span)? else {
            return Ok(None);
        };
        if source.root != condition_fact.symbol()
            || source.storage != condition_fact.nullable_type()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(Some(NullableMapConsumeWitness {
            source: self.source_token,
            function: self.function.id,
            control,
            call,
            root: source.root,
            storage: source.storage,
            nullable: source.nullable,
            inner: source.inner,
            non_null_when_true,
        }))
    }

    /// Called ahead of the general live-If Call rejection, with all preflight before Take.
    pub(in crate::ssa::unit_lower) fn lower_nullable_map_consume(
        &mut self,
        call: UnitExpressionId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let Some(source) = self.qualify_nullable_map_consume(call, span)? else {
            return Ok(None);
        };
        let proof = self
            .nullable_proofs
            .get(&source.root)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let control = proof
            .if_scope()
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let witness = self
            .nullable_map_consume_witness(control, span)?
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        if witness.source != self.source_token
            || witness.function != self.function.id
            || witness.call != call
            || witness.control != control
            || witness.root != source.root
            || witness.storage != source.storage
            || witness.nullable != source.nullable
            || witness.inner != source.inner
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        self.validate_nullable_map_consume_view(&source, proof, witness, span)?;
        self.preflight_nullable_map_consume_cleanup(&source, proof.owner, span)?;
        let receiver_span = self
            .parsed
            .ast()
            .expressions()
            .get(source.descriptor.receiver().expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
            .span();
        let (_, values) = self
            .function
            .append_instruction(
                self.block,
                Operation::NullableTake {
                    owner: proof.owner,
                    proof: proof.loan,
                },
                vec![EntityType::Value(source.inner)],
                Origin::Source(receiver_span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let [EntityId::Value(inner)] = values.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        let inner = *inner;
        if self.nullable_proofs.remove(&source.root) != Some(proof) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        // The same original identity checks apply, now after atomic proof consumption.
        self.take_owned_binding(source.root, proof.owner, span)?;
        self.map_borrow_guards.remove(&source.root);
        // Preflight admitted no other aliases or temporary owner registrations.
        for operand in source.operands {
            self.emit_drops(UnitDropPoint::AfterExpression(operand))?;
        }
        let (_, values) = self
            .function
            .append_instruction(
                self.block,
                Operation::MapConsume {
                    owner: inner,
                    map_type: source.result,
                },
                vec![EntityType::Value(source.result)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let [EntityId::Value(result)] = values.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        let result = *result;
        self.emit_drops(UnitDropPoint::CallReturn(call))?;
        Ok(Some(LoweredValue::Value(result)))
    }

    fn validate_nullable_map_consume_view(
        &self,
        source: &NullableMapConsumeSource,
        proof: super::super::nullable_proof::NullableProof,
        witness: NullableMapConsumeWitness,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.ensure_no_while_proof_carriers(span)?;
        if self.nullable_proofs.len() != 1
            || !self.pending_call_frames.is_empty()
            || !self.pending_operands.is_empty()
            || !self.temporaries.is_empty()
            || !self.borrow_bindings.is_empty()
            || !self.closure_bindings.is_empty()
            || self.closure_binding_context
            || self.thunk_expression.is_some()
            || self.consumed_receiver.is_some()
            || !self.loops.is_empty()
            || self.bindings.iter().any(|(&root, &value)| {
                root != source.root && value == LoweredValue::Value(proof.owner)
            })
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(witness.control.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::If { condition, .. } = *node.payload() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let guard = self
            .map_borrow_condition(condition, span)?
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self.map_borrow_guards.len() != 1
            || self.map_borrow_guards.get(&source.root) != Some(&guard)
            || guard.non_null_when_true() != witness.non_null_when_true
            || proof.inner != source.inner
            || proof.loan != proof.birth_loan()
            || self.bindings.get(&source.root) != Some(&LoweredValue::Value(proof.owner))
            || self
                .function
                .entity(EntityId::Value(proof.owner))
                .map(|entity| entity.ty)
                != Some(EntityType::Value(source.nullable))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let current = self
            .function
            .block(self.block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let columns = current
            .parameters
            .iter()
            .enumerate()
            .filter_map(|(index, value)| (*value == EntityId::Value(proof.owner)).then_some(index))
            .collect::<Vec<_>>();
        let [column] = columns.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        for instruction in &current.instructions {
            let instruction = self
                .function
                .instruction(*instruction)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if instruction.operation.entities().iter().any(|entity| {
                *entity == EntityId::Value(proof.owner) || *entity == EntityId::Loan(proof.loan)
            }) {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
        }
        if self
            .function
            .entity(EntityId::Value(proof.owner))
            .map(|entity| entity.definition)
            != Some(Definition::BlockParameter {
                block: self.block,
                index: *column,
            })
            || self
                .function
                .entity(EntityId::Loan(proof.loan))
                .map(|entity| entity.definition)
                != Some(Definition::BlockParameter {
                    block: self.block,
                    index: current.parameters.len() - 1,
                })
            || self
                .function
                .entity(EntityId::Loan(proof.loan))
                .map(|entity| entity.ty)
                != Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: source.inner,
                })
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let branches = self
            .function
            .blocks
            .iter()
            .filter_map(|block| {
                let terminator = block.terminator.as_ref()?;
                match &terminator.kind {
                    TerminatorKind::NullableBranch { view, .. } if *view == proof.birth_loan() => {
                        Some(terminator)
                    }
                    _ => None,
                }
            })
            .collect::<Vec<_>>();
        let [branch] = branches.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let TerminatorKind::NullableBranch {
            owner,
            when_null,
            when_non_null,
            view,
        } = &branch.kind
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let null = self
            .function
            .block(when_null.target)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let null_owner = null.parameters.get(*column).copied();
        if branch.origin.span() != node.span()
            || when_non_null.target != self.block
            || when_null.target == self.block
            || current.terminator.is_some()
            || *view != proof.loan
            || current.parameters.last() != Some(&EntityId::Loan(proof.loan))
            || current
                .parameters
                .iter()
                .filter(|entity| **entity == EntityId::Loan(proof.loan))
                .count()
                != 1
            || when_null
                .arguments
                .iter()
                .filter(|entity| **entity == EntityId::Value(*owner))
                .count()
                != 1
            || when_non_null
                .arguments
                .iter()
                .filter(|entity| **entity == EntityId::Value(*owner))
                .count()
                != 1
            || when_null.arguments != when_non_null.arguments
            || current.parameters.len() != when_non_null.arguments.len() + 1
            || null.parameters.len() != when_null.arguments.len()
            || when_null.arguments.len() != when_non_null.arguments.len()
            || when_null.arguments.get(*column) != Some(&EntityId::Value(*owner))
            || when_non_null.arguments.get(*column) != Some(&EntityId::Value(*owner))
            || self
                .function
                .entity(EntityId::Value(*owner))
                .map(|entity| entity.ty)
                != Some(EntityType::Value(source.nullable))
            || null_owner
                .and_then(|id| self.function.entity(id))
                .map(|entity| entity.ty)
                != Some(EntityType::Value(source.nullable))
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_nullable_map_consume_columns(when_null, span)?;
        self.validate_nullable_map_consume_columns(when_non_null, span)?;
        Ok(())
    }

    fn validate_nullable_map_consume_columns(
        &self,
        edge: &Edge,
        span: Span,
    ) -> Result<(), LoweringError> {
        let target = self
            .function
            .block(edge.target)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        for (index, &argument) in edge.arguments.iter().enumerate() {
            let parameter = target
                .parameters
                .get(index)
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if !matches!(argument, EntityId::Value(_)) || !matches!(parameter, EntityId::Value(_)) {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            let argument = self
                .function
                .entity(argument)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let parameter = self
                .function
                .entity(parameter)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if !matches!(argument.ty, EntityType::Value(_))
                || parameter.ty != argument.ty
                || parameter.definition
                    != (Definition::BlockParameter {
                        block: edge.target,
                        index,
                    })
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        Ok(())
    }
}
