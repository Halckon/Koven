//! Direct If-born nullable Map ownership extraction, before ordinary consume.
#[cfg(test)]
mod guard_tests;

use std::collections::BTreeSet;

use super::super::*;
use crate::ssa::borrow_result_support::ungroup;
use crate::ssa::lowering_support::callable_instances::SourceToken;
use crate::ssa::model::Definition;
use lang_frontend::{
    name_resolution::ScopeKind,
    ownership_checking::{DropTarget, IterationCleanupAction, LoanEndPoint},
    parser::VariableKind,
    type_checking::{ExpressionCategory, MapConsumeDescriptor},
};

pub(in crate::ssa::lower_frontend::map_consume) struct NullableConsumeSource {
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

    fn preflight_nullable_consume_cleanup(
        &self,
        source: &NullableConsumeSource,
        owner: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        // The carrier gate established no borrow state. A fact requiring that state
        // must fail here, rather than being swallowed by an absent-binding fallback.
        if self
            .owned
            .loans()
            .iter()
            .any(|loan| loan.call() == source.call || source.operands.contains(&loan.argument()))
            || self
                .owned
                .loan_ends()
                .iter()
                .any(|fact| fact.point() == LoanEndPoint::CallReturn(source.call))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let mut removed_symbols = BTreeSet::new();
        let mut removed_owners = BTreeSet::new();
        removed_symbols.insert(source.root);
        removed_owners.insert(owner);
        let points = source
            .operands
            .iter()
            .copied()
            .map(DropPoint::AfterExpression)
            .chain(std::iter::once(DropPoint::CallReturn(source.call)));
        for point in points {
            if self
                .owned
                .borrow_results()
                .ends()
                .iter()
                .any(|fact| fact.point() == point)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let facts = self
                .owned
                .drops()
                .iter()
                .copied()
                .filter(|fact| fact.point() == point)
                .collect::<Vec<_>>();
            let mut step_drops = Vec::new();
            for (_, action) in self
                .owned
                .cleanup_steps()
                .iter()
                .filter(|(at, _)| *at == point)
            {
                match action {
                    IterationCleanupAction::Drop(fact) => step_drops.push(*fact),
                    _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
                }
            }
            if step_drops != facts {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            for fact in facts {
                if fact.condition().is_some()
                    || fact.capture_slot().is_some()
                    || fact.instance_address().is_some()
                {
                    return Err(error(
                        LoweringErrorKind::UnsupportedNode,
                        fact.value_origin(),
                    ));
                }
                let DropTarget::Named(symbol) = fact.target() else {
                    // No receiver temporary is formed by Name/Group, and the call's
                    // result is not registered until the enclosing lower() returns.
                    return Err(error(LoweringErrorKind::MissingFact, fact.value_origin()));
                };
                let Some(LoweredValue::Value(value)) = self.bindings.get(&symbol).copied() else {
                    return Err(error(LoweringErrorKind::MissingFact, fact.value_origin()));
                };
                if !removed_symbols.insert(symbol) || !removed_owners.insert(value) {
                    return Err(error(LoweringErrorKind::MissingFact, fact.value_origin()));
                }
                let template = self
                    .typed
                    .symbol_type(symbol)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, fact.value_origin()))?;
                let concrete = self.resolve_type(template, fact.value_origin())?;
                let ty =
                    self.type_ids.get(&concrete).copied().ok_or_else(|| {
                        error(LoweringErrorKind::MissingFact, fact.value_origin())
                    })?;
                if self.typed.copyability(concrete) != Some(Copyability::MoveOnly)
                    || self
                        .function
                        .entity(EntityId::Value(value))
                        .map(|data| data.ty)
                        != Some(EntityType::Value(ty))
                {
                    return Err(error(LoweringErrorKind::MissingFact, fact.value_origin()));
                }
            }
        }
        Ok(())
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

