//! Actual checked success/abort and scalar writeback for the selected bridge body.
use super::*;
use crate::ssa::model::CheckedArithmeticOperator;

impl Builder<'_, '_> {
    pub(super) fn check_bridge_counter_tail(&mut self) -> Result<(), LoweringError> {
        let Some(plan) = &self.control_bridge_plan else {
            return Ok(());
        };
        let Some(tail) = plan.evidence.next_counter_tail() else {
            return Ok(());
        };
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let (entry, [header, body, finished, _], roots) = plan.next.as_ref().ok_or_else(fail)?;
        let scalar = plan.scalar.ok_or_else(fail)?;
        let (sites, entry, blocks, roots) = (
            tail.assignments(),
            *entry,
            [
                *header,
                *body,
                *finished,
                plan.next.as_ref().ok_or_else(fail)?.1[3],
            ],
            roots.clone(),
        );
        self.check_control_counter_tail(sites, entry, blocks, &roots, scalar)
    }
    pub(super) fn check_control_counter_tail(
        &mut self,
        sites: [UnitExpressionId; 4],
        entry: BlockRef,
        blocks: [BlockRef; 4],
        roots: &[Slot],
        [counter, guard]: [Slot; 2],
    ) -> Result<(), LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let [header, body, finished, _] = &blocks;
        let [decrement, arithmetic, reset, comparison] = sites;
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::BridgeCounterMissingReceipt,
        ) {
            self.checked_edge_receipts
                .retain(|r| r.expression != arithmetic);
        }
        let mut receipts = self
            .checked_edge_receipts
            .iter()
            .filter(|r| r.expression == arithmetic);
        let edge = receipts.next().ok_or_else(fail)?;
        let [actual_body, success, abort] = edge.blocks;
        let [result, _, left, right] = edge.values;
        if receipts.next().is_some()
            || actual_body != *body
            || !edge.matches(&self.body, self.span(arithmetic.expression()))
            || edge.operator != CheckedArithmeticOperator::Subtract
            || left != counter
            || counter == guard
            || roots.contains(&counter)
            || roots.contains(&guard)
            || !self.counter_integer(*body, edge.instruction, right, 1)
        {
            return Err(fail());
        }
        #[cfg(test)]
        {
            use owner_consumer_test_support::{OwnerFault, take_fault};
            if take_fault(OwnerFault::BridgeCounterForeignSuccessPredecessor) {
                self.body.blocks[entry.0].terminator =
                    Some((RTerm::Goto(success), self.inputs.span));
            }
            if take_fault(OwnerFault::BridgeCounterAbortToHeader) {
                self.body.blocks[abort.0].terminator =
                    Some((RTerm::Goto(*header), self.inputs.span));
            }
            if take_fault(OwnerFault::BridgeCounterWrongBackedge) {
                self.body.blocks[success.0].terminator =
                    Some((RTerm::Goto(abort), self.inputs.span));
            }
            if take_fault(OwnerFault::BridgeCounterStaleComparison) {
                for statement in &mut self.body.blocks[success.0].statements {
                    if let ROp::Compare { left, right, .. } = &mut statement.op {
                        *left = *right;
                    }
                }
            }
            if take_fault(OwnerFault::BridgeCounterWrongWriteback) {
                for statement in &mut self.body.blocks[success.0].statements {
                    if statement.results == [counter] {
                        statement.op = ROp::Alias { source: guard };
                    }
                }
            }
        }
        let predecessors = |target| {
            self.body
                .blocks
                .iter()
                .enumerate()
                .filter_map(|(i, b)| {
                    b.terminator
                        .as_ref()
                        .filter(|(t, _)| t.successors().contains(&target))
                        .map(|_| BlockRef(i))
                })
                .collect::<Vec<_>>()
        };
        if !edge.matches(&self.body, self.span(arithmetic.expression()))
            || predecessors(success) != [*body]
            || predecessors(abort) != [*body]
            || predecessors(*header) != [entry, success]
            || !matches!(self.body.blocks[success.0].terminator, Some((RTerm::Goto(t),_)) if t == *header)
            || !matches!(self.body.blocks[header.0].terminator, Some((RTerm::Branch{condition,when_true,when_false},_)) if condition == guard && when_true == *body && when_false == *finished)
        {
            return Err(fail());
        }
        let write = self.counter_alias(success, counter, result, Some(decrement))?;
        let guard_write =
            self.counter_guard_comparison(success, counter, guard, comparison, Some(reset))?;
        let statements = &self.body.blocks[success.0].statements;
        let compare_index = statements
            .iter()
            .position(|s| {
                s.span == self.span(comparison.expression()) && matches!(s.op, ROp::Compare { .. })
            })
            .ok_or_else(fail)?;
        if write >= compare_index
            || compare_index >= guard_write
            || guard_write + 1 != statements.len()
            || statements.len() != 4
            || statements.iter().any(|s| {
                roots
                    .iter()
                    .any(|root| s.results.contains(root) || s.op.operands().contains(root))
            })
            || self.body.blocks[body.0]
                .statements
                .iter()
                .any(|s| s.results.contains(&counter) || s.results.contains(&guard))
            || self.body.blocks[header.0]
                .statements
                .iter()
                .any(|s| s.results.contains(&counter) || s.results.contains(&guard))
        {
            return Err(fail());
        }
        // This ties the normal body to actual scalar success. It is not an entry
        // initializer, false-exit retirement, or cross-iteration owner proof.
        Ok(())
    }
}
