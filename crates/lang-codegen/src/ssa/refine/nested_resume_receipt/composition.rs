//! Compose already-finished local receipts into a bounded parent recurrence.
//! This is a symbolic transition proof, not a finite-iteration execution check.
use super::*;
use crate::ssa::model::{AbortReason, Definition, Edge, EntityType, LoanKind};
use std::collections::BTreeSet;
impl Emitted {
    // Caller must finish child-cycle, parent false partition and resume receipts
    // first. This method adds cross-receipt identity and whole-vector closure.
    pub(in super::super) fn compose(
        &self,
        body: &RefinedBody,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        let fail = || invalid(span);
        let required = self.composition_required.ok_or_else(fail)?;
        if required
            != body
                .nested_body_plans
                .0
                .as_ref()
                .is_some_and(|p| p.parent_composition_required)
            || required
                != body
                    .nested_resume_plans
                    .0
                    .as_ref()
                    .is_some_and(|p| p.composition_selected)
            || required != selected(body)
        {
            return Err(fail());
        }
        if !required {
            return Ok(());
        }
        let child = body.child_cycle_plans.0.as_ref().ok_or_else(fail)?;
        let initial = child.initial_base.ok_or_else(fail)?;
        let p = body.nested_resume_plans.0.as_ref().ok_or_else(fail)?;
        let c = p.carry.as_ref().ok_or_else(fail)?;
        let parent = body.nested_exit_plans.parent.as_ref().ok_or_else(fail)?;
        if !body.nested_exit_plans.parent_required
            || !body
                .nested_body_plans
                .0
                .as_ref()
                .is_some_and(|p| p.initial_base_required)
            || p.child != child.control
            || p.parent != parent.control
            || p.header != child.header
            || p.finished != child.exit
            || c.header != child.parent_header
            || parent.header != c.header
            || parent.roots != child.roots
            || !child.roots.contains(&p.root)
        {
            return Err(fail());
        }
        let (replace, owner, rhs, new, _) = self.replace.ok_or_else(fail)?;
        // The parent replacement is a fresh literal activation, never a stale
        // child/parent owner retained from a previous iteration.
        let Definition::InstructionResult {
            instruction: producer,
            index: 0,
        } = function.entity(rhs).ok_or_else(fail)?.definition
        else {
            return Err(fail());
        };
        let producer_op = function.instruction(producer).ok_or_else(fail)?;
        let resume = function.block(self.blocks[&p.resume]).ok_or_else(fail)?;
        if producer_op.block != resume.id
            || producer_op.results != [rhs]
            || !matches!(producer_op.operation, Operation::StringLiteral { .. })
            || resume
                .instructions
                .iter()
                .position(|id| *id == producer)
                .ok_or_else(fail)?
                >= resume
                    .instructions
                    .iter()
                    .position(|id| *id == replace)
                    .ok_or_else(fail)?
            || [owner, rhs, new].into_iter().collect::<BTreeSet<_>>().len() != 3
        {
            return Err(fail());
        }
        super::composition_loan::verify(function, replace, owner, span)?;
        let mut preparations = 0;
        for i in &function.instructions {
            let owner_uses = i
                .operation
                .entities()
                .iter()
                .filter(|e| **e == owner)
                .count();
            if owner_uses != 0 {
                if matches!(i.operation, Operation::RootPlace { owner: value } if EntityId::Value(value) == owner)
                {
                    preparations += 1;
                    if i.block != resume.id || owner_uses != 1 {
                        return Err(fail());
                    }
                } else if i.id != replace || owner_uses != 1 {
                    return Err(fail());
                }
            }
            if i.results.contains(&owner) || i.operation.entities().contains(&new) {
                return Err(fail());
            }
            if i.operation.entities().iter().filter(|e| **e == rhs).count()
                != usize::from(i.id == replace)
                || i.results.contains(&rhs) != (i.id == producer)
                || i.results.contains(&new) != (i.id == replace)
            {
                return Err(fail());
            }
        }
        if preparations != 1 {
            return Err(fail());
        }
        let mut edges = Vec::new();
        if p.finished != p.resume {
            edges.push((p.finished, p.resume));
        }
        edges.extend(c.edges.iter().copied());
        // Existing shared Int inputs are not owner-root loans. They can remain
        // as unchanged parameter slots (e.g. inner's initial counter). No newly
        // created loan or String-root place/loan may cross the induction header.
        let checked = body
            .blocks
            .get(child.body.0)
            .and_then(|b| b.statements.get(child.checked_instruction))
            .ok_or_else(fail)?;
        let integer = body
            .slots
            .get(checked.results.first().ok_or_else(fail)?.0)
            .copied()
            .ok_or_else(fail)?;
        let EntityType::Value(integer) = integer else {
            return Err(fail());
        };
        let slots = self.slots.get(&c.header).ok_or_else(fail)?;
        let parameters = self.parameters.get(&c.header).ok_or_else(fail)?;
        if slots.len() != parameters.len() {
            return Err(fail());
        }
        for (slot, entity) in slots.iter().zip(parameters) {
            match entity {
                EntityId::Value(_) => {}
                EntityId::Loan(_)
                    if body.parameters.contains(slot)
                        && body.slots.get(slot.0)
                            == Some(&EntityType::Loan {
                                kind: LoanKind::Shared,
                                target: integer,
                            })
                        && function.entity(*entity).is_some_and(|e| {
                            e.ty == EntityType::Loan {
                                kind: LoanKind::Shared,
                                target: integer,
                            }
                        }) =>
                {
                    if body
                        .blocks
                        .iter()
                        .any(|b| b.statements.iter().any(|s| s.results.contains(slot)))
                    {
                        return Err(fail());
                    }
                    for (role, current) in &self.current {
                        if self
                            .slots
                            .get(role)
                            .is_some_and(|slots| slots.contains(slot))
                            && current.get(slot)
                                != Some(&self.composition_parameter(*role, *slot, span)?)
                        {
                            return Err(fail());
                        }
                    }
                }
                _ => return Err(fail()),
            }
        }
        let mut at = child.exit;
        let mut visited = BTreeSet::new();
        for &(from, to) in &edges {
            if from != at || !visited.insert(from) || from == c.header {
                return Err(fail());
            }
            let block = function.block(self.blocks[&from]).ok_or_else(fail)?;
            let term = &block.terminator.as_ref().ok_or_else(fail)?.kind;
            let edge = match term {
                TerminatorKind::Branch(edge)
                    if !c.checked.iter().any(|check| check.body == from) =>
                {
                    edge
                }
                TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } => {
                    let check = c
                        .checked
                        .iter()
                        .find(|check| check.body == from)
                        .ok_or_else(fail)?;
                    if check.success != to || when_true.target != self.blocks[&check.abort] {
                        return Err(fail());
                    }
                    self.composition_edge(function, from, check.abort, when_true, span)?;
                    let abort = function.block(when_true.target).ok_or_else(fail)?;
                    let terminal = abort.terminator.as_ref().ok_or_else(fail)?;
                    let (_, op) = self.checked.get(&from).ok_or_else(fail)?;
                    let Operation::CheckedArithmetic {
                        operator, right, ..
                    } = op
                    else {
                        return Err(fail());
                    };
                    if !abort.instructions.is_empty()
                        || !matches!(terminal.kind, TerminatorKind::Abort)
                        || !matches!(terminal.abort,Some(AbortReason::CheckedArithmetic{operator:o,divisor}) if o==*operator && divisor==*right)
                    {
                        return Err(fail());
                    }
                    self.composition_predecessors(function, check.abort, &[from], span)?;
                    when_false
                }
                _ => return Err(fail()),
            };
            self.composition_edge(function, from, to, edge, span)?;
            if to != c.header {
                self.composition_predecessors(function, to, &[from], span)?;
            }
            for root in &child.roots {
                let current = *self
                    .current
                    .get(&from)
                    .and_then(|m| m.get(root))
                    .ok_or_else(fail)?;
                let expected = if from == p.resume && *root == p.root {
                    new
                } else {
                    self.composition_parameter(from, *root, span)?
                };
                if current != expected {
                    return Err(fail());
                }
                // Changed root input is checked by the existing replace/drop
                // receipt. All other roots must remain untouched parameters.
                if from != p.resume || *root != p.root {
                    for id in &block.instructions {
                        let i = function.instruction(*id).ok_or_else(fail)?;
                        if i.results.contains(&current) || i.operation.entities().contains(&current)
                        {
                            return Err(fail());
                        }
                    }
                }
                for outgoing in match term {
                    TerminatorKind::Branch(e) => vec![e],
                    TerminatorKind::Conditional {
                        when_true,
                        when_false,
                        ..
                    } => vec![when_true, when_false],
                    _ => return Err(fail()),
                } {
                    if outgoing.arguments.iter().filter(|e| **e == current).count() != 1 {
                        return Err(fail());
                    }
                }
            }
            at = to;
        }
        if at != c.header {
            return Err(fail());
        }
        let last = edges.last().ok_or_else(fail)?.0;
        self.composition_predecessors(function, c.header, &[initial, last], span)?;
        // rhs and new may not escape through an unrelated block or edge.
        for block in &function.blocks {
            let term = &block.terminator.as_ref().ok_or_else(fail)?.kind;
            if block.parameters.iter().filter(|e| **e == owner).count()
                != usize::from(block.id == resume.id)
                || block.parameters.contains(&rhs)
                || block.parameters.contains(&new)
                || term.entities().contains(&rhs)
                || term.entities().contains(&owner)
                || (block.id != resume.id && term.entities().contains(&new))
            {
                return Err(fail());
            }
        }
        #[cfg(test)]
        super::super::build::owner_consumer_test_support::record_action(
            "parent-generation-composition-ssa-complete",
            child.roots.len(),
        );
        Ok(())
    }
    fn composition_parameter(
        &self,
        block: BlockRef,
        root: Slot,
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        let fail = || invalid(span);
        let index = self
            .slots
            .get(&block)
            .ok_or_else(fail)?
            .iter()
            .position(|s| *s == root)
            .ok_or_else(fail)?;
        self.parameters
            .get(&block)
            .and_then(|p| p.get(index))
            .copied()
            .ok_or_else(fail)
    }
    fn composition_edge(
        &self,
        function: &Function,
        from: BlockRef,
        to: BlockRef,
        edge: &Edge,
        span: Span,
    ) -> Result<(), LoweringError> {
        let fail = || invalid(span);
        let current = self.current.get(&from).ok_or_else(fail)?;
        let expected = self
            .slots
            .get(&to)
            .ok_or_else(fail)?
            .iter()
            .map(|s| current.get(s).copied().ok_or_else(fail))
            .collect::<Result<Vec<_>, _>>()?;
        if edge.target != *self.blocks.get(&to).ok_or_else(fail)?
            || edge.arguments != expected
            || function
                .block(edge.target)
                .is_none_or(|b| Some(&b.parameters) != self.parameters.get(&to))
        {
            return Err(fail());
        }
        Ok(())
    }
    fn composition_predecessors(
        &self,
        function: &Function,
        target: BlockRef,
        allowed: &[BlockRef],
        span: Span,
    ) -> Result<(), LoweringError> {
        let fail = || invalid(span);
        let mut actual = Vec::new();
        for block in &function.blocks {
            for t in block.terminator.as_ref().ok_or_else(fail)?.kind.targets() {
                if t == self.blocks[&target] {
                    actual.push(block.id);
                }
            }
        }
        actual.sort();
        let mut expected = allowed.iter().map(|r| self.blocks[r]).collect::<Vec<_>>();
        expected.sort();
        if actual != expected {
            return Err(fail());
        }
        Ok(())
    }
}

// Called once before layout to seal eligibility, then rechecked rather than
// used as a late escape hatch. Only never-selected shapes may skip publication.
pub(super) fn selected(body: &RefinedBody) -> bool {
    let Some(child) = &body.child_cycle_plans.0 else {
        return false;
    };
    if child.initial_base.is_none() || body.nested_exit_plans.parent.is_none() {
        return false;
    }
    let Some(p) = &body.nested_resume_plans.0 else {
        return false;
    };
    let Some(c) = &p.carry else {
        return false;
    };
    c.checked.len() == 1
        && !c.edges.is_empty()
        && body.blocks.get(p.resume.0).is_some_and(|b| {
            b.statements
                .iter()
                .take(p.positions[0])
                .any(|s| s.results == [p.replacement] && matches!(s.op, ROp::StringLiteral { .. }))
        })
}