    pub(super) fn nullable_consume_source(
        &self,
        descriptor: MapConsumeDescriptor,
        inner: SsaTypeId,
        span: Span,
    ) -> Result<NullableConsumeSource, LoweringError> {
        let (root, storage) = descriptor
            .source()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let proof = self
            .non_null_bindings
            .get(&root)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let (control, _) = proof
            .if_origin()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let source = self.nullable_consume_local_source(root, control, span)?;
        let declared = self.resolve_type(storage, span)?;
        let narrowed = self.resolve_type(descriptor.receiver_type(), span)?;
        let nullable = *self
            .type_ids
            .get(&declared)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(self.typed.types().get(declared), Some(TypeKind::Nullable(target)) if *target == narrowed)
            || self.typed.copyability(declared) != Some(Copyability::MoveOnly)
            || self.type_ids.get(&narrowed) != Some(&inner)
            || self.nullable_maps.get(&nullable) != Some(&inner)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_nullable_consume_call(descriptor, span)?;
        let mut operand = descriptor.receiver();
        let mut operands = Vec::new();
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(operand)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            let fact = self
                .typed
                .non_null_use(operand)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if fact.expression() != operand
                || fact.symbol() != root
                || fact.declared_type() != storage
                || fact.narrowed_type() != descriptor.receiver_type()
                || self.typed.expression_type(operand) != Some(descriptor.receiver_type())
                || self.typed.expression_category(operand) != Some(ExpressionCategory::Place)
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            operands.push(operand);
            match node.payload() {
                Expression::Group { expression } => operand = *expression,
                Expression::Name if self.references.get(&span_key(node.span())) == Some(&root) => {
                    break;
                }
                _ => return Err(error(LoweringErrorKind::MissingFact, span)),
            }
        }
        operands.reverse();
        let comparison = self.nullable_consume_comparison(control, span)?;
        let condition = self
            .typed
            .null_comparison(comparison)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let source = NullableConsumeSource {
            source,
            control,
            call: descriptor.expression(),
            root,
            storage,
            nullable,
            inner,
            non_null_when_true: condition.non_null_when_true(),
            operands,
        };
        self.validate_nullable_consume_control(&source, span)?;
        Ok(source)
    }

    fn validate_nullable_consume_call(
        &self,
        descriptor: MapConsumeDescriptor,
        span: Span,
    ) -> Result<(), LoweringError> {
        let call = self
            .parsed
            .ast()
            .expressions()
            .get(descriptor.expression())
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = call.payload()
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let member = self
            .parsed
            .ast()
            .expressions()
            .get(*callee)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        if !arguments.is_empty()
            || !type_arguments.is_empty()
            || !matches!(member.payload(), Expression::Member { receiver, safe: false, .. }
                if *receiver == descriptor.receiver())
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    fn nullable_consume_comparison(
        &self,
        control: ExpressionId,
        span: Span,
    ) -> Result<ExpressionId, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(control)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::If { condition, .. } = node.payload() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        ungroup(self.parsed, *condition, span)
    }

    pub(super) fn validate_nullable_consume_control(
        &self,
        source: &NullableConsumeSource,
        span: Span,
    ) -> Result<(), LoweringError> {
        let comparison = self.nullable_consume_comparison(source.control, span)?;
        let descriptor = self
            .typed
            .null_comparison(comparison)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.expression() != comparison
            || descriptor.symbol() != source.root
            || descriptor.nullable_type() != source.storage
            || descriptor.non_null_when_true() != source.non_null_when_true
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_nullable_consume_condition(source, comparison, span)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(source.control)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::If {
            then_branch,
            else_branch,
            ..
        } = node.payload()
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let body = if source.non_null_when_true {
            Some(*then_branch)
        } else {
            *else_branch
        }
        .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let body = self
            .parsed
            .ast()
            .statements()
            .get(body)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Statement::ControlBody { elements } = body.payload() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let first = elements
            .first()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let first = self
            .parsed
            .ast()
            .statements()
            .get(*first)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let first_expression = match first.payload() {
            Statement::Expression { expression } => *expression,
            Statement::LocalVariable { declaration } => {
                let (item, _) = orchestrate::unwrap_modified(self.parsed, *declaration)?;
                let Item::Variable {
                    initializer,
                    kind: VariableKind::Val | VariableKind::Var,
                    ..
                } = item
                else {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                };
                initializer
            }
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        if ungroup(self.parsed, first_expression, span)? != source.call {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    fn validate_nullable_consume_condition(
        &self,
        source: &NullableConsumeSource,
        comparison: ExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(comparison)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Binary {
            left,
            operator,
            right,
            ..
        } = *node.payload()
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let polarity = match operator {
            AstBinaryOperator::NotEqual => true,
            AstBinaryOperator::Equal => false,
            _ => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        let is_null = |operand| -> Result<bool, LoweringError> {
            let operand = ungroup(self.parsed, operand, span)?;
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(operand)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            Ok(matches!(
                node.payload(),
                Expression::Literal(LiteralKind::Null)
            ))
        };
        let mut subject = match (is_null(left)?, is_null(right)?) {
            (true, false) => right,
            (false, true) => left,
            _ => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        if polarity != source.non_null_when_true {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(subject)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            if self.typed.expression_type(subject) != Some(source.storage)
                || self.typed.expression_category(subject) != Some(ExpressionCategory::Place)
                || self.typed.non_null_use(subject).is_some()
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            match node.payload() {
                Expression::Group { expression } => subject = *expression,
                Expression::Name
                    if self.references.get(&span_key(node.span())) == Some(&source.root) =>
                {
                    return Ok(());
                }
                _ => return Err(error(LoweringErrorKind::MissingFact, span)),
            }
        }
    }

    fn nullable_consume_local_source(
        &self,
        root: SymbolId,
        control: ExpressionId,
        span: Span,
    ) -> Result<SourceToken, LoweringError> {
        let source = self
            .source_token
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let key = self
            .instance_plan
            .source(source)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let callable = self
            .names
            .symbols()
            .get(key.symbol().index())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let local = self
            .names
            .symbols()
            .get(root.index())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if self.function_ids.get(key) != Some(&self.function.id)
            || callable.id() != key.symbol()
            || callable.kind() != SymbolKind::Function
            || local.id() != root
            || local.kind() != SymbolKind::Variable
            || local.is_synthetic()
            || local.name() == "_"
            || self.typed.parameter_mode(root).is_some()
            || !self.owned.formed_owned_locals().contains(&root)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let mut functions = self.parsed.ast().items().iter().filter_map(|(_, node)| {
            matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. }
                if *name == callable.span())
            .then_some(node.span())
        });
        let function_span = functions
            .next()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let control_span = self.expression_span(control)?;
        if functions.next().is_some()
            || self.function.origin != Origin::Source(function_span)
            || control_span.source_id() != self.parsed.source_id()
            || function_span.source_id() != control_span.source_id()
            || control_span.start() < function_span.start()
            || control_span.end() > function_span.end()
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_nullable_consume_local(root, function_span, span)?;
        Ok(source)
    }

    fn validate_nullable_consume_local(
        &self,
        root: SymbolId,
        function_span: Span,
        span: Span,
    ) -> Result<(), LoweringError> {
        let local = &self.names.symbols()[root.index()];
        let mut declarations = 0;
        for (_, node) in self.parsed.ast().statements().iter() {
            let Statement::LocalVariable { declaration } = node.payload() else {
                continue;
            };
            let (item, _) = orchestrate::unwrap_modified(self.parsed, *declaration)?;
            if matches!(item, Item::Variable {
                name: NameMarker::Present(name), kind: VariableKind::Val | VariableKind::Var, ..
            } if name == local.span())
            {
                declarations += 1;
            }
        }
        if declarations != 1 || local.span().source_id() != self.parsed.source_id() {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let mut id = local.scope();
        for _ in 0..self.names.scopes().len() {
            let scope = self
                .names
                .scopes()
                .get(id.index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            match scope.kind() {
                ScopeKind::Function if scope.span() == Some(function_span) => return Ok(()),
                ScopeKind::Block | ScopeKind::ControlBody | ScopeKind::Loop => {
                    id = scope
                        .parent()
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                }
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
        Err(error(LoweringErrorKind::MissingFact, span))
    }
}
