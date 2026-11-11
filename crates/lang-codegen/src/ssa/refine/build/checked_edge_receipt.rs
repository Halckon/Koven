//! Actual checked-expression CFG receipts, independent of any owner authorization.
//! This records emission identity; it does not authorize a loop/header or prove termination.
use super::super::body::RAbort;
use super::*;
use crate::ssa::model::CheckedArithmeticOperator;

pub(super) struct Receipt {
    expression: UnitExpressionId,
    operator: CheckedArithmeticOperator,
    blocks: [BlockRef; 3],
    values: [Slot; 4],
    instruction: usize,
}
impl Builder<'_, '_> {
    // Preserve the legacy hook at this same emission point without selecting its plan.
    pub(super) fn checked_expression_emitted(
        &mut self,
        expression: UnitExpressionId,
        blocks: [BlockRef; 3],
        values: [Slot; 4],
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(ROp::CheckedArithmetic { operator, .. }) = self
            .body
            .blocks
            .get(blocks[0].0)
            .and_then(|block| block.statements.last())
            .map(|statement| &statement.op)
        else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let operator = *operator;
        self.checked_expression_edges += 1;
        self.checked_expression_edge_emitted(expression, operator, blocks, values, span)?;
        self.header_increment_emitted(expression, blocks, values, span)
    }
    fn checked_expression_edge_emitted(
        &mut self,
        expression: UnitExpressionId,
        operator: CheckedArithmeticOperator,
        blocks: [BlockRef; 3],
        values: [Slot; 4],
        span: Span,
    ) -> Result<(), LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let [body, success, abort] = blocks;
        if body == success
            || body == abort
            || success == abort
            || self.block != Some(success)
            || !self.body.blocks[success.0].statements.is_empty()
            || self.body.blocks[success.0].terminator.is_some()
        {
            return Err(fail());
        }
        let instruction = self.body.blocks[body.0]
            .statements
            .len()
            .checked_sub(1)
            .ok_or_else(fail)?;
        let receipt = Receipt {
            expression,
            operator,
            blocks,
            values,
            instruction,
        };
        if !receipt.matches(&self.body, span) {
            return Err(fail());
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::MissingCheckedExpressionEdge,
        ) {
            return Ok(());
        }
        #[cfg(test)]
        let receipt = {
            let mut receipt = receipt;
            if owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::WrongCheckedExpressionEdgeOperand,
            ) {
                receipt.values[3] = receipt.values[1];
            }
            if owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::ReversedCheckedExpressionEdges,
            ) {
                receipt.blocks.swap(1, 2);
            }
            receipt
        };
        self.checked_edge_receipts.push(receipt);
        Ok(())
    }
    pub(super) fn finish_checked_edge_receipts(&self) -> Result<(), LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        if self.checked_expression_edges != self.checked_edge_receipts.len() {
            return Err(fail());
        }
        if self.checked_edge_receipts.is_empty() {
            return Ok(());
        }
        let mut predecessors: BTreeMap<BlockRef, Vec<BlockRef>> = BTreeMap::new();
        let mut definitions: BTreeMap<Slot, usize> = BTreeMap::new();
        for (index, block) in self.body.blocks.iter().enumerate() {
            for statement in &block.statements {
                for result in &statement.results {
                    *definitions.entry(*result).or_default() += 1;
                }
            }
            if let Some((term, _)) = &block.terminator {
                for target in term.successors() {
                    predecessors
                        .entry(target)
                        .or_default()
                        .push(BlockRef(index));
                }
            }
        }
        let mut seen = BTreeSet::new();
        for receipt in &self.checked_edge_receipts {
            let [body, success, abort] = receipt.blocks;
            let [result, failed, _, _] = receipt.values;
            if !seen.insert((body, receipt.instruction))
                || !receipt.matches(&self.body, self.span(receipt.expression.expression()))
                || predecessors.get(&success).map(Vec::as_slice) != Some(&[body])
                || predecessors.get(&abort).map(Vec::as_slice) != Some(&[body])
                || definitions.get(&result) != Some(&1)
                || definitions.get(&failed) != Some(&1)
            {
                return Err(fail());
            }
            #[cfg(test)]
            owner_consumer_test_support::record_action(
                "checked-expression-edge-receipt",
                receipt.expression.expression().index(),
            );
        }
        Ok(())
    }
}
impl Receipt {
    fn matches(&self, cfg: &RefinedBody, span: Span) -> bool {
        let [body, success, abort] = self.blocks;
        let [result, failed, left, right] = self.values;
        let Some(block) = cfg.blocks.get(body.0) else {
            return false;
        };
        let Some(abort_block) = cfg.blocks.get(abort.0) else {
            return false;
        };
        let Some(statement) = block.statements.get(self.instruction) else {
            return false;
        };
        result != failed
            && ![left, right].contains(&result)
            && ![left, right].contains(&failed)
            && self.instruction + 1 == block.statements.len()
            && statement.results == [result, failed]
            && statement.span == span
            && matches!(statement.op, ROp::CheckedArithmetic { operator, left: l, right: r } if operator == self.operator && l == left && r == right)
            && matches!(block.terminator, Some((RTerm::Branch { condition, when_true, when_false }, _)) if condition == failed && when_true == abort && when_false == success)
            && abort_block.statements.is_empty()
            && matches!(abort_block.terminator, Some((RTerm::Abort(RAbort::CheckedArithmetic { operator, divisor }), _)) if operator == self.operator && divisor == right)
    }
}
