//! Exact source identity and provider transport; no same-type or same-name owner substitution.
use super::*;
use crate::ssa::model::{Definition, Edge, Instruction};
use lang_frontend::{ownership_checking::UnitDropFact, parser::Expression};

#[derive(Clone, Copy)]
pub(super) struct MapCarrier {
    pub(super) identity: SourceIdentity,
    pub(super) drop: Option<UnitDropFact>,
    transfer_drop: Option<UnitDropFact>,
    pub(super) body_point: UnitDropPoint,
    body_exit: UnitIterationExitKind,
    pub(super) exhaustion_point: UnitDropPoint,
    pub(super) header: BlockId,
    pub(super) body: BlockId,
    pub(super) resumed: Option<nested::ParentRoles>,
    pub(super) nested: Option<map_iteration::QualifiedNested>,
    pub(super) anchor: EntityId,
    preheader: BlockId,
    pub(super) exhaustion: BlockId,
    seed_column: usize,
    source_column: usize,
    cursor_column: usize,
    pub(super) conditional: Option<map_iteration::QualifiedConditional>,
}

/// Read-only identities captured at the real provider birth; not a resumable lowering state.
#[derive(Clone, Copy)]
pub(super) struct Birth<'a> {
    pub(super) qualified: map_iteration::QualifiedMap,
    pub(super) seed: EntityId,
    pub(super) source: LoanId,
    pub(super) cursor: ValueId,
    pub(super) preheader: BlockId,
    pub(super) header: BlockId,
    pub(super) body: BlockId,
    pub(super) exhaustion: BlockId,
    pub(super) edge: &'a Edge,
    pub(super) span: Span,
}

