//! Exact source-parameter and provider transport; no same-type or same-name owner substitution.
use super::*;
use crate::ssa::model::{Definition, Edge, Instruction};
use lang_frontend::ownership_checking::UnitDropFact;

#[derive(Clone, Copy)]
pub(super) struct MapCarrier {
    pub(super) root: UnitSymbolId,
    pub(super) drop: Option<UnitDropFact>,
    header: BlockId,
    pub(super) body: BlockId,
    pub(super) exhaustion: BlockId,
    seed_column: usize,
    source_column: usize,
    cursor_column: usize,
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
        let entry = function.entry_block().ok_or_else(bad)?;
        if function
            .block(entry)
            .and_then(|block| block.parameters.get(qualified.parameter_slot))
            .copied()
            != Some(seed)
            || function.entity(seed).map(|entity| entity.definition)
                != Some(Definition::BlockParameter {
                    block: entry,
                    index: qualified.parameter_slot,
                })
            || edge.target != header
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
            root: qualified.root,
            drop: qualified.source_drop,
            header,
            body,
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
        let seed = self.parameter(lowerer.function, block, self.seed_column, span)?;
        let source = self.parameter(lowerer.function, block, self.source_column, span)?;
        let cursor = self.parameter(lowerer.function, block, self.cursor_column, span)?;
        if lowerer.pending_operands.get(source_slot) != Some(&source)
            || lowerer.pending_operands.get(cursor_slot) != Some(&cursor)
        {
            return Err(bad());
        }
        match (seed, self.drop) {
            (EntityId::Value(owner), Some(_))
                if lowerer.bindings.get(&self.root) == Some(&LoweredValue::Value(owner)) => {}
            (EntityId::Loan(loan), None)
                if lowerer.borrow_bindings.get(&self.root) == Some(&loan) => {}
            _ => return Err(bad()),
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
        let Some(TerminatorKind::Branch(edge)) = lowerer
            .function
            .block(self.body)
            .and_then(|block| block.terminator.as_ref())
            .map(|term| &term.kind)
        else {
            return Err(bad());
        };
        if edge.target != self.header {
            return Err(bad());
        }
        for column in [self.seed_column, self.source_column] {
            if edge.arguments.get(column).copied()
                != Some(self.parameter(lowerer.function, self.body, column, span)?)
            {
                return Err(bad());
            }
        }
        let source = self.parameter(lowerer.function, self.body, self.source_column, span)?;
        let cursor = self.parameter(lowerer.function, self.body, self.cursor_column, span)?;
        let (EntityId::Loan(source), EntityId::Value(cursor)) = (source, cursor) else {
            return Err(bad());
        };
        let next = edge
            .arguments
            .get(self.cursor_column)
            .copied()
            .ok_or_else(bad)?;
        let advance = instruction(lowerer.function, next, span)?;
        if advance.block != self.body
            || advance.operation != (Operation::MapIterationNext { source, cursor })
            || lowerer.pending_operands.get(source_slot) != Some(&EntityId::Loan(source))
            || lowerer.pending_operands.get(cursor_slot) != Some(&next)
        {
            return Err(bad());
        }
        Ok(())
    }

    pub(super) fn exhaustion_owner(
        &self,
        lowerer: &UnitExpressionLowerer<'_>,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let EntityId::Value(owner) =
            self.parameter(lowerer.function, self.exhaustion, self.seed_column, span)?
        else {
            return Err(bad());
        };
        if lowerer.block != self.exhaustion
            || lowerer.bindings.get(&self.root) != Some(&LoweredValue::Value(owner))
        {
            return Err(bad());
        }
        Ok(owner)
    }
}

fn instruction(
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
