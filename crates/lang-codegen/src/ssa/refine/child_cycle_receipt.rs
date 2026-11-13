//! Independent child cycle and bounded initial base, not a full parent induction.
#[cfg(test)]
mod faults;
mod initial_base;
mod initial_concat;
mod parent_entry;
use super::body::{BlockRef, RAbort, ROp, RTerm, RefinedBody, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{
        AbortReason, BlockId, CheckedArithmeticOperator, Edge, EntityId, Function, FunctionId,
        InstructionId, Operation, TerminatorKind,
    },
};
use lang_frontend::{source::Span, type_checking::UnitStatementId};
use std::collections::{BTreeMap, BTreeSet};
fn invalid(span: Span) -> LoweringError {
    error(LoweringErrorKind::InvalidModel, span)
}
#[derive(Clone, Debug)]
pub(super) struct Plan {
    pub control: UnitStatementId,
    pub parent_header: BlockRef,
    pub initial_base: Option<BlockRef>,
    pub entry: BlockRef,
    pub header: BlockRef,
    pub body: BlockRef,
    pub backedge: BlockRef,
    pub exit: BlockRef,
    pub abort: BlockRef,
    pub roots: Vec<Slot>,
    pub checked_instruction: usize,
    pub checked_span: Span,
    pub checked_operator: CheckedArithmeticOperator,
}
impl Plan {
    fn roles(&self) -> Vec<BlockRef> {
        let mut roles = vec![
            self.parent_header,
            self.entry,
            self.header,
            self.body,
            self.backedge,
            self.exit,
            self.abort,
        ];
        roles.extend(self.initial_base);
        roles
    }
    fn valid(&self) -> bool {
        self.roles().into_iter().collect::<BTreeSet<_>>().len() == self.roles().len()
            && !self.roots.is_empty()
            && self.roots.iter().collect::<BTreeSet<_>>().len() == self.roots.len()
    }
}
#[derive(Clone, Debug, Default)]
pub(super) struct Plans(pub Option<Plan>);
impl Plans {
    pub(super) fn fold(
        &mut self,
        block: BlockRef,
        successor: BlockRef,
        prefix: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        if let Some(p) = &mut self.0 {
            if p.body == successor {
                p.checked_instruction = p
                    .checked_instruction
                    .checked_add(prefix)
                    .ok_or_else(|| invalid(span))?;
            }
            for role in [
                &mut p.parent_header,
                &mut p.entry,
                &mut p.header,
                &mut p.body,
                &mut p.backedge,
                &mut p.exit,
                &mut p.abort,
            ] {
                if *role == successor {
                    *role = block;
                }
            }
            if p.initial_base == Some(successor) {
                p.initial_base = Some(block);
            }
            if !p.valid() {
                return Err(invalid(span));
            }
        }
        Ok(())
    }
}
struct Checked {
    id: InstructionId,
    operation: Operation,
    results: Vec<EntityId>,
}
#[derive(Default)]
pub(super) struct Emitted {
    plan: Option<Plan>,
    function: Option<FunctionId>,
    blocks: BTreeMap<BlockRef, BlockId>,
    slots: BTreeMap<BlockRef, Vec<Slot>>,
    entities: BTreeMap<BlockRef, Vec<EntityId>>,
    seen: BTreeMap<BlockRef, BTreeMap<Slot, EntityId>>,
    checked: Option<Checked>,
    initial_sources: BTreeMap<EntityId, (InstructionId, Operation)>,
}
impl Emitted {
    pub(super) fn layout(
        &mut self,
        body: &RefinedBody,
        function: &Function,
        blocks: &BTreeMap<BlockRef, BlockId>,
        slots: &BTreeMap<BlockRef, Vec<Slot>>,
        entities: &BTreeMap<BlockRef, Vec<EntityId>>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if body
            .nested_body_plans
            .0
            .as_ref()
            .is_some_and(|plan| plan.cycle_required)
            != body.child_cycle_plans.0.is_some()
            || body
                .nested_body_plans
                .0
                .as_ref()
                .is_some_and(|plan| plan.initial_base_required)
                != body
                    .child_cycle_plans
                    .0
                    .as_ref()
                    .is_some_and(|plan| plan.initial_base.is_some())
        {
            #[cfg(test)]
            super::build::owner_consumer_test_support::record_action(
                "child-cycle-independent-rejection",
                1,
            );
            return Err(invalid(span));
        }
        let Some(p) = &body.child_cycle_plans.0 else {
            return Ok(());
        };
        let fail = || invalid(span);
        if self.plan.is_some() || !p.valid() {
            return Err(fail());
        }
        let raw = body.blocks.get(p.body.0).ok_or_else(fail)?;
        let checked = raw.statements.get(p.checked_instruction).ok_or_else(fail)?;
        let ROp::CheckedArithmetic {
            operator,
            left,
            right,
        } = &checked.op
        else {
            return Err(fail());
        };
        if p.checked_operator != CheckedArithmeticOperator::Subtract
            || *operator != p.checked_operator
            || checked.span != p.checked_span
            || p.checked_instruction + 1 != raw.statements.len()
            || checked.results.len() != 2
            || checked.results[0] == checked.results[1]
            || checked.results.contains(left)
            || checked.results.contains(right)
            || p.roots.contains(left)
            || p.roots.contains(right)
            || checked.results.iter().any(|s| p.roots.contains(s))
        {
            return Err(fail());
        }
        let raw_term = |r: BlockRef| {
            body.blocks
                .get(r.0)
                .and_then(|b| b.terminator.as_ref())
                .map(|t| &t.0)
        };
        if !matches!(raw_term(p.entry),Some(RTerm::Goto(t)) if *t==p.header)
            || !matches!(raw_term(p.header),Some(RTerm::Branch{when_true,when_false,..}) if *when_true==p.body && *when_false==p.exit)
            || !matches!(raw_term(p.body),Some(RTerm::Branch{condition,when_true,when_false}) if *condition==checked.results[1] && *when_true==p.abort && *when_false==p.backedge)
            || !matches!(raw_term(p.backedge),Some(RTerm::Goto(t)) if *t==p.header)
            || !matches!(raw_term(p.abort),Some(RTerm::Abort(RAbort::CheckedArithmetic{operator,divisor})) if *operator==p.checked_operator && divisor==right)
            || body
                .blocks
                .get(p.abort.0)
                .is_none_or(|b| !b.statements.is_empty())
        {
            return Err(fail());
        }
        let mut ids = BTreeSet::new();
        for role in p.roles() {
            let id = *blocks.get(&role).ok_or_else(fail)?;
            let ss = slots.get(&role).ok_or_else(fail)?;
            let es = entities.get(&role).ok_or_else(fail)?;
            if !ids.insert(id)
                || ss.len() != es.len()
                || ss.iter().collect::<BTreeSet<_>>().len() != ss.len()
                || es.iter().collect::<BTreeSet<_>>().len() != es.len()
                || function.block(id).is_none_or(|b| b.parameters != *es)
            {
                return Err(fail());
            }
            if role != p.entry
                && role != p.abort
                && Some(role) != p.initial_base
                && p.roots.iter().any(|r| !ss.contains(r))
            {
                return Err(fail());
            }
            self.blocks.insert(role, id);
            self.slots.insert(role, ss.clone());
            self.entities.insert(role, es.clone());
        }
        self.function = Some(function.id());
        self.plan = Some(p.clone());
        Ok(())
    }
    pub(super) fn terminator(
        &mut self,
        body: &RefinedBody,
        function: &Function,
        from: BlockId,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &self.plan else {
            return Ok(());
        };
        let Some(role) = self
            .blocks
            .iter()
            .find_map(|(r, id)| (*id == from).then_some(*r))
        else {
            return Ok(());
        };
        let fail = || invalid(span);
        if self.function != Some(function.id()) || self.seen.insert(role, current.clone()).is_some()
        {
            return Err(fail());
        }
        if Some(role) == p.initial_base {
            for root in &p.roots {
                let owner = *current.get(root).ok_or_else(fail)?;
                if let crate::ssa::model::Definition::InstructionResult { instruction, .. } =
                    function.entity(owner).ok_or_else(fail)?.definition
                {
                    let operation = function
                        .instruction(instruction)
                        .ok_or_else(fail)?
                        .operation
                        .clone();
                    if self
                        .initial_sources
                        .insert(owner, (instruction, operation))
                        .is_some()
                    {
                        return Err(fail());
                    }
                }
            }
        }
        if role == p.body {
            let raw = body
                .blocks
                .get(p.body.0)
                .and_then(|b| b.statements.get(p.checked_instruction))
                .ok_or_else(fail)?;
            let ROp::CheckedArithmetic {
                operator,
                left,
                right,
            } = &raw.op
            else {
                return Err(fail());
            };
            let b = function.block(from).ok_or_else(fail)?;
            let id = *b.instructions.last().ok_or_else(fail)?;
            let i = function.instruction(id).ok_or_else(fail)?;
            let (Some(left), Some(right)) = (current.get(left), current.get(right)) else {
                return Err(fail());
            };
            if i.block != from
                || i.origin.span() != p.checked_span
                || !matches!(i.operation,Operation::CheckedArithmetic{operator:op,left:l,right:r} if op==*operator && *left==EntityId::Value(l) && *right==EntityId::Value(r))
                || i.results.len() != 2
                || raw
                    .results
                    .iter()
                    .zip(&i.results)
                    .any(|(s, e)| current.get(s) != Some(e))
            {
                return Err(fail());
            }
            self.checked = Some(Checked {
                id,
                operation: i.operation.clone(),
                results: i.results.clone(),
            });
        }
        Ok(())
    }
    fn parameter(&self, role: BlockRef, root: Slot, span: Span) -> Result<EntityId, LoweringError> {
        let fail = || invalid(span);
        let ss = self.slots.get(&role).ok_or_else(fail)?;
        let i = ss.iter().position(|s| *s == root).ok_or_else(fail)?;
        self.entities
            .get(&role)
            .and_then(|es| es.get(i))
            .copied()
            .ok_or_else(fail)
    }
    fn edge(
        &self,
        function: &Function,
        from: BlockRef,
        to: BlockRef,
        edge: &Edge,
        span: Span,
    ) -> Result<(), LoweringError> {
        let fail = || invalid(span);
        let ss = self.slots.get(&to).ok_or_else(fail)?;
        let current = self.seen.get(&from).ok_or_else(fail)?;
        let expected = ss
            .iter()
            .map(|s| current.get(s).copied().ok_or_else(fail))
            .collect::<Result<Vec<_>, _>>()?;
        if edge.target != *self.blocks.get(&to).ok_or_else(fail)?
            || edge.arguments != expected
            || function
                .block(edge.target)
                .is_none_or(|b| Some(&b.parameters) != self.entities.get(&to))
        {
            return Err(fail());
        }
        Ok(())
    }
    pub(super) fn finish(
        &self,
        body: &RefinedBody,
        function: &Function,
        generations: &[(Slot, EntityId, EntityId)],
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &self.plan else {
            return if body.child_cycle_plans.0.is_none() {
                Ok(())
            } else {
                Err(invalid(span))
            };
        };
        let fail = || invalid(span);
        let actual = body.child_cycle_plans.0.as_ref().ok_or_else(fail)?;
        if self.function != Some(function.id())
            || actual.control != p.control
            || actual.roles() != p.roles()
            || actual.roots != p.roots
            || actual.checked_instruction != p.checked_instruction
            || actual.checked_span != p.checked_span
            || actual.checked_operator != p.checked_operator
            || self.seen.len() != p.roles().len()
            || generations.len() != p.roots.len()
            || generations.iter().map(|g| g.0).collect::<BTreeSet<_>>()
                != p.roots.iter().copied().collect()
            || generations
                .iter()
                .map(|g| g.1)
                .collect::<BTreeSet<_>>()
                .len()
                != generations.len()
            || generations
                .iter()
                .map(|g| g.2)
                .collect::<BTreeSet<_>>()
                .len()
                != generations.len()
            || generations
                .iter()
                .any(|g| generations.iter().any(|h| g.2 == h.1))
        {
            return Err(fail());
        }
        let block = |r: BlockRef| {
            self.blocks
                .get(&r)
                .and_then(|id| function.block(*id))
                .ok_or_else(fail)
        };
        for role in p.roles() {
            if Some(&block(role)?.parameters) != self.entities.get(&role) {
                return Err(fail());
            }
        }
        for (target, allowed) in [
            (p.header, vec![p.entry, p.backedge]),
            (p.body, vec![p.header]),
            (p.backedge, vec![p.body]),
            (p.exit, vec![p.header]),
            (p.abort, vec![p.body]),
        ] {
            let mut predecessors = BTreeSet::new();
            for b in &function.blocks {
                let term = &b.terminator.as_ref().ok_or_else(fail)?.kind;
                for t in term.targets() {
                    if t == self.blocks[&target]
                        && (matches!(term, TerminatorKind::NullableBranch { .. })
                            || !predecessors.insert(b.id))
                    {
                        return Err(fail());
                    }
                }
            }
            if predecessors != allowed.iter().map(|r| self.blocks[r]).collect() {
                return Err(fail());
            }
        }
        for from in [p.entry, p.backedge] {
            let TerminatorKind::Branch(edge) =
                &block(from)?.terminator.as_ref().ok_or_else(fail)?.kind
            else {
                return Err(fail());
            };
            self.edge(function, from, p.header, edge, span)?;
        }
        let TerminatorKind::Conditional {
            when_true,
            when_false,
            condition,
        } = &block(p.header)?.terminator.as_ref().ok_or_else(fail)?.kind
        else {
            return Err(fail());
        };
        let Some((
            RTerm::Branch {
                condition: guard, ..
            },
            _,
        )) = body
            .blocks
            .get(p.header.0)
            .and_then(|b| b.terminator.as_ref())
        else {
            return Err(fail());
        };
        if self.seen[&p.header].get(guard) != Some(&EntityId::Value(*condition)) {
            return Err(fail());
        }
        self.edge(function, p.header, p.body, when_true, span)?;
        self.edge(function, p.header, p.exit, when_false, span)?;
        let checked = self.checked.as_ref().ok_or_else(fail)?;
        let b = block(p.body)?;
        let i = function.instruction(checked.id).ok_or_else(fail)?;
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &b.terminator.as_ref().ok_or_else(fail)?.kind
        else {
            return Err(fail());
        };
        if b.instructions.last() != Some(&checked.id)
            || b.instructions
                .iter()
                .filter(|id| **id == checked.id)
                .count()
                != 1
            || i.block != b.id
            || i.origin.span() != p.checked_span
            || i.operation != checked.operation
            || i.results != checked.results
            || i.results.len() != 2
            || i.results[1] != EntityId::Value(*condition)
        {
            return Err(fail());
        }
        self.edge(function, p.body, p.abort, when_true, span)?;
        self.edge(function, p.body, p.backedge, when_false, span)?;
        let abort = block(p.abort)?;
        let terminal = abort.terminator.as_ref().ok_or_else(fail)?;
        // Subtraction's abort metadata references the emitted RHS directly; unlike
        // division it has no live divisor parameter on the abort edge.
        let Operation::CheckedArithmetic { right: divisor, .. } = checked.operation else {
            return Err(fail());
        };
        if !abort.instructions.is_empty()
            || !matches!(terminal.kind, TerminatorKind::Abort)
            || !matches!(terminal.abort.as_ref(),Some(AbortReason::CheckedArithmetic{operator,divisor:d}) if *operator==p.checked_operator && divisor == *d)
        {
            return Err(fail());
        }
        self.parent_entry(body, function, span)?;
        self.initial_base(function, span)?;
        let entry = self.seen.get(&p.entry).ok_or_else(fail)?;
        let entry_roots = p
            .roots
            .iter()
            .map(|r| entry.get(r).copied().ok_or_else(fail))
            .collect::<Result<BTreeSet<_>, _>>()?;
        if entry_roots.len() != p.roots.len() {
            return Err(fail());
        }
        for &(root, input, new) in generations {
            if self.parameter(p.body, root, span)? != input
                || self.seen[&p.body].get(&root) != Some(&new)
            {
                return Err(fail());
            }
            for role in [p.header, p.backedge] {
                let value = self.parameter(role, root, span)?;
                if self.seen[&role].get(&root) != Some(&value) {
                    return Err(fail());
                }
                for id in &block(role)?.instructions {
                    let op = function.instruction(*id).ok_or_else(fail)?;
                    if op.results.contains(&value) || op.operation.entities().contains(&value) {
                        return Err(fail());
                    }
                }
            }
            // The newly installed generation has one definition and no direct
            // consumer. Mutually exclusive success/abort edges each carry at most
            // one instance; abort may carry the live owner but never returns it.
            if function
                .instructions
                .iter()
                .filter(|op| op.results.contains(&new))
                .count()
                != 1
            {
                return Err(fail());
            }
            for op in &function.instructions {
                if op.operation.entities().contains(&new) {
                    return Err(fail());
                }
            }
            for block in &function.blocks {
                if block.parameters.contains(&new) {
                    return Err(fail());
                }
                let uses = block
                    .terminator
                    .as_ref()
                    .ok_or_else(fail)?
                    .kind
                    .entities()
                    .iter()
                    .filter(|entity| **entity == new)
                    .count();
                if uses
                    != if block.id == b.id {
                        1 + usize::from(when_true.arguments.contains(&new))
                    } else {
                        0
                    }
                {
                    return Err(fail());
                }
            }
            if when_true.arguments.contains(&new) {
                let position = self.slots[&p.abort]
                    .iter()
                    .position(|slot| *slot == root)
                    .ok_or_else(fail)?;
                if when_true.arguments.get(position) != Some(&new) {
                    return Err(fail());
                }
            }
            if when_false
                .arguments
                .iter()
                .filter(|entity| **entity == new)
                .count()
                != 1
                || when_true
                    .arguments
                    .iter()
                    .filter(|entity| **entity == new)
                    .count()
                    > 1
            {
                return Err(fail());
            }
        }
        #[cfg(test)]
        super::build::owner_consumer_test_support::record_action(
            "child-cycle-ssa-complete",
            p.roots.len(),
        );
        Ok(())
    }
}