impl MapCarrier {
    pub(super) fn new(
        lowerer: &UnitExpressionLowerer<'_>,
        birth: &Birth<'_>,
    ) -> Result<Self, LoweringError> {
        let Birth {
            qualified,
            seed,
            source,
            cursor,
            preheader,
            header,
            body,
            exhaustion,
            edge,
            span,
        } = *birth;
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let function = &lowerer.function;
        let anchor = match qualified.source {
            SourceIdentity::Parameter { slot, .. } => {
                let entry = function.entry_block().ok_or_else(bad)?;
                let anchor = transport::parameter(function, entry, slot, span)?;
                if qualified.nested.is_some() {
                    transport::from_anchor(function, seed, anchor, span)?;
                } else if anchor != seed {
                    return Err(bad());
                }
                anchor
            }
            SourceIdentity::Temporary(expression) => {
                source::validate_temporary_birth(lowerer, expression, seed, preheader, span)?;
                seed
            }
        };
        if edge.target != header
            || !matches!(function.block(preheader).and_then(|block| block.terminator.as_ref()).map(|term| &term.kind), Some(TerminatorKind::Branch(actual)) if actual == edge)
        {
            return Err(bad());
        }
        let source_birth = instruction(function, EntityId::Loan(source), span)?;
        if source_birth.block != preheader {
            return Err(bad());
        }
        match (seed, &source_birth.operation, qualified.source_drop) {
            (
                EntityId::Value(seed),
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                Some(_),
            ) => {
                let root = instruction(function, EntityId::Place(*place), span)?;
                if root.block != preheader
                    || root.operation != (Operation::RootPlace { owner: seed })
                {
                    return Err(bad());
                }
            }
            (EntityId::Loan(seed), Operation::SharedReborrow { source }, None)
                if *source == seed => {}
            _ => return Err(bad()),
        }
        let first = instruction(function, EntityId::Value(cursor), span)?;
        if first.block != preheader || first.operation != (Operation::MapIterationFirst { source })
        {
            return Err(bad());
        }
        let slot = |entity| -> Result<usize, LoweringError> {
            let columns = edge
                .arguments
                .iter()
                .enumerate()
                .filter_map(|(index, argument)| (*argument == entity).then_some(index))
                .collect::<Vec<_>>();
            match columns.as_slice() {
                [column] => Ok(*column),
                _ => Err(bad()),
            }
        };
        let carrier = Self {
            identity: qualified.source,
            conditional: qualified.conditional,
            drop: qualified.source_drop,
            transfer_drop: qualified.transfer_drop,
            body_point: qualified.body_point,
            body_exit: qualified.body_exit,
            exhaustion_point: UnitDropPoint::LoopExit(qualified.statement),
            header,
            body,
            resumed: None,
            nested: qualified.nested,
            anchor,
            preheader,
            exhaustion,
            seed_column: slot(seed)?,
            source_column: slot(EntityId::Loan(source))?,
            cursor_column: slot(EntityId::Value(cursor))?,
        };
        let Some(TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        }) = function
            .block(header)
            .and_then(|block| block.terminator.as_ref())
            .map(|term| &term.kind)
        else {
            return Err(bad());
        };
        if when_true.target != body || when_false.target != exhaustion {
            return Err(bad());
        }
        let header_source = carrier.parameter(function, header, carrier.source_column, span)?;
        let header_cursor = carrier.parameter(function, header, carrier.cursor_column, span)?;
        let (EntityId::Loan(header_source), EntityId::Value(header_cursor)) =
            (header_source, header_cursor)
        else {
            return Err(bad());
        };
        let guard = instruction(function, EntityId::Value(*condition), span)?;
        if guard.block != header
            || guard.operation
                != (Operation::MapIterationHasNext {
                    source: header_source,
                    cursor: header_cursor,
                })
        {
            return Err(bad());
        }
        for successor in [when_true, when_false] {
            for column in [
                carrier.seed_column,
                carrier.source_column,
                carrier.cursor_column,
            ] {
                if successor.arguments.get(column).copied()
                    != Some(carrier.parameter(function, header, column, span)?)
                {
                    return Err(bad());
                }
                carrier.parameter(function, successor.target, column, span)?;
            }
        }
        if lowerer.block != header
            || carrier.identity.current(lowerer, span)?
                != carrier.parameter(function, header, carrier.seed_column, span)?
        {
            return Err(bad());
        }
        Ok(carrier)
    }

    fn parameter(
        &self,
        function: &crate::ssa::model::Function,
        block: BlockId,
        column: usize,
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let parameter = function
            .block(block)
            .and_then(|block| block.parameters.get(column))
            .copied()
            .ok_or_else(bad)?;
        if function.entity(parameter).map(|entity| entity.definition)
            != Some(Definition::BlockParameter {
                block,
                index: column,
            })
        {
            return Err(bad());
        }
        Ok(parameter)
    }

    pub(super) fn validate_current(
        &self,
        lowerer: &UnitExpressionLowerer<'_>,
        source_slot: usize,
        cursor_slot: usize,
        block: BlockId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        if lowerer.block != block {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let [seed, source, cursor] = self.current_entities(lowerer.function, block, span)?;
        if lowerer.pending_operands.get(source_slot) != Some(&source)
            || lowerer.pending_operands.get(cursor_slot) != Some(&cursor)
        {
            return Err(bad());
        }
        if self.nested.is_some() {
            transport::from_anchor(lowerer.function, seed, self.anchor, span)?;
        }
        if self.identity.current(lowerer, span)? != seed
            || !matches!(
                (seed, self.drop),
                (EntityId::Value(_), Some(_)) | (EntityId::Loan(_), None)
            )
        {
            return Err(bad());
        }
        Ok(())
    }

    pub(super) fn validate_backedge(
        &self,
        lowerer: &UnitExpressionLowerer<'_>,
        source_slot: usize,
        cursor_slot: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let body = self.active_body();
        let [seed, source, cursor] = self.current_entities(lowerer.function, body, span)?;
        let Some(TerminatorKind::Branch(edge)) = lowerer
            .function
            .block(body)
            .and_then(|block| block.terminator.as_ref())
            .map(|term| &term.kind)
        else {
            return Err(bad());
        };
        if edge.target != self.header
            || lowerer.block != body
            || self.identity.current(lowerer, span)? != seed
        {
            return Err(bad());
        }
        for (column, entity) in [(self.seed_column, seed), (self.source_column, source)] {
            if edge.arguments.get(column).copied() != Some(entity) {
                return Err(bad());
            }
        }
        let (EntityId::Loan(source), EntityId::Value(cursor)) = (source, cursor) else {
            return Err(bad());
        };
        let next = edge
            .arguments
            .get(self.cursor_column)
            .copied()
            .ok_or_else(bad)?;
        let advance = instruction(lowerer.function, next, span)?;
        if advance.block != body
            || advance.operation != (Operation::MapIterationNext { source, cursor })
            || lowerer.pending_operands.get(source_slot) != Some(&EntityId::Loan(source))
            || lowerer.pending_operands.get(cursor_slot) != Some(&next)
        {
            return Err(bad());
        }
        if self.nested.is_some() {
            let incoming = transport::incoming(lowerer.function, self.header);
            if incoming.len() != 2
                || incoming
                    .iter()
                    .filter(|(block, _)| *block == self.preheader)
                    .count()
                    != 1
                || incoming.iter().filter(|(block, _)| *block == body).count() != 1
            {
                return Err(bad());
            }
            transport::from_anchor(lowerer.function, seed, self.anchor, span)?;
        }
        Ok(())
    }

    pub(super) fn active_body(&self) -> BlockId {
        self.resumed.map_or(self.body, |roles| roles.block)
    }

    fn current_entities(
        &self,
        function: &crate::ssa::model::Function,
        block: BlockId,
        span: Span,
    ) -> Result<[EntityId; 3], LoweringError> {
        if let Some(roles) = self.resumed.filter(|roles| roles.block == block) {
            return Ok([
                roles.caller,
                EntityId::Loan(roles.source),
                EntityId::Value(roles.cursor),
            ]);
        }
        Ok([
            self.parameter(function, block, self.seed_column, span)?,
            self.parameter(function, block, self.source_column, span)?,
            self.parameter(function, block, self.cursor_column, span)?,
        ])
    }

    pub(super) fn validate_return(
        &self,
        lowerer: &UnitExpressionLowerer<'_>,
        span: Span,
    ) -> Result<(), LoweringError> {
        use lang_frontend::ownership_checking::UnitIterationCleanupAction;
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let UnitIterationExitKind::Return(expression) = self.body_exit else {
            return Err(bad());
        };
        let node = lowerer
            .parsed
            .ast()
            .expressions()
            .get(expression.expression())
            .map_err(|_| bad())?;
        let Expression::Return {
            value: Some(operand),
            ..
        } = node.payload()
        else {
            return Err(bad());
        };
        let operand_span = lowerer
            .parsed
            .ast()
            .expressions()
            .get(*operand)
            .map_err(|_| bad())?
            .span();
        let block = lowerer.function.block(self.body).ok_or_else(bad)?;
        let Some(TerminatorKind::Return { values }) =
            block.terminator.as_ref().map(|term| &term.kind)
        else {
            return Err(bad());
        };
        let [result] = values.as_slice() else {
            return Err(bad());
        };
        let call = instruction(lowerer.function, EntityId::Value(*result), span)?;
        let call_position = block
            .instructions
            .iter()
            .position(|id| *id == call.id)
            .ok_or_else(bad)?;
        if call.block != self.body
            || call.origin != Origin::Source(operand_span)
            || !matches!(call.operation, Operation::DirectCall { receiver: None, .. })
            || call.results.as_slice() != [EntityId::Value(*result)]
        {
            return Err(bad());
        }
        let actions = lowerer
            .owned
            .iteration_cleanup_at(self.body_point)
            .ok_or_else(bad)?;
        for action in actions {
            let UnitIterationCleanupAction::Drop(fact) = action else {
                continue;
            };
            let positions = block
                .instructions
                .iter()
                .enumerate()
                .filter_map(|(index, id)| {
                    let instruction = lowerer.function.instruction(*id)?;
                    (matches!(instruction.operation, Operation::Drop { .. })
                        && instruction.origin == Origin::Source(fact.value_origin()))
                    .then_some(index)
                })
                .collect::<Vec<_>>();
            let [drop_position] = positions.as_slice() else {
                return Err(bad());
            };
            if call_position >= *drop_position {
                return Err(bad());
            }
        }
        let mut component_ends = Vec::new();
        for id in &block.instructions {
            let projection = lowerer.function.instruction(*id).ok_or_else(bad)?;
            if !matches!(
                projection.operation,
                Operation::MapIterationKeyLoan { .. } | Operation::MapIterationValueLoan { .. }
            ) {
                continue;
            }
            let [EntityId::Loan(loan)] = projection.results.as_slice() else {
                return Err(bad());
            };
            let positions = block
                .instructions
                .iter()
                .enumerate()
                .filter_map(|(index, id)| {
                    (lowerer.function.instruction(*id)?.operation
                        == Operation::BorrowEnd { loan: *loan })
                    .then_some(index)
                })
                .collect::<Vec<_>>();
            let [position] = positions.as_slice() else {
                return Err(bad());
            };
            component_ends.push(*position);
        }
        if component_ends.len() != 2
            || component_ends
                .iter()
                .any(|position| *position <= call_position)
            || block.instructions.iter().any(|id| {
                lowerer
                    .function
                    .instruction(*id)
                    .is_some_and(|instruction| {
                        matches!(instruction.operation, Operation::MapIterationNext { .. })
                    })
            })
        {
            return Err(bad());
        }
        Ok(())
    }

    pub(super) fn terminal_block(
        &self,
        point: UnitDropPoint,
        span: Span,
    ) -> Result<BlockId, LoweringError> {
        if point == self.exhaustion_point {
            Ok(self.exhaustion)
        } else if point == self.body_point
            && matches!(
                self.body_exit,
                UnitIterationExitKind::Break(_) | UnitIterationExitKind::Return(_)
            )
        {
            Ok(self.active_body())
        } else if self.nested_return_point() == Some(point) {
            Ok(self.active_body())
        } else {
            Err(lowering_error(LoweringErrorKind::MissingFact, span))
        }
    }

    fn nested_return_point(&self) -> Option<UnitDropPoint> {
        match self.nested?.tail?.kind {
            UnitIterationExitKind::Return(expression) => {
                Some(UnitDropPoint::ControlTransfer(expression))
            }
            _ => None,
        }
    }

    pub(super) fn header_entities(
        &self,
        function: &crate::ssa::model::Function,
        span: Span,
    ) -> Result<[EntityId; 3], LoweringError> {
        Ok([
            self.parameter(function, self.header, self.seed_column, span)?,
            self.parameter(function, self.header, self.source_column, span)?,
            self.parameter(function, self.header, self.cursor_column, span)?,
        ])
    }

    pub(super) fn is_body_point(&self, point: UnitDropPoint) -> bool {
        point == self.body_point
            || self.nested_return_point() == Some(point)
            || self.conditional.is_some_and(|shape| {
                shape.transfers.iter().any(|kind| match kind {
                    UnitIterationExitKind::Continue(expression)
                    | UnitIterationExitKind::Break(expression)
                    | UnitIterationExitKind::Return(expression) => {
                        point == UnitDropPoint::ControlTransfer(*expression)
                    }
                    _ => false,
                })
            })
    }

    pub(super) fn drop_at(&self, point: UnitDropPoint) -> Option<UnitDropFact> {
        if let Some(shape) = self.conditional {
            for (kind, drop) in shape.transfers.into_iter().zip(shape.drops) {
                if let UnitIterationExitKind::Break(expression)
                | UnitIterationExitKind::Return(expression) = kind
                    && point == UnitDropPoint::ControlTransfer(expression)
                {
                    return drop;
                }
            }
        }
        if point == self.exhaustion_point {
            self.drop
        } else if point == self.body_point {
            self.transfer_drop
        } else {
            None
        }
    }

    pub(super) fn terminal_owner(
        &self,
        lowerer: &UnitExpressionLowerer<'_>,
        point: UnitDropPoint,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let block = self.terminal_block(point, span)?;
        let EntityId::Value(owner) =
            self.parameter(lowerer.function, block, self.seed_column, span)?
        else {
            return Err(bad());
        };
        if lowerer.block != block || self.identity.current(lowerer, span)? != EntityId::Value(owner)
        {
            return Err(bad());
        }
        Ok(owner)
    }
}

pub(super) fn instruction(
    function: &crate::ssa::model::Function,
    result: EntityId,
    span: Span,
) -> Result<&Instruction, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let Some(Definition::InstructionResult {
        instruction,
        index: 0,
    }) = function.entity(result).map(|entity| entity.definition)
    else {
        return Err(bad());
    };
    let instruction = function.instruction(instruction).ok_or_else(bad)?;
    if instruction.results.as_slice() != [result] {
        return Err(bad());
    }
    Ok(instruction)
}
