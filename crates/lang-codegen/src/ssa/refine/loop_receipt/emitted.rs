//! Consume actual loop producer events and independently verify final edge transport.
use super::*;
use crate::ssa::model::{
    BlockId, Edge, EntityId, Function, InstructionId, LoanKind, Operation, PlaceAccess,
    TerminatorKind,
};

#[derive(Clone, Debug)]
pub(crate) struct Receipt {
    pub(crate) lineage: UnitLoanLoopInvariantLineage,
    pub(crate) caller: FunctionId,
    pub(crate) instructions: Vec<(Tag, InstructionId)>,
    pub(crate) edges: Vec<(BlockId, Edge)>,
    pub(super) blocks: BTreeMap<BlockRef, BlockId>,
    pub(super) parameters: BTreeMap<BlockId, Vec<Slot>>,
    pub(crate) entities: BTreeMap<BlockId, Vec<EntityId>>,
}
impl Receipt {
    #[cfg(test)]
    pub(crate) fn parameter_count(&self, block: BlockId) -> Option<usize> {
        self.parameters.get(&block).map(Vec::len)
    }
}
#[derive(Default)]
pub(in crate::ssa::refine) struct Emitted {
    receipt: Option<Receipt>,
    terminated: BTreeSet<BlockId>,
    owner_aliased: bool,
    expected: BTreeMap<BlockId, BTreeMap<Slot, EntityId>>,
}
impl Emitted {
    pub(in crate::ssa::refine) fn new(
        plans: &Plans,
        function: &Function,
        span: Span,
    ) -> Result<Self, LoweringError> {
        plans.finish(span)?;
        if plans
            .plan
            .as_ref()
            .is_some_and(|p| p.caller != function.id())
        {
            return Err(invalid(span));
        }
        Ok(Self::default())
    }
    pub(in crate::ssa::refine) fn layout(
        &mut self,
        plans: &Plans,
        function: &Function,
        blocks: &BTreeMap<BlockRef, BlockId>,
        parameters: &BTreeMap<BlockRef, Vec<Slot>>,
        entities: &BTreeMap<BlockRef, Vec<EntityId>>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &plans.plan else {
            return Ok(());
        };
        if self.receipt.is_some() {
            return Err(invalid(span));
        }
        let b = p.blocks.ok_or_else(|| invalid(span))?;
        let expected = [b.entry, b.header, b.body, b.finished, b.exit]
            .into_iter()
            .map(|b| plans.canonical(b))
            .collect::<BTreeSet<_>>();
        if blocks.keys().copied().collect::<BTreeSet<_>>() != expected {
            return Err(invalid(span));
        }
        let mut ps = BTreeMap::new();
        let mut es = BTreeMap::new();
        for (key, id) in blocks {
            let slots = parameters.get(key).ok_or_else(|| invalid(span))?;
            let values = entities.get(key).ok_or_else(|| invalid(span))?;
            if slots.len() != values.len()
                || slots.iter().collect::<BTreeSet<_>>().len() != slots.len()
                || values.iter().collect::<BTreeSet<_>>().len() != values.len()
                || function.block(*id).map(|b| &b.parameters) != Some(values)
            {
                return Err(invalid(span));
            }
            self.expected.insert(
                *id,
                slots.iter().copied().zip(values.iter().copied()).collect(),
            );
            ps.insert(*id, slots.clone());
            es.insert(*id, values.clone());
        }
        let entry = blocks
            .get(&plans.canonical(b.entry))
            .ok_or_else(|| invalid(span))?;
        let flag = p.flag.ok_or_else(|| invalid(span))?;
        let expected_flag = function
            .block(*entry)
            .and_then(|b| b.parameters.get(p.flag_parameter))
            .copied()
            .ok_or_else(|| invalid(span))?;
        if !matches!(expected_flag, EntityId::Loan(_))
            || self.expected.get(entry).and_then(|m| m.get(&flag)) != Some(&expected_flag)
        {
            return Err(invalid(span));
        }
        self.receipt = Some(Receipt {
            lineage: p.lineage.clone(),
            caller: p.caller,
            instructions: Vec::new(),
            edges: Vec::new(),
            blocks: blocks.clone(),
            parameters: ps,
            entities: es,
        });
        Ok(())
    }
    pub(in crate::ssa::refine) fn alias(
        &mut self,
        plans: &Plans,
        source: Slot,
        result: Slot,
        target: BlockId,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &plans.plan else {
            return Ok(());
        };
        let r = self.receipt.as_ref().ok_or_else(|| invalid(span))?;
        let b = p.blocks.ok_or_else(|| invalid(span))?;
        if self.owner_aliased
            || p.owner_alias != Some((source, result))
            || p.literal != Some(source)
            || p.owner != Some(result)
            || source == result
            || r.blocks.get(&plans.canonical(b.entry)) != Some(&target)
            || r.instructions.len() != 1
            || r.instructions[0].0 != Tag::Owner
        {
            return Err(invalid(span));
        }
        let expected = self
            .expected
            .get_mut(&target)
            .ok_or_else(|| invalid(span))?;
        let id = *expected.get(&source).ok_or_else(|| invalid(span))?;
        if !matches!(id, EntityId::Value(_))
            || expected.contains_key(&result)
            || current.get(&source) != Some(&id)
            || current.get(&result) != Some(&id)
            || expected.iter().any(|(s, id)| current.get(s) != Some(id))
        {
            return Err(invalid(span));
        }
        expected.insert(result, id);
        self.owner_aliased = true;
        Ok(())
    }
    pub(in crate::ssa::refine) fn statement(
        &mut self,
        plans: &Plans,
        tag: Tag,
        function: &Function,
        instruction: InstructionId,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let p = plans.plan.as_ref().ok_or_else(|| invalid(span))?;
        let r = self.receipt.as_mut().ok_or_else(|| invalid(span))?;
        let d = function
            .instruction(instruction)
            .ok_or_else(|| invalid(span))?;
        if r.instructions.iter().any(|(t, _)| *t == tag) || !p.events.contains(&tag) {
            return Err(invalid(span));
        }
        let output_slot = match tag {
            Tag::Owner => p.literal,
            Tag::Place => p.place,
            Tag::Source => p.source,
            Tag::Result => p.result,
            Tag::Condition => p.condition_value,
            _ => None,
        };
        let expected = self
            .expected
            .get_mut(&d.block)
            .ok_or_else(|| invalid(span))?;
        if expected
            .iter()
            .any(|(slot, id)| current.get(slot) != Some(id))
        {
            return Err(invalid(span));
        }
        if let Some(slot) = output_slot {
            let [id] = d.results.as_slice() else {
                return Err(invalid(span));
            };
            if expected.contains_key(&slot)
                || current.get(&slot) != Some(id)
                || expected.values().any(|v| v == id)
            {
                return Err(invalid(span));
            }
            expected.insert(slot, *id);
        } else if !d.results.is_empty() {
            return Err(invalid(span));
        }
        let e = |s: Option<Slot>| {
            s.and_then(|s| expected.get(&s).copied())
                .ok_or_else(|| invalid(span))
        };
        let b = p.blocks.ok_or_else(|| invalid(span))?;
        let original = match tag {
            Tag::Owner | Tag::Place | Tag::Source | Tag::Result => b.entry,
            Tag::Condition => b.header,
            Tag::Consumer(_) => b.body,
            Tag::ResultEnd | Tag::SourceEnd | Tag::OwnerDrop => b.exit,
        };
        if r.blocks.get(&plans.canonical(original)) != Some(&d.block) {
            return Err(invalid(span));
        }
        let predecessors = match tag {
            Tag::Owner => Vec::new(),
            Tag::Place => vec![Tag::Owner],
            Tag::Source => vec![Tag::Place],
            Tag::Result => vec![Tag::Source],
            Tag::Consumer(i) if i > 0 => vec![Tag::Consumer(i - 1)],
            Tag::SourceEnd => vec![Tag::ResultEnd],
            Tag::OwnerDrop => vec![Tag::SourceEnd],
            _ => Vec::new(),
        };
        if predecessors
            .iter()
            .any(|t| !r.instructions.iter().any(|(seen, _)| seen == t))
        {
            return Err(invalid(span));
        }
        let empty = d.results.is_empty();
        let valid = match (tag, &d.operation) {
            (Tag::Owner, Operation::StringLiteral { .. }) => {
                d.results.as_slice() == [e(p.literal)?]
            }
            (Tag::Place, Operation::RootPlace { owner }) => {
                EntityId::Value(*owner) == e(p.owner)? && d.results.as_slice() == [e(p.place)?]
            }
            (
                Tag::Source,
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
            ) => EntityId::Place(*place) == e(p.place)? && d.results.as_slice() == [e(p.source)?],
            (
                Tag::Result,
                Operation::BorrowCall {
                    callee,
                    arguments,
                    source,
                },
            ) => {
                *callee == p.source_callee
                    && arguments.as_slice() == [e(p.source)?]
                    && EntityId::Loan(*source) == e(p.source)?
                    && d.results.as_slice() == [e(p.result)?]
            }
            (
                Tag::Condition,
                Operation::Read {
                    source: PlaceAccess::Loan(loan),
                },
            ) => {
                EntityId::Loan(*loan) == e(p.flag)?
                    && d.results.as_slice() == [e(p.condition_value)?]
                    && matches!(d.results.as_slice(), [EntityId::Value(_)])
            }
            (
                Tag::Consumer(i),
                Operation::DirectCall {
                    callee,
                    receiver: None,
                    arguments,
                },
            ) => {
                p.callees.get(i) == Some(callee) && arguments.as_slice() == [e(p.result)?] && empty
            }
            (Tag::ResultEnd, Operation::BorrowEnd { loan }) => {
                EntityId::Loan(*loan) == e(p.result)? && empty
            }
            (Tag::SourceEnd, Operation::BorrowEnd { loan }) => {
                EntityId::Loan(*loan) == e(p.source)? && empty
            }
            (Tag::OwnerDrop, Operation::Drop { owner }) => {
                EntityId::Value(*owner) == e(p.owner)? && empty
            }
            _ => false,
        };
        if !valid {
            return Err(invalid(span));
        }
        r.instructions.push((tag, instruction));
        Ok(())
    }
    pub(in crate::ssa::refine) fn terminator(
        &mut self,
        plans: &Plans,
        function: &Function,
        target: BlockId,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &plans.plan else {
            return Ok(());
        };
        let r = self.receipt.as_mut().ok_or_else(|| invalid(span))?;
        if !self.terminated.insert(target) {
            return Err(invalid(span));
        }
        let expected = self.expected.get(&target).ok_or_else(|| invalid(span))?;
        if expected
            .iter()
            .any(|(slot, id)| current.get(slot) != Some(id))
        {
            return Err(invalid(span));
        }
        let candidates = p
            .terms
            .iter()
            .filter(|(b, t)| {
                r.blocks.get(&plans.canonical(*b)) == Some(&target)
                    && !matches!(t,RTerm::Goto(to) if plans.canonical(*b)==plans.canonical(*to))
            })
            .collect::<Vec<_>>();
        let [(_, original)] = candidates.as_slice() else {
            return Err(invalid(span));
        };
        let actual = &function
            .block(target)
            .and_then(|b| b.terminator.as_ref())
            .ok_or_else(|| invalid(span))?
            .kind;
        let edge = |actual: &Edge, to: BlockRef| -> Result<(), LoweringError> {
            let expected_target = r
                .blocks
                .get(&plans.canonical(to))
                .ok_or_else(|| invalid(span))?;
            let slots = r
                .parameters
                .get(expected_target)
                .ok_or_else(|| invalid(span))?;
            let args = slots
                .iter()
                .map(|s| expected.get(s).copied())
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| invalid(span))?;
            if actual.target != *expected_target || actual.arguments != args {
                return Err(invalid(span));
            }
            Ok(())
        };
        let edges = match (original, actual) {
            (RTerm::Goto(to), TerminatorKind::Branch(e)) => {
                edge(e, *to)?;
                vec![e.clone()]
            }
            (
                RTerm::Branch {
                    condition,
                    when_true,
                    when_false,
                },
                TerminatorKind::Conditional {
                    condition: c,
                    when_true: t,
                    when_false: f,
                },
            ) => {
                if expected.get(condition) != Some(&EntityId::Value(*c)) {
                    return Err(invalid(span));
                }
                edge(t, *when_true)?;
                edge(f, *when_false)?;
                vec![t.clone(), f.clone()]
            }
            (RTerm::Return(None), TerminatorKind::Return { values }) if values.is_empty() => {
                Vec::new()
            }
            _ => return Err(invalid(span)),
        };
        r.edges.extend(edges.into_iter().map(|e| (target, e)));
        Ok(())
    }
    pub(in crate::ssa::refine) fn finish(
        self,
        plans: &Plans,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &plans.plan else {
            return Ok(());
        };
        plans.finish(span)?;
        let r = self.receipt.ok_or_else(|| invalid(span))?;
        if !self.owner_aliased
            || r.caller != function.id()
            || r.instructions.len() != p.events.len()
            || self.terminated.len() != r.blocks.len()
            || p.events
                .iter()
                .any(|t| r.instructions.iter().filter(|(a, _)| a == t).count() != 1)
        {
            return Err(invalid(span));
        }
        let actual = r
            .blocks
            .values()
            .map(|id| function.block(*id).map(|b| b.instructions.clone()))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| invalid(span))?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let ids = r
            .instructions
            .iter()
            .map(|(_, id)| *id)
            .collect::<BTreeSet<_>>();
        if actual.len() != ids.len() || actual.iter().copied().collect::<BTreeSet<_>>() != ids {
            return Err(invalid(span));
        }
        #[cfg(test)]
        ci_tests::record(plans, function, std::slice::from_ref(&r), span);
        record(r);
        Ok(())
    }
}

#[cfg(test)]
#[path = "ci_tests.rs"]
mod ci_tests;
