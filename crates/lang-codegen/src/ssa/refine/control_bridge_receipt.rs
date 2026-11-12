//! Production slot-to-SSA transport checks for an already admitted control bridge.
//! This consumes actual emission events; it grants no new frontend ownership capability.
use super::body::{BlockRef, ROp, RTerm, RefinedBody, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{
        BlockId, Edge, EntityId, Function, FunctionId, InstructionId, Operation, TerminatorKind,
    },
};
use lang_frontend::{source::Span, type_checking::UnitStatementId};
use std::collections::{BTreeMap, BTreeSet};

fn invalid(span: Span) -> LoweringError {
    error(LoweringErrorKind::InvalidModel, span)
}

#[derive(Clone, Debug, Default)]
pub(super) struct Plans {
    plan: Option<Plan>,
    group: Vec<Member>,
    leading: Option<Anchor>,
    links: Vec<Link>,
}
#[derive(Clone, Debug)]
struct Plan {
    prior: [BlockRef; 4],
    next: [BlockRef; 4],
    checked: [BlockRef; 2],
    sources: Vec<Slot>,
    targets: Vec<Slot>,
}
/// Already validated actual control geometry, not a concrete entry-owner catalog.
#[derive(Clone, Debug)]
pub(super) struct Member {
    pub control: UnitStatementId,
    pub blocks: [BlockRef; 4],
    pub checked: [BlockRef; 2],
    pub roots: Vec<Slot>,
}
/// Actual first-control transport and separately validated normal-body root snapshots.
#[derive(Clone, Debug)]
pub(super) struct Anchor {
    pub control: UnitStatementId,
    pub blocks: [BlockRef; 4],
    pub roots: Vec<Slot>,
    pub body_roots: Vec<Slot>,
    pub scalar: Option<[Slot; 2]>,
    pub checked: [BlockRef; 2],
}
/// Actual source-to-target alias mapping between the two admitted members.
#[derive(Clone, Debug)]
pub(super) struct Link {
    pub from_control: UnitStatementId,
    pub to_control: UnitStatementId,
    pub block: BlockRef,
    pub pairs: Vec<(Slot, Slot)>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Role {
    Legacy(usize),
    Control(UnitStatementId),
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
            group: Vec::new(),
            leading: None,
            links: Vec::new(),
            plan: Some(Plan {
                prior,
                next,
                checked: next_checked,
                sources,
                targets,
            }),
        })
    }
    pub(super) fn seal_group(
        group: Vec<Member>,
        leading: Anchor,
        links: Vec<Link>,
        span: Span,
    ) -> Result<Self, LoweringError> {
        if group.len() != 2
            || group.iter().enumerate().any(|(i, m)| {
                let blocks: Vec<_> = m.blocks.iter().chain(m.checked.iter()).copied().collect();
                m.roots.len() < 2
                    || m.roots.iter().collect::<BTreeSet<_>>().len() != m.roots.len()
                    || blocks.iter().collect::<BTreeSet<_>>().len() != blocks.len()
                    || group[..i].iter().any(|p| {
                        p.control == m.control
                            || p.control.source_unit() != m.control.source_unit()
                            || p.blocks
                                .iter()
                                .chain(p.checked.iter())
                                .any(|b| blocks.contains(b))
                            || p.roots.iter().any(|r| m.roots.contains(r))
                    })
            })
        {
            return Err(invalid(span));
        }
        let leading_blocks: Vec<_> = leading
            .blocks
            .iter()
            .chain(leading.checked.iter())
            .copied()
            .collect();
        if leading
            .scalar
            .is_some_and(|s| s[0] == s[1] || s.iter().any(|r| leading.body_roots.contains(r)))
            || leading.body_roots.len() < 2
            || leading.body_roots.iter().collect::<BTreeSet<_>>().len() != leading.body_roots.len()
            || leading
                .roots
                .iter()
                .any(|r| !leading.body_roots.contains(r))
            || leading.roots.len() < 2
            || leading.roots.iter().collect::<BTreeSet<_>>().len() != leading.roots.len()
            || leading_blocks.iter().collect::<BTreeSet<_>>().len() != leading_blocks.len()
            || group.iter().any(|m| {
                m.control == leading.control
                    || m.control.source_unit() != leading.control.source_unit()
                    || m.roots.iter().any(|r| leading.body_roots.contains(r))
                    || m.blocks
                        .iter()
                        .chain(m.checked.iter())
                        .any(|b| leading_blocks.contains(b))
            })
            || links.len() != 2
        {
            return Err(invalid(span));
        }
        for (index, link) in links.iter().enumerate() {
            let (control, blocks, roots) = if index == 0 {
                (leading.control, leading.blocks, &leading.roots)
            } else {
                (group[0].control, group[0].blocks, &group[0].roots)
            };
            let next = &group[index];
            if link.from_control != control
                || link.to_control != next.control
                || link.block != blocks[3]
                || link.pairs.len() != next.roots.len()
                || link.pairs.iter().enumerate().any(|(i, (source, target))| {
                    !roots.contains(source)
                        || *target != next.roots[i]
                        || link.pairs[..i]
                            .iter()
                            .any(|(s, t)| s == source || t == target)
                })
            {
                return Err(invalid(span));
            }
        }
        Ok(Self {
            plan: None,
            group,
            leading: Some(leading),
            links,
        })
    }

    fn active(&self) -> bool {
        self.plan.is_some() || !self.group.is_empty()
    }
    fn roles(&self) -> Vec<(Role, [BlockRef; 4], &[Slot])> {
        if let Some(p) = &self.plan {
            vec![
                (Role::Legacy(0), p.prior, &p.sources),
                (Role::Legacy(1), p.next, &p.targets),
            ]
        } else {
            self.leading
                .iter()
                .map(|m| (Role::Control(m.control), m.blocks, m.body_roots.as_slice()))
                .chain(
                    self.group
                        .iter()
                        .map(|m| (Role::Control(m.control), m.blocks, m.roots.as_slice())),
                )
                .collect()
        }
    }
    fn checked(&self) -> Vec<([BlockRef; 4], [BlockRef; 2])> {
        if let Some(p) = &self.plan {
            vec![(p.next, p.checked)]
        } else {
            self.leading
                .iter()
                .map(|m| (m.blocks, m.checked))
                .chain(self.group.iter().map(|m| (m.blocks, m.checked)))
                .collect()
        }
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
        #[cfg(test)]
        let touched = self
            .group
            .iter()
            .any(|m| m.blocks.contains(&successor) || m.checked.contains(&successor));
        for link in &mut self.links {
            if link.block == successor {
                link.block = block;
            }
        }
        #[cfg(test)]
        let leading_touched = self
            .leading
            .as_ref()
            .is_some_and(|a| a.blocks.contains(&successor) || a.checked.contains(&successor));
        if let Some(anchor) = &mut self.leading {
            for b in anchor.blocks.iter_mut().chain(anchor.checked.iter_mut()) {
                if *b == successor {
                    *b = block;
                }
            }
        }
        #[cfg(test)]
        if leading_touched && let Some(anchor) = &mut self.leading {
            use super::build::owner_consumer_test_support::{
                OwnerFault, record_action, take_fault,
            };
            if take_fault(OwnerFault::ChainLeadingSsaFoldWrongChecked) {
                anchor.checked[0] = anchor.checked[1];
                record_action("control-chain-leading-ssa-fault-hit", 1);
            }
        }
        for m in &mut self.group {
            for b in m.blocks.iter_mut().chain(m.checked.iter_mut()) {
                if *b == successor {
                    *b = block;
                }
            }
        }
        #[cfg(test)]
        if touched && self.group.len() == 2 {
            use super::build::owner_consumer_test_support::{
                OwnerFault, record_action, take_fault,
            };
            if take_fault(OwnerFault::ChainSsaFoldWrongBody) {
                self.group[1].blocks[1] = self.group[0].blocks[1];
                record_action("control-chain-ssa-fault-hit", 3);
            }
            if take_fault(OwnerFault::ChainSsaFoldWrongChecked) {
                self.group[1].checked[0] = self.group[0].checked[0];
                record_action("control-chain-ssa-fault-hit", 4);
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct Emitted {
    caller: Option<FunctionId>,
    links_observed: BTreeSet<(UnitStatementId, UnitStatementId)>,
    blocks: BTreeMap<BlockRef, BlockId>,
    parameters: BTreeMap<BlockRef, Vec<Slot>>,
    current: BTreeMap<BlockRef, BTreeMap<Slot, EntityId>>,
    positions: BTreeMap<BlockRef, usize>,
    instructions: BTreeMap<BlockRef, Vec<InstructionId>>,
    terms: BTreeMap<BlockRef, TerminatorKind>,
    drops: BTreeMap<InstructionId, (BlockRef, EntityId)>,
    replacements: BTreeMap<(Role, usize), (InstructionId, Operation, Vec<EntityId>)>,
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
        let plans = &body.owner_bridge_plans;
        if !plans.active() {
            return Ok(());
        }
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
        for (_, geometry, roots) in plans.roles() {
            if geometry.iter().any(|b| !blocks.contains_key(b))
                || parameters
                    .get(&geometry[0])
                    .is_none_or(|p| roots.iter().any(|s| !p.contains(s)))
            {
                return Err(invalid(span));
            }
        }
        // Normal success must return to its own header; abort is not a backedge.
        let term = |b: BlockRef| {
            body.blocks
                .get(b.0)
                .and_then(|b| b.terminator.as_ref())
                .map(|(t, _)| t)
        };
        for (geometry, checked) in plans.checked() {
            if checked.iter().any(|b| !blocks.contains_key(b))
                || !matches!(term(geometry[1]), Some(RTerm::Branch { when_true, when_false, .. }) if *when_true == checked[1] && *when_false == checked[0])
                || !matches!(term(checked[0]), Some(RTerm::Goto(b)) if *b == geometry[0])
                || !matches!(term(checked[1]), Some(RTerm::Abort(_)))
            {
                return Err(invalid(span));
            }
        }
        if let Some(anchor) = &plans.leading {
            let header = parameters
                .get(&anchor.blocks[0])
                .ok_or_else(|| invalid(span))?;
            if anchor.blocks.iter().any(|b| !blocks.contains_key(b))
                || anchor
                    .scalar
                    .is_some_and(|s| s.iter().any(|r| !header.contains(r)))
                || anchor.roots.iter().any(|r| !header.contains(r))
            {
                return Err(invalid(span));
            }
        }
        for link in &plans.links {
            let next = plans
                .group
                .iter()
                .find(|m| m.control == link.to_control)
                .ok_or_else(|| invalid(span))?;
            if !blocks.contains_key(&link.block)
                || !matches!(term(link.block), Some(RTerm::Goto(target)) if *target == next.blocks[0])
            {
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
        let plans = &body.owner_bridge_plans;
        if !plans.active() {
            return Ok(());
        }
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
                let roles = plans.roles();
                let mut matching = roles.iter().filter(|(_, geometry, _)| geometry[1] == block);
                let role = matching.next().map(|(role, _, roots)| (*role, *roots));
                if matching.next().is_some() {
                    return Err(invalid(span));
                }
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
        if !plans.active() {
            return Ok(());
        }
        #[cfg(test)]
        if let Some(plan) = &plans.plan {
            use super::build::owner_consumer_test_support::{
                OwnerFault, record_action, take_fault,
            };
            if block == plan.next[1] && take_fault(OwnerFault::BridgeSsaDropNewGeneration) {
                let (_, _, results) = self
                    .replacements
                    .get(&(Role::Legacy(1), 0))
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
        #[cfg(test)]
        if let Some(anchor) = &plans.leading
            && block == anchor.checked[1]
            && super::build::owner_consumer_test_support::take_fault(
                super::build::owner_consumer_test_support::OwnerFault::ChainLeadingSsaAbortBackedge,
            )
        {
            super::build::owner_consumer_test_support::record_action(
                "control-chain-leading-ssa-fault-hit",
                2,
            );
            let target = self.blocks[&block];
            let data = function
                .blocks
                .iter_mut()
                .find(|b| b.id == target)
                .ok_or_else(|| invalid(span))?;
            data.terminator.as_mut().ok_or_else(|| invalid(span))?.kind =
                TerminatorKind::Branch(Edge {
                    target: self.blocks[&anchor.blocks[0]],
                    arguments: Vec::new(),
                });
        }
        #[cfg(test)]
        if let Some(last) = plans.group.last()
            && block == last.blocks[1]
        {
            use super::build::owner_consumer_test_support::{
                OwnerFault, record_action, take_fault,
            };
            if take_fault(OwnerFault::ChainSsaMissingLastTerminator) {
                record_action("control-chain-ssa-fault-hit", 0);
                return Ok(());
            }
            if take_fault(OwnerFault::ChainSsaLastDropNewGeneration) {
                let (_, _, results) = self
                    .replacements
                    .get(&(Role::Control(last.control), 0))
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
                record_action("control-chain-ssa-fault-hit", 1);
            }
        }
        #[cfg(test)]
        if let (Some(first), Some(last)) = (plans.group.first(), plans.group.last())
            && block == last.blocks[0]
            && super::build::owner_consumer_test_support::take_fault(
                super::build::owner_consumer_test_support::OwnerFault::ChainSsaCrossMemberRootArgument)
        {
            let foreign = *self.current.get(&first.blocks[0])
                .and_then(|c| c.get(&first.roots[0])).ok_or_else(|| invalid(span))?;
            let index = self.parameters[&last.blocks[1]].iter()
                .position(|s| *s == last.roots[0]).ok_or_else(|| invalid(span))?;
            let data = function.blocks.iter_mut().find(|b| b.id == self.blocks[&block]).ok_or_else(|| invalid(span))?;
            let Some(t) = &mut data.terminator else { return Err(invalid(span)) };
            let TerminatorKind::Conditional { when_true, .. } = &mut t.kind else { return Err(invalid(span)) };
            if when_true.arguments.get(index) == Some(&foreign) { return Err(invalid(span)); }
            *when_true.arguments.get_mut(index).ok_or_else(|| invalid(span))? = foreign;
            super::build::owner_consumer_test_support::record_action("control-chain-ssa-fault-hit", 2);
        }
        #[cfg(test)]
        if let Some(link) = plans.links.last()
            && block == link.block
            && super::build::owner_consumer_test_support::take_fault(
                super::build::owner_consumer_test_support::OwnerFault::ChainSsaLinkWrongEdgeArguments)
        {
            let last = plans.group.last().ok_or_else(|| invalid(span))?;
            let positions = &self.parameters[&last.blocks[0]];
            let a = positions.iter().position(|s| *s == link.pairs[0].1).ok_or_else(|| invalid(span))?;
            let b = positions.iter().position(|s| *s == link.pairs[1].1).ok_or_else(|| invalid(span))?;
            let data = function.blocks.iter_mut().find(|b| b.id == self.blocks[&block]).ok_or_else(|| invalid(span))?;
            let Some(t) = &mut data.terminator else { return Err(invalid(span)) };
            let TerminatorKind::Branch(edge) = &mut t.kind else { return Err(invalid(span)) };
            if a == b || edge.arguments.get(a) == edge.arguments.get(b) || a >= edge.arguments.len() || b >= edge.arguments.len() { return Err(invalid(span)); }
            edge.arguments.swap(a, b);
            super::build::owner_consumer_test_support::record_action("control-chain-link-fault-hit", 2);
        }
        #[cfg(test)]
        if let Some(link) = plans.links.first()
            && block == link.block
            && super::build::owner_consumer_test_support::take_fault(
                super::build::owner_consumer_test_support::OwnerFault::ChainSsaLeadingLinkWrongEdgeArguments) {
            let next = plans.group.first().ok_or_else(|| invalid(span))?;
            let positions = &self.parameters[&next.blocks[0]];
            let a = positions.iter().position(|s| *s == link.pairs[0].1).ok_or_else(|| invalid(span))?;
            let b = positions.iter().position(|s| *s == link.pairs[1].1).ok_or_else(|| invalid(span))?;
            let data = function.blocks.iter_mut().find(|b| b.id == self.blocks[&block]).ok_or_else(|| invalid(span))?;
            let Some(t) = &mut data.terminator else { return Err(invalid(span)); };
            let TerminatorKind::Branch(edge) = &mut t.kind else { return Err(invalid(span)); };
            if a == b || a >= edge.arguments.len() || b >= edge.arguments.len()
                || edge.arguments[a] == edge.arguments[b] { return Err(invalid(span)); }
            edge.arguments.swap(a,b);
            super::build::owner_consumer_test_support::record_action("control-chain-link-fault-hit", 3);
        }
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
        for link in plans.links.iter().filter(|l| l.block == block) {
            let last = plans
                .group
                .iter()
                .find(|m| m.control == link.to_control)
                .ok_or_else(|| invalid(span))?;
            let key = (link.from_control, link.to_control);
            if self.links_observed.contains(&key) {
                return Err(invalid(span));
            }
            let TerminatorKind::Branch(edge) = term else {
                return Err(invalid(span));
            };
            if !matches!(original, RTerm::Goto(target) if *target == last.blocks[0]) {
                return Err(invalid(span));
            }
            #[cfg(test)]
            let corrupt = plans
                .links
                .last()
                .is_some_and(|last| last.to_control == link.to_control)
                && super::build::owner_consumer_test_support::take_fault(
                    super::build::owner_consumer_test_support::OwnerFault::ChainSsaWrongLinkTarget,
                );
            #[cfg(test)]
            if corrupt {
                super::build::owner_consumer_test_support::record_action(
                    "control-chain-link-fault-hit",
                    1,
                );
            }
            for (index, (source, target)) in link.pairs.iter().enumerate() {
                let expected = current.get(source).ok_or_else(|| invalid(span))?;
                let actual = current.get(target).ok_or_else(|| invalid(span))?;
                #[cfg(test)]
                let actual = if corrupt && index == 0 {
                    current.get(&link.pairs[1].0).ok_or_else(|| invalid(span))?
                } else {
                    actual
                };
                let _ = index;
                if !matches!(expected, EntityId::Value(_)) || expected != actual {
                    return Err(invalid(span));
                }
            }
            self.edge(body, last.blocks[0], edge, current, span)?;
            self.links_observed.insert(key);
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
        if !plans.active() {
            return Ok(());
        }
        let roles = plans.roles();
        if self.caller != Some(function.id())
            || self.links_observed.len() != plans.links.len()
            || plans.links.iter().any(|l| {
                !self
                    .links_observed
                    .contains(&(l.from_control, l.to_control))
            })
            || self.terms.len() != self.blocks.len()
            || self.replacements.len()
                != roles.iter().map(|(_, _, roots)| roots.len()).sum::<usize>()
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
            let block = roles
                .iter()
                .find(|(role, _, _)| role == side)
                .map(|(_, geometry, _)| geometry[1])
                .ok_or_else(|| invalid(span))?;
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
        for link in &plans.links {
            super::build::owner_consumer_test_support::record_action(
                "control-chain-link-complete",
                link.pairs.len(),
            );
        }
        #[cfg(test)]
        if let Some(p) = &plans.plan {
            super::build::owner_consumer_test_support::record_action(
                "bridge-ssa-receipt-complete",
                p.sources.len(),
            );
        } else {
            super::build::owner_consumer_test_support::record_action(
                "control-chain-ssa-complete",
                plans.group.len(),
            );
        }
        Ok(())
    }
}
