//! Current-block identity for an existing nullable branch proof, plus ASAP drop and call-prefix cleanup.
use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
    ownership_checking::{DropPoint, DropTarget, LoanEndPoint},
    source::Span,
};

use super::control::BranchExit;
use super::{
    EntityId, EntityType, ExpressionLowerer, LoanId, LoanKind, LoweredValue, LoweringError,
    LoweringErrorKind, Operation, SsaTypeId, ValueId, error,
};
use crate::ssa::model::{Ownership, SsaTypeKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ssa) enum ProofScope {
    If(ExpressionId),
    When(ExpressionId),
    While(StatementId),
    Assertion(ExpressionId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ssa) struct ProofOrigin {
    birth: LoanId,
    introduced_by: ProofScope,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ssa) struct NonNullProof {
    pub(in crate::ssa) loan: LoanId,
    pub(in crate::ssa) owner: ValueId,
    pub(in crate::ssa) inner: SsaTypeId,
    pub(in crate::ssa) origin: ProofOrigin,
}

impl NonNullProof {
    pub(in crate::ssa) fn introduced(
        loan: LoanId,
        owner: ValueId,
        inner: SsaTypeId,
        introduced_by: ProofScope,
    ) -> Self {
        Self {
            loan,
            owner,
            inner,
            origin: ProofOrigin {
                birth: loan,
                introduced_by,
            },
        }
    }

    pub(in crate::ssa) fn while_origin(self) -> Option<(StatementId, LoanId)> {
        match self.origin.introduced_by {
            ProofScope::While(statement) => Some((statement, self.origin.birth)),
            _ => None,
        }
    }

    pub(in crate::ssa) fn if_origin(self) -> Option<(ExpressionId, LoanId)> {
        match self.origin.introduced_by {
            ProofScope::If(expression) => Some((expression, self.origin.birth)),
            _ => None,
        }
    }

    pub(in crate::ssa) fn is_while_origin(self) -> bool {
        matches!(self.origin.introduced_by, ProofScope::While(_))
    }
}

impl ExpressionLowerer<'_> {
    pub(in crate::ssa) fn finish_borrowed_call(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.finish_borrowed_call_with_sources(expression, span, Vec::new())
    }

    pub(in crate::ssa) fn finish_borrowed_call_with_sources(
        &mut self,
        expression: ExpressionId,
        span: Span,
        sources: Vec<LoanId>,
    ) -> Result<(), LoweringError> {
        let ending_loans = self.owned.loans_ending_at(expression).collect::<Vec<_>>();
        for fact in &ending_loans {
            self.end_pending_source_loan(
                (expression.index(), fact.argument().index()),
                fact.end_span(),
            )?;
        }
        if self
            .pending_call_loans
            .keys()
            .any(|(call, _)| *call == expression.index())
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        for loan in sources.into_iter().rev() {
            self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
        }
        self.finish_short_call_ranges(expression, span)?;
        for fact in ending_loans {
            self.emit_drops(DropPoint::AfterExpression(fact.argument()))?;
        }
        self.emit_drops(DropPoint::CallReturn(expression))
    }

    pub(in crate::ssa) fn emit_control_transfer_cleanup(
        &mut self,
        transfer: ExpressionId,
    ) -> Result<(), LoweringError> {
        let span = self.expression_span(transfer)?;
        let endings = self
            .owned
            .loan_ends()
            .iter()
            .filter(|fact| fact.point() == LoanEndPoint::ControlTransfer(transfer))
            .map(|fact| (fact.call().index(), fact.argument().index()))
            .collect::<Vec<_>>();
        for key in &endings {
            self.end_pending_source_loan(*key, span)?;
            self.release_primitive_unit_operand(key.0, key.1);
        }
        let ranges = self
            .owned
            .borrow_results()
            .range_uses()
            .iter()
            .filter(|fact| {
                matches!(fact.site(), lang_frontend::ownership_checking::RangeUseSite::Call(call)
                    if endings.iter().any(|key| key.0 == call.index()))
                    && self.temporaries.contains_key(&fact.expression().index())
            })
            .map(|fact| fact.expression())
            .collect::<Vec<_>>();
        for range in ranges.into_iter().rev() {
            self.finish_short_range(range, span)?;
        }
        let is_return = matches!(
            self.parsed
                .ast()
                .expressions()
                .get(transfer)
                .map(|node| node.payload()),
            Ok(lang_frontend::parser::Expression::Return { .. })
        );
        let views = self
            .non_null_bindings
            .iter()
            .filter_map(|(&symbol, proof)| {
                if proof.is_while_origin() {
                    return None;
                }
                let retained = !is_return
                    && self
                        .loops
                        .last()
                        .is_some_and(|context| context.entry_views.contains_key(&symbol));
                (!retained).then_some(symbol)
            })
            .collect::<Vec<_>>();
        for symbol in views {
            self.end_non_null_proof(symbol, span)?;
        }
        self.emit_drops(DropPoint::ControlTransfer(transfer))
    }

    pub(in crate::ssa) fn emit_drops(&mut self, point: DropPoint) -> Result<(), LoweringError> {
        let facts = self
            .owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.point() == point)
            .collect::<Vec<_>>();
        self.preflight_nullable_if_cleanup_groups(&facts)?;
        self.preflight_nullable_numeric_cleanup(&facts)?;
        self.preflight_nullable_borrowed_cleanup(&facts)?;
        self.end_result_bindings(point)?;
        let mut index = 0;
        while index < facts.len() {
            if let Some(consumed) = self.emit_nullable_if_cleanup_group(&facts[index..])? {
                index += consumed;
                continue;
            }
            let fact = facts[index];
            index += 1;
            if self.emit_resource_map_drop(fact)? {
                continue;
            }
            if self.emit_local_map_drop(fact)? {
                continue;
            }
            if self.emit_outer_borrowed_drop(fact)? {
                continue;
            }
            if self.emit_nullable_borrowed_cleanup(fact)? {
                continue;
            }
            if self.emit_nullable_numeric_cleanup(fact)? {
                continue;
            }
            let template = match fact.target() {
                DropTarget::Named(symbol) => Some(self.typed.symbol_type(symbol)),
                DropTarget::Temporary(expression) => Some(self.typed.expression_type(expression)),
                _ => None,
            };
            if let Some(template) = template {
                let template = template
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, fact.value_origin()))?;
                let concrete = self.resolve_type(template, fact.value_origin())?;
                if concrete != template
                    && self.typed.copyability(concrete)
                        == Some(lang_frontend::type_checking::Copyability::Copyable)
                {
                    continue;
                }
            }
            if fact.condition().is_some()
                && !super::map_loop_owner::supported_drop(
                    self.parsed,
                    self.names,
                    self.typed,
                    self.owned,
                    fact,
                )
            {
                return Err(error(
                    LoweringErrorKind::UnsupportedNode,
                    fact.value_origin(),
                ));
            }
            let owner = match fact.target() {
                DropTarget::Named(symbol) => {
                    if matches!(point, DropPoint::LoopExit(_))
                        && !self.bindings.contains_key(&symbol)
                        && (fact.condition().is_none()
                            || super::constant_presence::constant_drop(
                                self.parsed,
                                self.names,
                                self.typed,
                                self.owned,
                                fact,
                            ))
                    {
                        continue;
                    }
                    self.end_non_null_proof(symbol, fact.value_origin())?;
                    match self.bindings.remove(&symbol) {
                        Some(LoweredValue::Value(value)) => value,
                        Some(LoweredValue::Unit | LoweredValue::Diverged) | None => {
                            return Err(error(LoweringErrorKind::MissingFact, fact.value_origin()));
                        }
                    }
                }
                DropTarget::Temporary(expression) => {
                    let owner = self
                        .temporaries
                        .remove(&expression.index())
                        .ok_or_else(|| {
                            error(LoweringErrorKind::MissingFact, fact.value_origin())
                        })?;
                    self.temporaries.retain(|_, candidate| *candidate != owner);
                    owner
                }
                DropTarget::Captured { .. } => continue,
                DropTarget::RetainedSource(_) => {
                    return Err(error(
                        LoweringErrorKind::UnsupportedNode,
                        fact.value_origin(),
                    ));
                }
                DropTarget::ReplacedElement(_) => {
                    return Err(error(
                        LoweringErrorKind::UnsupportedNode,
                        fact.value_origin(),
                    ));
                }
            };
            self.record_map_loop_drop(fact, owner)?;
            self.append(Operation::Drop { owner }, Vec::new(), fact.value_origin())?;
            self.release_owner_capture_loans(owner, fact.value_origin())?;
        }
        Ok(())
    }

    pub(in crate::ssa) fn shared_owner_operand(
        &mut self,
        receiver: ExpressionId,
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        if let Some(proof) = self.non_null_proof_for_expression(receiver, span)? {
            return Ok(EntityId::Loan(proof.loan));
        }
        self.require_value(receiver)
            .map(EntityId::Value)
            .map_err(|mut error| {
                error.span.get_or_insert(span);
                error
            })
    }

    pub(in crate::ssa) fn validate_non_null_proof_types(
        &self,
        proof: NonNullProof,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(EntityType::Value(nullable)) = self
            .function
            .entity(EntityId::Value(proof.owner))
            .map(|entity| entity.ty)
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let valid_storage = match self.ssa_types.get(nullable.index()) {
            Some(SsaTypeKind::NullableHandle { inner }) => *inner == proof.inner,
            Some(SsaTypeKind::Aggregate {
                ownership: Ownership::MoveOnly,
                ..
            }) if !proof.is_while_origin() => {
                self.nullable_maps.get(&nullable) == Some(&proof.inner)
                    || self.map_results.get(&nullable) == Some(&proof.inner)
            }
            _ => false,
        };
        if !valid_storage
            || self
                .function
                .entity(EntityId::Loan(proof.loan))
                .map(|entity| entity.ty)
                != Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: proof.inner,
                })
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    pub(in crate::ssa) fn validate_non_null_proof(
        &self,
        symbol: SymbolId,
        proof: NonNullProof,
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if bindings.get(&symbol) != Some(&LoweredValue::Value(proof.owner)) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_non_null_proof_types(proof, span)
    }

    pub(in crate::ssa) fn checked_non_null_proof(
        &self,
        symbol: SymbolId,
        inner: SsaTypeId,
        span: Span,
    ) -> Result<Option<NonNullProof>, LoweringError> {
        let Some(proof) = self.non_null_bindings.get(&symbol).copied() else {
            return Ok(None);
        };
        self.validate_non_null_proof(symbol, proof, &self.bindings, span)?;
        if proof.inner != inner {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(Some(proof))
    }

    pub(in crate::ssa) fn end_non_null_proof(
        &mut self,
        symbol: SymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(proof) = self.non_null_bindings.get(&symbol).copied() else {
            return Ok(());
        };
        self.validate_non_null_proof(symbol, proof, &self.bindings, span)?;
        if proof.is_while_origin() {
            self.ensure_no_unresolved_while_proof_carriers(span)?;
        }
        self.append(Operation::BorrowEnd { loan: proof.loan }, Vec::new(), span)?;
        self.non_null_bindings.remove(&symbol);
        Ok(())
    }

    pub(in crate::ssa) fn non_null_proof_for_expression(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<NonNullProof>, LoweringError> {
        let Some(non_null) = self.typed.non_null_use(expression) else {
            return Ok(None);
        };
        if !self.non_null_bindings.contains_key(&non_null.symbol()) {
            return Ok(None);
        }
        self.checked_non_null_proof(
            non_null.symbol(),
            self.expression_ssa_type(expression, span)?,
            span,
        )
    }

    pub(in crate::ssa) fn validate_non_null_exit_proofs(
        &self,
        exits: &[BranchExit],
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(first) = exits.first() else {
            return Ok(());
        };
        for exit in exits {
            if exit.views.keys().ne(first.views.keys()) {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            for (&symbol, &proof) in &exit.views {
                self.validate_non_null_proof(symbol, proof, &exit.bindings, span)?;
                if proof.inner != first.views[&symbol].inner
                    || proof.origin != first.views[&symbol].origin
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
            }
        }
        Ok(())
    }

    pub(in crate::ssa) fn merged_non_null_proofs(
        &self,
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        parameters: Vec<(SymbolId, LoanId, NonNullProof)>,
        span: Span,
    ) -> Result<BTreeMap<SymbolId, NonNullProof>, LoweringError> {
        let mut views = BTreeMap::new();
        for (symbol, loan, source) in parameters {
            let Some(LoweredValue::Value(owner)) = bindings.get(&symbol).copied() else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let proof = NonNullProof {
                loan,
                owner,
                ..source
            };
            self.validate_non_null_proof(symbol, proof, bindings, span)?;
            views.insert(symbol, proof);
        }
        Ok(views)
    }
}


pub(in crate::ssa) mod outer_borrowed_provider {
    //! Actual provider and nullable-branch births; no loan is identified by type alone.
    use super::super::outer_borrowed_while::carrier::{
        Columns, Guard, forward, incoming, instructions, parameter, result, transport,
        transport_states,
    };
    use super::super::outer_borrowed_while::*;
    use crate::ssa::model::{BinaryOperator, Instruction, SsaTypeKind};
    pub(in crate::ssa) struct Provider {
        pub(in crate::ssa) preheader: BlockId,
        pub(in crate::ssa) header: Columns,
        pub(in crate::ssa) body: Columns,
        pub(in crate::ssa) exhausted: Columns,
        pub(in crate::ssa) length: ValueId,
        pub(in crate::ssa) cursor: ValueId,
        pub(in crate::ssa) body_cursor: ValueId,
        pub(in crate::ssa) step: ValueId,
    }
    pub(in crate::ssa) fn value(entity: EntityId) -> Option<ValueId> {
        if let EntityId::Value(value) = entity {
            Some(value)
        } else {
            None
        }
    }
    pub(in crate::ssa) fn loan(entity: EntityId) -> Option<LoanId> {
        if let EntityId::Loan(loan) = entity {
            Some(loan)
        } else {
            None
        }
    }
    fn only_result(instruction: &Instruction) -> Option<EntityId> {
        let [entity] = instruction.results.as_slice() else {
            return None;
        };
        Some(*entity)
    }
    pub(in crate::ssa) fn context<'a>(
        lowerer: &'a ExpressionLowerer<'_>,
        relation: &Relation,
    ) -> Option<&'a loop_control::LoopContext> {
        let [outer, rest @ ..] = lowerer.loops.as_slice() else {
            return None;
        };
        if rest.len() > 1
            || rest.iter().any(|context| {
                context.for_loop.is_some()
                    || context.numeric_loop.is_some()
                    || context.outer_borrowed_while.as_ref() != Some(relation)
            })
            || outer.numeric_loop.is_some()
            || outer.for_loop.as_ref()?.statement != relation.outer
            || !outer.entry_views.is_empty()
            || !outer.breaks.is_empty()
            || !outer.continues.is_empty()
        {
            return None;
        }
        Some(outer)
    }
    pub(in crate::ssa) fn shape(
        lowerer: &ExpressionLowerer<'_>,
        relation: &Relation,
    ) -> Option<Provider> {
        let function = &*lowerer.function;
        let context = context(lowerer, relation)?;
        let data = context.for_loop.as_ref()?;
        let span = lowerer
            .parsed
            .ast()
            .statements()
            .get(relation.outer)
            .ok()?
            .span();
        let descriptor = lowerer.owned.iteration(relation.outer)?.descriptor();
        let source_type = *lowerer.type_ids.get(&descriptor.source_type())?;
        let element_type = *lowerer.type_ids.get(&descriptor.element_type())?;
        let source = data.new_source_loan?;
        let begin = result(function, EntityId::Loan(source))?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = begin.operation
        else {
            return None;
        };
        let root = result(function, EntityId::Place(place))?;
        let Operation::RootPlace { owner: items } = root.operation else {
            return None;
        };
        let preheader = root.block;
        let binding = |symbol| {
            unique(
                context
                    .carried
                    .slots
                    .iter()
                    .filter(|slot| slot.symbol == Some(symbol)),
            )
        };
        let current = value(binding(relation.root)?.source)?;
        let construct = result(function, EntityId::Value(items))?;
        if data.is_carried_source
            || binding(relation.items)?.source != EntityId::Value(items)
            || current == items
            || parameter(function, EntityId::Value(current))?
                != (function.entry_block()?, relation.parameter_slot)
            || !matches!(construct.operation, Operation::ContainerConstruct { container, .. } if container == source_type)
            || function.entity(EntityId::Value(items))?.ty != EntityType::Value(source_type)
            || construct.origin
                != Origin::Source(
                    lowerer
                        .parsed
                        .ast()
                        .expressions()
                        .get(relation.initializer)
                        .ok()?
                        .span(),
                )
            || begin.block != preheader
            || begin.origin != Origin::Source(span)
            || root.origin != Origin::Source(span)
            || function.entity(EntityId::Place(place))?.ty != EntityType::Place(source_type)
            || function.entity(EntityId::Loan(source))?.ty
                != (EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: source_type,
                })
            || function.entity(EntityId::Value(current))?.ty
                != EntityType::Value(*lowerer.type_ids.get(&relation.storage)?)
            || lowerer.ssa_types.get(element_type.index())
                != Some(&SsaTypeKind::Integer {
                    bits: 32,
                    signed: true,
                })
            || !matches!(lowerer.ssa_types.get(source_type.index()), Some(SsaTypeKind::SequentialContainer { kind: crate::ssa::model::SequentialContainerKind::List, element }) if *element == element_type)
        {
            return None;
        }
        let ops = instructions(function, preheader)?;
        let position = |instruction: &Instruction| ops.iter().position(|op| op.id == instruction.id);
        if position(root)? >= position(begin)? {
            return None;
        }
        let length = unique(ops.iter().copied().filter(|instruction| {
            instruction.operation
                == (Operation::ContainerLength {
                    owner: EntityId::Loan(source),
                })
        }))?;
        let constant = |number| {
            unique(ops.iter().copied().filter(|instruction| {
                instruction.origin == Origin::Source(span)
                    && instruction.operation == Operation::Constant(ScalarConstant::Integer(number))
            }))
        };
        let zero = constant(0)?;
        let one = constant(1)?;
        let step = value(only_result(one)?)?;
        if position(begin)? >= position(length)? || length.origin != Origin::Source(span) {
            return None;
        }
        let term = function.block(preheader)?.terminator.as_ref()?;
        let TerminatorKind::Branch(entry) = &term.kind else {
            return None;
        };
        if term.origin != Origin::Source(span) || entry.target != context.header {
            return None;
        }
        let header = forward(
            function,
            entry,
            &[
                EntityId::Value(current),
                EntityId::Value(items),
                EntityId::Loan(source),
            ],
        )?;
        let numbers = forward(function, entry, &[only_result(length)?, only_result(zero)?])?;
        let (length, cursor) = (value(numbers.entities[0])?, value(numbers.entities[1])?);
        let term = function.block(header.block)?.terminator.as_ref()?;
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &term.kind
        else {
            return None;
        };
        let compare = result(function, EntityId::Value(*condition))?;
        if term.origin != Origin::Source(span)
            || compare.block != header.block
            || compare.origin != Origin::Source(span)
            || compare.operation
                != (Operation::Compare {
                    operator: ComparisonOperator::LessThan,
                    left: cursor,
                    right: length,
                })
            || when_true.target == when_false.target
            || when_true.target == header.block
            || when_false.target == header.block
        {
            return None;
        }
        let mut body = forward(function, when_true, &header.entities)?;
        let exhausted = forward(function, when_false, &header.entities)?;
        if incoming(function, body.block).as_slice() != [(header.block, when_true)]
            || incoming(function, exhausted.block).as_slice() != [(header.block, when_false)]
        {
            return None;
        }
        let body_cursor =
            value(forward(function, when_true, &[EntityId::Value(cursor)])?.entities[0])?;
        let body_source = loan(body.entities[2])?;
        let ops = instructions(function, body.block)?;
        let element_place = unique(ops.iter().copied().filter(|instruction| {
            matches!(
                instruction.operation,
                Operation::ContainerElementPlace { .. }
            )
        }))?;
        let EntityId::Place(place) = only_result(element_place)? else {
            return None;
        };
        let begin = unique(
            ops.iter()
                .copied()
                .filter(|instruction| matches!(instruction.operation, Operation::BorrowBegin { .. })),
        )?;
        if element_place.operation
            != (Operation::ContainerElementPlace {
                owner: EntityId::Loan(body_source),
                index: body_cursor,
            })
            || begin.operation
                != (Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                })
            || element_place.origin != Origin::Source(span)
            || begin.origin != Origin::Source(span)
            || function.entity(EntityId::Place(place))?.ty != EntityType::Place(element_type)
            || ops.iter().position(|op| op.id == element_place.id)?
                >= ops.iter().position(|op| op.id == begin.id)?
        {
            return None;
        }
        let element = loan(only_result(begin)?)?;
        if function.entity(EntityId::Loan(element))?.ty
            != (EntityType::Loan {
                kind: LoanKind::Shared,
                target: element_type,
            })
        {
            return None;
        }
        body.entities.push(EntityId::Loan(element));
        if data.provider_header.block != header.block
            || Some(data.provider_header.source) != loan(header.entities[2])
            || data.provider_header.length != length
            || data.provider_header.cursor != cursor
            || data.provider_header.step != step
            || parameter(function, header.entities[2])?.1 != data.source_slot
            || data.source_slot != data.provider_header.source_slot
            || parameter(function, EntityId::Value(length))?.1 != data.length_slot
            || data.length_slot != data.provider_header.length_slot
            || parameter(function, EntityId::Value(cursor))?.1 != data.cursor_slot
            || data.cursor_slot != data.provider_header.cursor_slot
            || data.guarded_element.body() != body.block
            || data.guarded_element.header() != header.block
            || data.guarded_element.source() != body_source
            || data.guarded_element.cursor() != body_cursor
            || data.body_cursor != body_cursor
            || data.guarded_element.step() != step
        {
            return None;
        }
        for state in [&header, &body, &exhausted] {
            if function.block(state.block)?.origin != Origin::Source(span)
                || function
                    .block(state.block)?
                    .parameters
                    .iter()
                    .any(|entity| {
                        matches!(entity, EntityId::Loan(_)) && !state.entities.contains(entity)
                    })
            {
                return None;
            }
        }
        Some(Provider {
            preheader,
            header,
            body,
            exhausted,
            length,
            cursor,
            body_cursor,
            step,
        })
    }

    /// The only NullableBranch which transport may cross. Its two owner/provider
    /// columns are explicit; only the non-null target owns the newly born view.
    pub(in crate::ssa) fn guard(
        lowerer: &ExpressionLowerer<'_>,
        relation: &Relation,
        provider: &Provider,
    ) -> Option<Guard> {
        let function = &*lowerer.function;
        let span = lowerer
            .parsed
            .ast()
            .statements()
            .get(relation.inner)
            .ok()?
            .span();
        let (block, owner, view, when_null, when_non_null) =
            unique(function.blocks.iter().filter_map(|block| {
                let term = block.terminator.as_ref()?;
                let TerminatorKind::NullableBranch {
                    owner,
                    view,
                    when_null,
                    when_non_null,
                } = &term.kind
                else {
                    return None;
                };
                (term.origin == Origin::Source(span)).then_some((
                    block.id,
                    *owner,
                    *view,
                    when_null,
                    when_non_null,
                ))
            }))?;
        let (body_parent, edge) = unique(incoming(function, block).into_iter())?;
        if body_parent != provider.body.block
            || forward(function, edge, &provider.body.entities)?.entities[0]
                != EntityId::Value(owner)
        {
            return None;
        }
        let header = forward(function, edge, &provider.body.entities)?;
        if !transport(function, header.clone(), &provider.body, None, None) {
            return None;
        }
        let mut yes = forward(function, when_non_null, &header.entities)?;
        let no = forward(function, when_null, &header.entities)?;
        let target = function.block(yes.block)?;
        let storage = *lowerer.type_ids.get(&relation.storage)?;
        let SsaTypeKind::NullableHandle { inner } = lowerer.ssa_types.get(storage.index())? else {
            return None;
        };
        if no.block == yes.block
            || target.parameters.last() != Some(&EntityId::Loan(view))
            || target.parameters.len() != when_non_null.arguments.len() + 1
            || when_null.arguments.len() != function.block(no.block)?.parameters.len()
            || when_non_null.arguments.contains(&EntityId::Loan(view))
            || when_null.arguments.contains(&EntityId::Loan(view))
            || function.entity(EntityId::Loan(view))?.ty
                != (EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: *inner,
                })
            || incoming(function, yes.block).as_slice() != [(block, when_non_null)]
            || incoming(function, no.block).as_slice() != [(block, when_null)]
            || !matches!(
                lowerer.ssa_types.get(inner.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            )
        {
            return None;
        }
        yes.entities.push(EntityId::Loan(view));
        Some(Guard {
            header: block,
            owner,
            view,
            when_null: no.block,
            when_non_null: yes.block,
            non_null: yes,
        })
    }

    pub(in crate::ssa) fn completed_backedge(
        lowerer: &ExpressionLowerer<'_>,
        relation: &Relation,
        provider: &Provider,
    ) -> Option<()> {
        let function = &*lowerer.function;
        let guard = guard(lowerer, relation, provider)?;
        let edges = incoming(function, provider.header.block);
        if edges.len() != 2
            || edges
                .iter()
                .filter(|(block, _)| *block == provider.preheader)
                .count()
                != 1
        {
            return None;
        }
        let (block, edge) = unique(
            edges
                .into_iter()
                .filter(|(block, _)| *block != provider.preheader),
        )?;
        let span = lowerer
            .parsed
            .ast()
            .statements()
            .get(relation.outer)
            .ok()?
            .span();
        let term = function.block(block)?.terminator.as_ref()?;
        if term.origin != Origin::Source(span)
            || !matches!(&term.kind, TerminatorKind::Branch(actual) if actual == edge)
        {
            return None;
        }
        let ops = instructions(function, block)?;
        let end = unique(
            ops.iter()
                .copied()
                .filter(|op| matches!(op.operation, Operation::BorrowEnd { .. })),
        )?;
        let Operation::BorrowEnd { loan: element } = end.operation else {
            return None;
        };
        let mut entities = provider
            .header
            .entities
            .iter()
            .map(|entity| edge.arguments.get(parameter(function, *entity)?.1).copied())
            .collect::<Option<Vec<_>>>()?;
        entities.push(EntityId::Loan(element));
        if end.origin != Origin::Source(span)
            || edge.arguments.contains(&EntityId::Loan(element))
            || edge.arguments.contains(&EntityId::Loan(guard.view))
        {
            return None;
        }
        let carried = transport_states(
            function,
            Columns { block, entities },
            &provider.body,
            Some((block, element)),
            Some(&guard),
        )?;
        let length = parameter(function, EntityId::Value(provider.length))?.1;
        let cursor = parameter(function, EntityId::Value(provider.cursor))?.1;
        let next = result(function, *edge.arguments.get(cursor)?)?;
        if edge.arguments.get(length) != Some(&EntityId::Value(provider.length))
            || next.block != block
            || next.origin != Origin::Source(span)
            || next.operation
                != (Operation::Binary {
                    operator: BinaryOperator::Add,
                    left: provider.body_cursor,
                    right: provider.step,
                })
            || ops.iter().position(|op| op.id == end.id)?
                >= ops.iter().position(|op| op.id == next.id)?
        {
            return None;
        }
        // Checked scalar operations rebind all five columns. Locate the real break
        // block and trace its current view together with its owner and provider.
        let break_span = lowerer
            .parsed
            .ast()
            .expressions()
            .get(relation.breaking)
            .ok()?
            .span();
        let end_view = unique(
            function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .filter_map(|id| function.instruction(*id))
                .filter(|op| {
                    op.origin == Origin::Source(break_span)
                        && matches!(op.operation, Operation::BorrowEnd { .. })
                }),
        )?;
        let Operation::BorrowEnd { loan: view } = end_view.operation else {
            return None;
        };
        let mut breaking =
            unique(carried.iter().filter(|state| state.block == end_view.block))?.clone();
        let TerminatorKind::Branch(merge_edge) =
            &function.block(breaking.block)?.terminator.as_ref()?.kind
        else {
            return None;
        };
        let merged = forward(function, merge_edge, &breaking.entities)?;
        breaking.entities.push(EntityId::Loan(view));
        if merge_edge.arguments.contains(&EntityId::Loan(view))
            || edge.arguments.contains(&EntityId::Loan(view))
            || function
                .block(merged.block)?
                .parameters
                .iter()
                .any(|entity| {
                    matches!(entity, EntityId::Loan(_)) && !merged.entities.contains(entity)
                })
            || !transport(
                function,
                breaking,
                &guard.non_null,
                Some((end_view.block, view)),
                None,
            )
            || instructions(function, guard.when_null)?.iter().any(|op| {
                matches!(
                    op.operation,
                    Operation::BorrowEnd { .. } | Operation::Drop { .. }
                )
            })
        {
            return None;
        }
        Some(())
    }
}
