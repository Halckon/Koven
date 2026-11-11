//! Audit completed FE countdown identities against real checked edges; no new permission.
use super::*;
mod boundary;
use crate::ssa::model::{CheckedArithmeticOperator, ComparisonOperator, ScalarConstant};
use lang_frontend::ownership_checking::{
    UnitDropTarget, UnitOwnerCallable, UnitOwnerCounterTailEvidenceV1,
};

pub(super) struct Plan<'a> {
    evidence: &'a UnitOwnerCounterTailEvidenceV1,
    geometry: Option<[BlockRef; 5]>,
    counter: Option<Slot>,
    guard: Option<Slot>,
    roots: Vec<Slot>,
}
impl<'a> Plan<'a> {
    pub(super) fn new(inputs: &RefineInputs<'a>) -> Result<Option<Self>, LoweringError> {
        let Some(item) = inputs.function_item else {
            return Ok(None);
        };
        let callable = UnitOwnerCallable::Function(UnitItemId::new(inputs.source_unit, item));
        let mut candidates = inputs
            .owned
            .owner_observations()
            .iter()
            .filter(|o| o.callable() == callable)
            .filter_map(|o| o.counter_tail_evidence_v1());
        let Some(evidence) = candidates.next() else {
            return Ok(None);
        };
        if candidates.next().is_some() {
            return Err(error(LoweringErrorKind::InvalidModel, inputs.span));
        }
        Ok(Some(Self {
            evidence,
            geometry: None,
            counter: None,
            guard: None,
            roots: Vec::new(),
        }))
    }
}
impl Builder<'_, '_> {
    pub(super) fn counter_tail_while(
        &mut self,
        statement: UnitStatementId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        blocks: [BlockRef; 4],
    ) -> Result<(), LoweringError> {
        let Some(plan) = &mut self.counter_tail_plan else {
            return Ok(());
        };
        if statement != plan.evidence.statement() {
            return Ok(());
        }
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        if condition != plan.evidence.condition()
            || body != plan.evidence.body()
            || plan.geometry.is_some()
        {
            return Err(fail());
        }
        plan.counter = self.bindings.get(&plan.evidence.counter()).copied();
        plan.guard = self.bindings.get(&plan.evidence.guard()).copied();
        plan.roots = plan
            .evidence
            .steps()
            .iter()
            .map(|s| {
                self.bindings
                    .get(&s.header_root())
                    .copied()
                    .ok_or_else(fail)
            })
            .collect::<Result<_, _>>()?;
        plan.geometry = Some([
            self.block.ok_or_else(fail)?,
            blocks[0],
            blocks[1],
            blocks[2],
            blocks[3],
        ]);
        Ok(())
    }
    pub(super) fn finish_counter_tail_receipt(&mut self) -> Result<(), LoweringError> {
        let Some(plan) = &self.counter_tail_plan else {
            return Ok(());
        };
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let [entry, header, body, finished, _exit] = plan.geometry.ok_or_else(fail)?;
        let counter = plan.counter.ok_or_else(fail)?;
        let guard = plan.guard.ok_or_else(fail)?;
        let [decrement, arithmetic, reset, comparison] = plan.evidence.assignments();
        let mut edges = self
            .checked_edge_receipts
            .iter()
            .filter(|r| r.expression == arithmetic);
        let edge = edges.next().ok_or_else(fail)?;
        let [actual_body, success, _abort] = edge.blocks;
        let [result, _, left, right] = edge.values;
        if edges.next().is_some()
            || actual_body != body
            || edge.operator != CheckedArithmeticOperator::Subtract
            || left != counter
            || counter == guard
            || plan.roots.contains(&counter)
            || plan.roots.contains(&guard)
            || plan
                .roots
                .iter()
                .enumerate()
                .any(|(i, r)| plan.roots[..i].contains(r))
            || !self.counter_integer(body, edge.instruction, right, 1)
            || !matches!(self.body.blocks[entry.0].terminator, Some((RTerm::Goto(t),_)) if t == header)
            || !matches!(self.body.blocks[success.0].terminator, Some((RTerm::Goto(t),_)) if t == header)
            || !matches!(self.body.blocks[header.0].terminator, Some((RTerm::Branch {condition,when_true,when_false},_)) if condition == guard && when_true == body && when_false == finished)
        {
            return Err(fail());
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
        if predecessors(header) != [entry, success]
            || predecessors(body) != [header]
            || predecessors(finished) != [header]
        {
            return Err(fail());
        }
        // The generic receipt already checks the real failure branch, terminal abort,
        // success/abort predecessors, unique arithmetic outputs, and exact instruction.
        #[cfg(test)]
        {
            use owner_consumer_test_support::{OwnerFault, take_fault};
            if take_fault(OwnerFault::CounterTailLateInitialization) {
                let statements = &mut self.body.blocks[entry.0].statements;
                let initial = statements
                    .iter()
                    .position(|s| s.results.contains(&counter))
                    .unwrap();
                let compare = statements
                    .iter()
                    .position(|s| matches!(s.op, ROp::Compare { left, .. } if left == counter))
                    .unwrap();
                statements.swap(initial, compare);
            }
            if take_fault(OwnerFault::CounterTailWrongBackedge) {
                self.body.blocks[success.0].terminator =
                    Some((RTerm::Goto(_abort), self.inputs.span));
            }
            for (fault, id, source) in [
                (OwnerFault::CounterTailWrongCounterWrite, decrement, guard),
                (OwnerFault::CounterTailWrongGuardWrite, reset, counter),
            ] {
                if take_fault(fault) {
                    let span = self.span(id.expression());
                    for s in &mut self.body.blocks[success.0].statements {
                        if s.span == span && matches!(s.op, ROp::Alias { .. }) {
                            s.op = ROp::Alias { source };
                        }
                    }
                }
            }
            if take_fault(OwnerFault::CounterTailStaleComparison) {
                for s in &mut self.body.blocks[success.0].statements {
                    if let ROp::Compare { left, right, .. } = &mut s.op {
                        *left = *right;
                    }
                }
            }
        }
        if !matches!(self.body.blocks[success.0].terminator, Some((RTerm::Goto(t),_)) if t == header)
        {
            return Err(fail());
        }
        let success_statements = &self.body.blocks[success.0].statements;
        let counter_write = self.counter_alias(success, counter, result, Some(decrement))?;
        let guard_write =
            self.counter_guard_comparison(success, counter, guard, comparison, Some(reset))?;
        if counter_write >= guard_write
            || guard_write + 1 != success_statements.len()
            || success_statements.len() != 4
            || self.body.blocks[entry.0]
                .statements
                .iter()
                .filter(|s| s.results.contains(&counter))
                .count()
                != 1
            || self.body.blocks[body.0]
                .statements
                .iter()
                .any(|s| s.results.contains(&counter) || s.results.contains(&guard))
        {
            return Err(fail());
        }
        let compare_index = success_statements
            .iter()
            .position(|s| {
                s.span == self.span(comparison.expression()) && matches!(s.op, ROp::Compare { .. })
            })
            .ok_or_else(fail)?;
        if counter_write >= compare_index {
            return Err(fail());
        }
        self.counter_guard_comparison(entry, counter, guard, plan.evidence.initializer(), None)?;
        // Initial value must exist before the comparison. Counting definitions alone
        // cannot establish the first header's scalar dependency.
        let initial = self.body.blocks[entry.0]
            .statements
            .iter()
            .position(|s| s.results.contains(&counter))
            .ok_or_else(fail)?;
        let comparison = self.body.blocks[entry.0]
            .statements
            .iter()
            .position(|s| {
                s.span == self.span(plan.evidence.initializer().expression())
                    && matches!(s.op, ROp::Compare { .. })
            })
            .ok_or_else(fail)?;
        if initial >= comparison {
            return Err(fail());
        }
        let statements = &self.body.blocks[body.0].statements;
        if statements
            .iter()
            .filter(|s| matches!(s.op, ROp::RootReplace { .. }))
            .count()
            != plan.roots.len()
            || self.body.blocks[header.0].statements.iter().any(|s| {
                s.results.contains(&counter)
                    || s.results.contains(&guard)
                    || plan
                        .roots
                        .iter()
                        .any(|r| s.results.contains(r) || s.op.operands().contains(r))
            })
        {
            return Err(fail());
        }
        let mut previous = None;
        for (step, root) in plan.evidence.steps().iter().zip(&plan.roots) {
            let producer = self.multi_root_literal(
                plan.evidence
                    .body_catalog()
                    .definition(step.producer())
                    .ok_or_else(fail)?
                    .origin(),
                body,
            )?;
            let matching: Vec<_> = statements.iter().enumerate().filter(|(_,s)| s.span == self.span(step.call().expression()) && matches!(s.op,ROp::RootReplace {owner,replacement,..} if owner == *root && replacement == producer)).collect();
            let [(replace_index, replace)] = matching.as_slice() else {
                return Err(fail());
            };
            if replace.results.len() != 2 || replace.results[0] != *root {
                return Err(fail());
            }
            let old = replace.results[1];
            let fact = step.returned_value_drop();
            let UnitDropTarget::Named(binding) = fact.target() else {
                return Err(fail());
            };
            let detached = *self.bindings.get(&binding).ok_or_else(fail)?;
            let alias_index = self.counter_alias(body, detached, old, None)?;
            let drop_index = self.multi_root_drop(fact, body, detached)?;
            if previous.is_some_and(|index| index >= *replace_index)
                || *replace_index >= alias_index
                || alias_index >= drop_index
                || drop_index >= edge.instruction
                || old == *root
                || detached == *root
                || self.multi_root_definitions(old) != 1
                || self.multi_root_definitions(detached) != 1
                || statements
                    .iter()
                    .filter(|s| s.results.contains(root))
                    .count()
                    != 1
                || success_statements.iter().any(|s| {
                    [*root, old, detached]
                        .iter()
                        .any(|slot| s.results.contains(slot) || s.op.operands().contains(slot))
                })
            {
                return Err(fail());
            }
            previous = Some(drop_index);
        }
        self.counter_tail_boundary(plan)?;
        #[cfg(test)]
        owner_consumer_test_support::record_action("counter-tail-owner-audit", plan.roots.len());
        Ok(())
    }
    fn counter_alias(
        &self,
        block: BlockRef,
        target: Slot,
        source: Slot,
        id: Option<UnitExpressionId>,
    ) -> Result<usize, LoweringError> {
        let matches: Vec<_> = self.body.blocks[block.0]
            .statements
            .iter()
            .enumerate()
            .filter(|(_, s)| s.results.contains(&target))
            .collect();
        let [(index, s)] = matches.as_slice() else {
            return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
        };
        if s.results != [target]
            || !matches!(s.op,ROp::Alias {source: actual} if actual == source)
            || id.is_some_and(|id| s.span != self.span(id.expression()))
        {
            return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
        }
        Ok(*index)
    }
    fn counter_integer(&self, block: BlockRef, before: usize, slot: Slot, value: i128) -> bool {
        self.multi_root_definitions(slot) == 1 && self.body.blocks[block.0].statements[..before].iter().any(|s| s.results == [slot] && matches!(s.op, ROp::Constant(ScalarConstant::Integer(actual)) if actual == value))
    }
    fn counter_guard_comparison(
        &self,
        block: BlockRef,
        counter: Slot,
        guard: Slot,
        id: UnitExpressionId,
        assignment: Option<UnitExpressionId>,
    ) -> Result<usize, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let matches: Vec<_> = self.body.blocks[block.0]
            .statements
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.span == self.span(id.expression()) && matches!(s.op, ROp::Compare { .. })
            })
            .collect();
        let [(index, s)] = matches.as_slice() else {
            return Err(fail());
        };
        let ROp::Compare {
            operator: ComparisonOperator::GreaterThan,
            left,
            right,
        } = s.op
        else {
            return Err(fail());
        };
        let [result] = s.results.as_slice() else {
            return Err(fail());
        };
        if left != counter
            || !self.counter_integer(block, *index, right, 0)
            || self.multi_root_definitions(*result) != 1
        {
            return Err(fail());
        }
        let write = self.counter_alias(block, guard, *result, assignment)?;
        if *index >= write {
            return Err(fail());
        }
        Ok(write)
    }
}
