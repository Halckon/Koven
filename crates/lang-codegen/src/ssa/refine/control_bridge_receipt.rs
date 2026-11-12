//! Production slot-to-SSA transport checks for an already admitted control bridge.
//! This consumes actual emission events; it grants no new frontend ownership capability.
use super::body::{BlockRef, ROp, RTerm, RefinedBody, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{
        BlockId, Edge, EntityId, Function, FunctionId, InstructionId, Operation, TerminatorKind,
    },
};
use lang_frontend::source::Span;
use std::collections::{BTreeMap, BTreeSet};

fn invalid(span: Span) -> LoweringError {
    error(LoweringErrorKind::InvalidModel, span)
}

#[derive(Clone, Debug, Default)]
pub(super) struct Plans {
    plan: Option<Plan>,
}
#[derive(Clone, Debug)]
struct Plan {
    prior: [BlockRef; 4],
    next: [BlockRef; 4],
    checked: [BlockRef; 2],
    sources: Vec<Slot>,
    targets: Vec<Slot>,
}
impl Plans {
    pub(super) fn seal(
        prior: [BlockRef; 4],
        next: [BlockRef; 4],
        next_checked: [BlockRef; 2],
        sources: Vec<Slot>,
        targets: Vec<Slot>,
        span: Span,
    ) -> Result<Self, LoweringError> {
        let unique = |s: &[Slot]| s.iter().collect::<BTreeSet<_>>().len() == s.len();
        if sources.len() < 2
            || sources.len() != targets.len()
            || !unique(&sources)
            || !unique(&targets)
            || sources.iter().any(|s| targets.contains(s))
            || prior[0] == next[0]
            || prior[1] == next[1]
            || next_checked[0] == next_checked[1]
            || next_checked.iter().any(|b| next.contains(b))
        {
            return Err(invalid(span));
        }
        Ok(Self {
            plan: Some(Plan {
                prior,
                next,
                checked: next_checked,
                sources,
                targets,
            }),
        })
    }
    /// `successor` is folded into `block`, matching merge_straight_lines.
    pub(super) fn fold(
        &mut self,
        block: BlockRef,
        successor: BlockRef,
        _prefix_len: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        if block == successor {
            return Err(invalid(span));
        }
        if let Some(p) = &mut self.plan {
            for b in p
                .prior
                .iter_mut()
                .chain(p.next.iter_mut())
                .chain(p.checked.iter_mut())
            {
                if *b == successor {
                    *b = block;
                }
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct Emitted {
    caller: Option<FunctionId>,
    blocks: BTreeMap<BlockRef, BlockId>,
    parameters: BTreeMap<BlockRef, Vec<Slot>>,
    current: BTreeMap<BlockRef, BTreeMap<Slot, EntityId>>,
    positions: BTreeMap<BlockRef, usize>,
    instructions: BTreeMap<BlockRef, Vec<InstructionId>>,
    terms: BTreeMap<BlockRef, TerminatorKind>,
    drops: BTreeMap<InstructionId, (BlockRef, EntityId)>,
    replacements: BTreeMap<(usize, usize), (InstructionId, Operation, Vec<EntityId>)>,
}
impl Emitted {
    pub(super) fn new() -> Self {
        Self::default()
    }
    pub(super) fn layout(
        &mut self,
        body: &RefinedBody,
        function: &Function,
        blocks: &BTreeMap<BlockRef, BlockId>,
        parameters: &BTreeMap<BlockRef, Vec<Slot>>,
        entities: &BTreeMap<BlockRef, Vec<EntityId>>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &body.owner_bridge_plans.plan else {
            return Ok(());
        };
        if self.caller.is_some() {
            return Err(invalid(span));
        }
        self.caller = Some(function.id());
        self.blocks = blocks.clone();
        self.parameters = parameters.clone();
        for (block, id) in blocks {
            let slots = parameters.get(block).ok_or_else(|| invalid(span))?;
            let values = entities.get(block).ok_or_else(|| invalid(span))?;
            if slots.len() != values.len()
                || function.block(*id).map(|b| &b.parameters) != Some(values)
                || slots.iter().collect::<BTreeSet<_>>().len() != slots.len()
            {
                return Err(invalid(span));
            }
            self.current.insert(
                *block,
                slots.iter().copied().zip(values.iter().copied()).collect(),
            );
            self.positions.insert(*block, 0);
            self.instructions.insert(
                *block,
                function
                    .block(*id)
                    .ok_or_else(|| invalid(span))?
                    .instructions
                    .clone(),
            );
        }
        for block in p.prior.iter().chain(p.next.iter()).chain(p.checked.iter()) {
            if !blocks.contains_key(block) {
                return Err(invalid(span));
            }
        }
        // The admitted checked normal predecessor is success, never abort.
        let term = |b: BlockRef| {
            body.blocks
                .get(b.0)
                .and_then(|b| b.terminator.as_ref())
                .map(|(t, _)| t)
        };
        if !matches!(term(p.next[1]), Some(RTerm::Branch { when_true, when_false, .. }) if *when_true == p.checked[1] && *when_false == p.checked[0])
            || !matches!(term(p.checked[0]), Some(RTerm::Goto(b)) if *b == p.next[0])
            || !matches!(term(p.checked[1]), Some(RTerm::Abort(_)))
        {
            return Err(invalid(span));
        }
        for (header, roots) in [(p.prior[0], &p.sources), (p.next[0], &p.targets)] {
            if roots.iter().any(|s| !parameters[&header].contains(s)) {
                return Err(invalid(span));
            }
        }
        Ok(())
    }
    /// Called after aliases update current, or after actual instruction results update it.
    pub(super) fn statement(
        &mut self,
        body: &RefinedBody,
        function: &Function,
        position: (BlockRef, usize),
        instruction: Option<InstructionId>,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let (block, index) = position;
        let Some(p) = &body.owner_bridge_plans.plan else {
            return Ok(());
        };
        if self.caller != Some(function.id())
            || self.positions.get(&block) != Some(&index)
            || self.terms.contains_key(&block)
        {
            return Err(invalid(span));
        }
        let stmt = body
            .blocks
            .get(block.0)
            .and_then(|b| b.statements.get(index))
            .ok_or_else(|| invalid(span))?;
        let expected = self.current.get_mut(&block).ok_or_else(|| invalid(span))?;
        if let ROp::Alias { source } = stmt.op {
            let [result] = stmt.results.as_slice() else {
                return Err(invalid(span));
            };
            if instruction.is_some() {
                return Err(invalid(span));
            }
            let value = *expected.get(&source).ok_or_else(|| invalid(span))?;
            expected.insert(*result, value);
        } else {
            let id = instruction.ok_or_else(|| invalid(span))?;
            let actual = function.instruction(id).ok_or_else(|| invalid(span))?;
            let observed = self
                .instructions
                .get_mut(&block)
                .ok_or_else(|| invalid(span))?;
            if observed.contains(&id) {
                return Err(invalid(span));
            }
            observed.push(id);
            if actual.block != self.blocks[&block] || actual.results.len() != stmt.results.len() {
                return Err(invalid(span));
            }
            if let ROp::RootReplace {
                owner,
                loan,
                replacement,
            } = stmt.op
            {
                let role = if block == p.prior[1] {
                    Some((0, &p.sources))
                } else if block == p.next[1] {
                    Some((1, &p.targets))
                } else {
                    None
                };
                if let Some((side, roots)) = role
                    && let Some(root) = roots.iter().position(|s| *s == owner)
                {
                    let (
                        Some(EntityId::Value(owner_value)),
                        Some(EntityId::Loan(loan_value)),
                        Some(EntityId::Value(new_value)),
                    ) = (
                        expected.get(&owner),
                        expected.get(&loan),
                        expected.get(&replacement),
                    )
                    else {
                        return Err(invalid(span));
                    };
                    let op = Operation::RootReplace {
                        owner: *owner_value,
                        loan: *loan_value,
                        replacement: *new_value,
                    };
                    if actual.operation != op
                        || stmt.results.first() != Some(&owner)
                        || actual.results.len() != 2
                        || self
                            .replacements
                            .insert((side, root), (id, op, actual.results.clone()))
                            .is_some()
                    {
                        return Err(invalid(span));
                    }
                }
            }
            if let ROp::Drop { owner } = stmt.op {
                let value = *expected.get(&owner).ok_or_else(|| invalid(span))?;
                let EntityId::Value(value_id) = value else {
                    return Err(invalid(span));
                };
                if actual.operation != (Operation::Drop { owner: value_id })
                    || self.drops.insert(id, (block, value)).is_some()
                {
                    return Err(invalid(span));
                }
            }
            for (slot, value) in stmt.results.iter().zip(&actual.results) {
                expected.insert(*slot, *value);
            }
        }
        if expected != current {
            return Err(invalid(span));
        }
        self.positions.insert(block, index + 1);
        Ok(())
    }
    fn edge(
        &self,
        body: &RefinedBody,
        destination: BlockRef,
        edge: &Edge,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.blocks.get(&destination) != Some(&edge.target) {
            return Err(invalid(span));
        }
        let view = body
            .blocks
            .get(destination.0)
            .ok_or_else(|| invalid(span))?
            .view;
        let slots = self
            .parameters
            .get(&destination)
            .ok_or_else(|| invalid(span))?;
        let expected = slots
            .iter()
            .filter(|s| Some(**s) != view)
            .map(|s| current.get(s).copied().ok_or_else(|| invalid(span)))
            .collect::<Result<Vec<_>, _>>()?;
        if edge.arguments != expected {
            return Err(invalid(span));
        }
        Ok(())
    }
    pub(super) fn terminator(
        &mut self,
        plans: &Plans,
        body: &RefinedBody,
        function: &mut Function,
        block: BlockRef,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(plan) = &plans.plan else {
            return Ok(());
        };
        #[cfg(test)]
        {
            use super::build::owner_consumer_test_support::{
                OwnerFault, record_action, take_fault,
            };
            if block == plan.next[1] && take_fault(OwnerFault::BridgeSsaDropNewGeneration) {
                let (_, _, results) = self
                    .replacements
                    .get(&(1, 0))
                    .ok_or_else(|| invalid(span))?;
                let EntityId::Value(new_owner) = results[0] else {
                    return Err(invalid(span));
                };
                let retired = results[1];
                let id = *self
                    .drops
                    .iter()
                    .find(|(_, (b, v))| *b == block && *v == retired)
                    .map(|(id, _)| id)
                    .ok_or_else(|| invalid(span))?;
                function
                    .instructions
                    .iter_mut()
                    .find(|i| i.id == id)
                    .ok_or_else(|| invalid(span))?
                    .operation = Operation::Drop { owner: new_owner };
                record_action("bridge-ssa-fault-injected", 4);
            }
            if block == plan.next[1] && take_fault(OwnerFault::BridgeSsaExtraDrop) {
                let target = self.blocks[&block];
                let drop = function
                    .block(target)
                    .ok_or_else(|| invalid(span))?
                    .instructions
                    .iter()
                    .copied()
                    .find(|id| {
                        function
                            .instruction(*id)
                            .is_some_and(|i| matches!(i.operation, Operation::Drop { .. }))
                    })
                    .ok_or_else(|| invalid(span))?;
                function
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == target)
                    .ok_or_else(|| invalid(span))?
                    .instructions
                    .push(drop);
                record_action("bridge-ssa-fault-injected", 3);
            }
            if block == plan.next[0] && take_fault(OwnerFault::BridgeSsaMissingTerminator) {
                record_action("bridge-ssa-fault-injected", 1);
                return Ok(());
            }
            if block == plan.next[0] && take_fault(OwnerFault::BridgeSsaWrongRootArgument) {
                record_action("bridge-ssa-fault-injected", 0);
                let positions = &self.parameters[&plan.next[1]];
                let a = positions
                    .iter()
                    .position(|s| *s == plan.targets[0])
                    .ok_or_else(|| invalid(span))?;
                let b = positions
                    .iter()
                    .position(|s| *s == plan.targets[1])
                    .ok_or_else(|| invalid(span))?;
                let target = self.blocks[&block];
                let data = function
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == target)
                    .ok_or_else(|| invalid(span))?;
                if let Some(t) = &mut data.terminator
                    && let TerminatorKind::Conditional { when_true, .. } = &mut t.kind
                {
                    when_true.arguments.swap(a, b);
                } else {
                    return Err(invalid(span));
                }
            }
            if block == plan.checked[1] && take_fault(OwnerFault::BridgeSsaAbortBackedge) {
                record_action("bridge-ssa-fault-injected", 2);
                let target = self.blocks[&block];
                let data = function
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == target)
                    .ok_or_else(|| invalid(span))?;
                data.terminator.as_mut().ok_or_else(|| invalid(span))?.kind =
                    TerminatorKind::Branch(Edge {
                        target: self.blocks[&plan.next[0]],
                        arguments: Vec::new(),
                    });
            }
        }
        let _ = plan;
        let original = body.blocks.get(block.0).ok_or_else(|| invalid(span))?;
        if self.caller != Some(function.id())
            || self.current.get(&block) != Some(current)
            || self.positions.get(&block) != Some(&original.statements.len())
            || self.terms.contains_key(&block)
        {
            return Err(invalid(span));
        }
        let term = &function
            .block(*self.blocks.get(&block).ok_or_else(|| invalid(span))?)
            .and_then(|b| b.terminator.as_ref())
            .ok_or_else(|| invalid(span))?
            .kind;
        let (original, _) = original.terminator.as_ref().ok_or_else(|| invalid(span))?;
        match (original, term) {
            (RTerm::Goto(target), TerminatorKind::Branch(edge)) => {
                self.edge(body, *target, edge, current, span)?
            }
            (
                RTerm::Branch {
                    condition,
                    when_true,
                    when_false,
                },
                TerminatorKind::Conditional {
                    condition: actual,
                    when_true: a,
                    when_false: b,
                },
            ) => {
                if current.get(condition) != Some(&EntityId::Value(*actual)) {
                    return Err(invalid(span));
                }
                self.edge(body, *when_true, a, current, span)?;
                self.edge(body, *when_false, b, current, span)?;
            }
            (
                RTerm::NullBranch {
                    owner,
                    when_null,
                    when_non_null,
                },
                TerminatorKind::NullableBranch {
                    owner: actual,
                    when_null: a,
                    when_non_null: b,
                    ..
                },
            ) => {
                if current.get(owner) != Some(&EntityId::Value(*actual)) {
                    return Err(invalid(span));
                }
                self.edge(body, *when_null, a, current, span)?;
                self.edge(body, *when_non_null, b, current, span)?;
            }
            (RTerm::Abort(_), TerminatorKind::Abort)
            | (RTerm::Unreachable, TerminatorKind::Unreachable)
            | (RTerm::Return(_), TerminatorKind::Return { .. })
            | (RTerm::BorrowReturn { .. }, TerminatorKind::BorrowReturn { .. })
            | (RTerm::RangeReturn { .. }, TerminatorKind::RangeReturn { .. }) => {}
            _ => return Err(invalid(span)),
        }
        self.terms.insert(block, term.clone());
        Ok(())
    }
    pub(super) fn finish(
        &self,
        plans: &Plans,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &plans.plan else {
            return Ok(());
        };
        if self.caller != Some(function.id())
            || self.terms.len() != self.blocks.len()
            || self.replacements.len() != p.sources.len() * 2
        {
            return Err(invalid(span));
        }
        for (block, ids) in &self.instructions {
            if function.block(self.blocks[block]).map(|b| &b.instructions) != Some(ids) {
                return Err(invalid(span));
            }
        }
        for (block, term) in &self.terms {
            if function
                .block(self.blocks[block])
                .and_then(|b| b.terminator.as_ref())
                .map(|t| &t.kind)
                != Some(term)
            {
                return Err(invalid(span));
            }
        }
        // A replacement's second result is the retired generation, not the new
        // header-carried root. Require its actual same-body consumption once.
        for ((side, _), (_, _, results)) in &self.replacements {
            let retired = results.get(1).ok_or_else(|| invalid(span))?;
            let block = if *side == 0 { p.prior[1] } else { p.next[1] };
            if self
                .drops
                .values()
                .filter(|(b, value)| *b == block && value == retired)
                .count()
                != 1
            {
                return Err(invalid(span));
            }
        }
        for (id, (_, entity)) in &self.drops {
            let EntityId::Value(value) = entity else {
                return Err(invalid(span));
            };
            if function.instruction(*id).map(|i| &i.operation)
                != Some(&Operation::Drop { owner: *value })
            {
                return Err(invalid(span));
            }
        }
        for (id, op, results) in self.replacements.values() {
            let actual = function.instruction(*id).ok_or_else(|| invalid(span))?;
            if &actual.operation != op || &actual.results != results {
                return Err(invalid(span));
            }
        }
        #[cfg(test)]
        super::build::owner_consumer_test_support::record_action(
            "bridge-ssa-receipt-complete",
            p.sources.len(),
        );
        Ok(())
    }
}
