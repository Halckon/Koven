//! Audit completed FE countdown identities against real checked edges; no new permission.
use super::*;
mod boundary;
mod emission;
mod entry;
mod exit;
mod plans;
mod retirement;
use crate::ssa::model::{CheckedArithmeticOperator, ComparisonOperator, ScalarConstant};
use lang_frontend::ownership_checking::{
    UnitDropTarget, UnitOwnerCallable, UnitOwnerCounterTailEvidenceV1,
};
use plans::Key;
pub(super) use plans::Plans;

pub(super) struct Plan<'a> {
    evidence: &'a UnitOwnerCounterTailEvidenceV1,
    geometry: Option<[BlockRef; 5]>,
    counter: Option<Slot>,
    guard: Option<Slot>,
    roots: Vec<Slot>,
    chain: Vec<retirement::Stage>,
    initial: Vec<Option<Slot>>,
    exits: Option<Vec<exit::Item>>,
}
impl<'a> Plan<'a> {
    fn from_evidence(evidence: &'a UnitOwnerCounterTailEvidenceV1) -> Self {
        Self {
            evidence,
            geometry: None,
            counter: None,
            guard: None,
            roots: Vec::new(),
            chain: vec![retirement::Stage::Ready; evidence.steps().len()],
            initial: vec![None; evidence.root_slots().len()],
            exits: exit::items(evidence),
        }
    }
}
impl<'a> Builder<'_, 'a> {
    pub(super) fn counter_tail_while(
        &mut self,
        statement: UnitStatementId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        blocks: [BlockRef; 4],
    ) -> Result<(), LoweringError> {
        let Some(key) = self
            .counter_tail_plan
            .select(|p| p.evidence.statement() == statement, self.inputs.span)?
        else {
            return Ok(());
        };
        let plan = self.counter_tail_plan.get_mut(key, self.inputs.span)?;
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
        if plan
            .evidence
            .root_slots()
            .iter()
            .zip(&plan.initial)
            .any(|(slot, actual)| {
                self.bindings.get(&slot.root()).copied() != *actual || actual.is_none()
            })
        {
            return Err(fail());
        }
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
        self.counter_tail_plan
            .validate_batch_layout(self.inputs.span)?;
        let keys = self.counter_tail_plan.keys();
        let mut pending = Vec::new();
        for key in keys {
            let complete = self.check_counter_tail_plan(key)?;
            if self.counter_tail_plan.is_batch() && complete.is_none() {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            pending.push((key, complete));
        }
        // Validate every identity and checked edge again before any final receipt commits.
        for (key, complete) in &pending {
            let plan = self.counter_tail_plan.get(*key, self.inputs.span)?;
            if let Some(complete) = complete {
                complete.validate(plan, self.inputs.span)?;
            }
            let arithmetic = plan.evidence.assignments()[1];
            let receipts: Vec<_> = self
                .checked_edge_receipts
                .iter()
                .filter(|r| r.expression == arithmetic)
                .collect();
            if receipts.len() != 1
                || !receipts[0].matches(&self.body, self.span(arithmetic.expression()))
            {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
        }
        for (key, complete) in pending {
            let plan = self.counter_tail_plan.get(key, self.inputs.span)?;
            if let Some(complete) = complete {
                complete.commit(plan, self.inputs.span)?;
            }
            #[cfg(test)]
            owner_consumer_test_support::record_action(
                "counter-tail-owner-audit",
                plan.roots.len(),
            );
        }
        #[cfg(test)]
        if self.counter_tail_plan.is_batch() {
            owner_consumer_test_support::record_action(
                "counter-batch-complete-audit",
                self.counter_tail_plan.keys().len(),
            );
        }
        Ok(())
    }
    fn check_counter_tail_plan(
        &mut self,
        key: Key,
    ) -> Result<Option<boundary::Complete<'a>>, LoweringError> {
        let plan = self.counter_tail_plan.get(key, self.inputs.span)?;
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        if plan
            .chain
            .iter()
            .any(|stage| *stage != retirement::Stage::Dropped)
        {
            return Err(fail());
        }
        if plan.exits.as_ref().is_some_and(|items| {
            items
                .iter()
                .any(|item| item.stage != retirement::Stage::Dropped)
        }) {
            return Err(fail());
        }
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
        let complete = self.counter_tail_boundary(plan)?;
        // Do not publish the complete receipt until checked success/abort remains
        // valid at the same finalization point as entry, body and false-exit checks.
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::CounterLateAbortToHeader,
        ) || (self.counter_tail_plan.is_batch()
            && key.ordinal() == 1
            && owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::CounterBatchSecondLateAbort,
            ))
        {
            self.body.blocks[_abort.0].terminator = Some((RTerm::Goto(header), self.inputs.span));
        }
        if !edge.matches(&self.body, self.span(arithmetic.expression())) {
            return Err(fail());
        }
        Ok(complete)
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
