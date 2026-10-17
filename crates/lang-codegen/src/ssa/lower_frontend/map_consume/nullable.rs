//! Direct If-born nullable Map ownership extraction, before ordinary consume.
#[cfg(test)]
mod guard_tests;

mod cleanup;
mod source;

use super::super::*;
use crate::ssa::lowering_support::callable_instances::SourceToken;
use crate::ssa::model::Definition;
use lang_frontend::type_checking::MapConsumeDescriptor;

struct NullableConsumeSource {
    source: SourceToken,
    control: ExpressionId,
    call: ExpressionId,
    root: SymbolId,
    storage: TypeId,
    nullable: SsaTypeId,
    inner: SsaTypeId,
    non_null_when_true: bool,
    // Original receiver boundaries, from the leaf Name out to the outer Group.
    operands: Vec<ExpressionId>,
}

impl ExpressionLowerer<'_> {
    pub(in super::super) fn lower_nullable_map_consume(
        &mut self,
        descriptor: MapConsumeDescriptor,
        inner: SsaTypeId,
        map_type: SsaTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let source = self.nullable_consume_source(descriptor, inner, span)?;
        let proof = self
            .non_null_bindings
            .get(&source.root)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        self.validate_nullable_consume_live(&source, proof, span)?;
        self.preflight_nullable_consume_cleanup(&source, proof.owner, span)?;
        // NullableTake itself consumes the actual branch loan and the wrapper.
        // Ending the loan first or replacing it with a transported alias is invalid.
        let (_, results) = self.append(
            Operation::NullableTake {
                owner: proof.owner,
                proof: proof.loan,
            },
            vec![EntityType::Value(source.inner)],
            self.expression_span(descriptor.receiver())?,
        )?;
        let [EntityId::Value(owner)] = results.as_slice() else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let owner = *owner;
        if self.non_null_bindings.remove(&source.root) != Some(proof)
            || self.bindings.remove(&source.root) != Some(LoweredValue::Value(proof.owner))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        // Qualification established exactly these state keys, with no wrapper aliases.
        for &operand in &source.operands {
            self.emit_drops(DropPoint::AfterExpression(operand))?;
        }
        let (_, results) = self.append(
            Operation::MapConsume { owner, map_type },
            vec![EntityType::Value(map_type)],
            span,
        )?;
        let [EntityId::Value(result)] = results.as_slice() else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let result = *result;
        self.emit_drops(DropPoint::CallReturn(source.call))?;
        Ok(LoweredValue::Value(result))
    }

    fn validate_nullable_consume_live(
        &self,
        source: &NullableConsumeSource,
        proof: NonNullProof,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_non_null_proof(source.root, proof, &self.bindings, span)?;
        let key = self
            .instance_plan
            .source(source.source)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if self.source_token != Some(source.source)
            || self.function_ids.get(key) != Some(&self.function.id)
            || proof.if_origin() != Some((source.control, proof.loan))
            || proof.inner != source.inner
            || self.typed.symbol_type(source.root) != Some(source.storage)
            || self.nullable_maps.get(&source.nullable) != Some(&source.inner)
            || self
                .function
                .entity(EntityId::Value(proof.owner))
                .map(|data| data.ty)
                != Some(EntityType::Value(source.nullable))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_nullable_consume_control(source, span)?;
        self.validate_nullable_consume_branch(source, proof, span)?;
        self.ensure_no_unresolved_while_proof_carriers(span)?;
        if self.non_null_bindings.len() != 1
            || !self.borrow_bindings.is_empty()
            || !self.capture_loans.is_empty()
            || !self.result_source_loans.is_empty()
            || !self.temporaries.is_empty()
            || !self.loops.is_empty()
            || self.map_loop_owners.has_loop_carriers()
            || self.thunk_expression.is_some()
            || self.readonly_receiver.is_some()
            || self.function.borrow_return.is_some()
            || self.function.carrier_return.is_some()
            || self.bindings.iter().any(|(&root, binding)| {
                root != source.root && *binding == LoweredValue::Value(proof.owner)
            })
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    fn validate_nullable_consume_branch(
        &self,
        source: &NullableConsumeSource,
        proof: NonNullProof,
        span: Span,
    ) -> Result<(), LoweringError> {
        let block = self
            .function
            .block(self.block)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(Definition::BlockParameter {
            block: owner_block,
            index: column,
        }) = self
            .function
            .entity(EntityId::Value(proof.owner))
            .map(|data| data.definition)
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let mut births = self.function.blocks.iter().filter_map(|header| {
            let terminator = header.terminator.as_ref()?;
            match &terminator.kind {
                TerminatorKind::NullableBranch { view, .. } if *view == proof.loan => {
                    Some(terminator)
                }
                _ => None,
            }
        });
        let birth = births
            .next()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        if births.next().is_some() {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let TerminatorKind::NullableBranch {
            owner,
            when_null,
            when_non_null,
            view,
        } = &birth.kind
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let null_block = self
            .function
            .block(when_null.target)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let control_span = self.expression_span(source.control)?;
        if birth.origin != Origin::Source(control_span)
            || owner_block != self.block
            || when_non_null.target != self.block
            || when_null.target == self.block
            || block.terminator.is_some()
            || block.parameters.get(column) != Some(&EntityId::Value(proof.owner))
            || block.parameters.last() != Some(&EntityId::Loan(*view))
            || block.parameters.len() != when_non_null.arguments.len() + 1
            || null_block.parameters.len() != when_null.arguments.len()
            || when_null.arguments != when_non_null.arguments
            || when_null.arguments.get(column) != Some(&EntityId::Value(*owner))
            || when_non_null.arguments.get(column) != Some(&EntityId::Value(*owner))
            || self
                .function
                .entity(EntityId::Value(*owner))
                .map(|data| data.ty)
                != Some(EntityType::Value(source.nullable))
            || null_block
                .parameters
                .get(column)
                .and_then(|&parameter| self.function.entity(parameter))
                .map(|data| data.ty)
                != Some(EntityType::Value(source.nullable))
            || self
                .function
                .entity(EntityId::Loan(*view))
                .map(|data| (data.ty, data.definition))
                != Some((
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: source.inner,
                    },
                    Definition::BlockParameter {
                        block: self.block,
                        index: block.parameters.len() - 1,
                    },
                ))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_nullable_consume_columns(when_null, *owner, span)?;
        self.validate_nullable_consume_columns(when_non_null, *owner, span)?;
        if block
            .parameters
            .iter()
            .filter(|&&parameter| parameter == EntityId::Value(proof.owner))
            .count()
            != 1
            || block
                .parameters
                .iter()
                .filter(|&&parameter| parameter == EntityId::Loan(proof.loan))
                .count()
                != 1
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        // The first body consumer cannot already have used or ended either operand.
        for &instruction in &block.instructions {
            let instruction = self
                .function
                .instruction(instruction)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if instruction.operation.entities().iter().any(|entity| {
                *entity == EntityId::Value(proof.owner) || *entity == EntityId::Loan(proof.loan)
            }) {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
        }
        Ok(())
    }

    fn validate_nullable_consume_columns(
        &self,
        edge: &Edge,
        owner: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let block = self
            .function
            .block(edge.target)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if edge
            .arguments
            .iter()
            .filter(|&&argument| argument == EntityId::Value(owner))
            .count()
            != 1
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        for (index, &argument) in edge.arguments.iter().enumerate() {
            let parameter = *block
                .parameters
                .get(index)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let actual = self
                .function
                .entity(argument)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let target = self
                .function
                .entity(parameter)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if !matches!(
                (argument, parameter),
                (EntityId::Value(_), EntityId::Value(_))
            ) {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            if !matches!(actual.ty, EntityType::Value(_))
                || actual.ty != target.ty
                || target.definition
                    != (Definition::BlockParameter {
                        block: edge.target,
                        index,
                    })
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        Ok(())
    }
}
