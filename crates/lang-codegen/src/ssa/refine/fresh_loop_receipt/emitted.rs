//! Freeze identities at actual append and terminator callbacks for fresh body leases.
use super::*;
use crate::ssa::model::{
    BlockId, Edge, EntityId, EntityType, Function, InstructionId, LoanKind, Operation, PlaceAccess,
    SsaTypeId, TerminatorKind,
};

use lang_frontend::type_checking::ParameterMode;

#[derive(Clone, Debug)]
pub(crate) struct Receipt {
    pub(crate) caller: FunctionId,
    pub(crate) loops: Vec<LoopReceipt>,
    pub(crate) segments: Vec<SegmentReceipt>,
    pub(crate) edges: Vec<(BlockId, Edge)>,
    pub(super) span: Span,
    pub(super) abi_types: Vec<SsaTypeId>,
    pub(super) abi_modes: Vec<ParameterMode>,
    pub(super) folds: Vec<(BlockRef, BlockRef)>,
    pub(super) blocks: BTreeMap<BlockRef, BlockId>,
    parameters: BTreeMap<BlockId, Vec<Slot>>,
}
#[derive(Clone, Debug)]
pub(crate) struct LoopReceipt {
    pub(crate) fact: UnitLoopNoCarryFact,
    pub(crate) instructions: Vec<(Tag, InstructionId)>,
    pub(super) proof: Proof,
}
#[derive(Clone, Debug)]
pub(crate) struct SegmentReceipt {
    pub(super) block: BlockRef,
    pub(super) consumers: Vec<SourceConsumer>,
    pub(crate) instructions: Vec<InstructionId>,
    pub(super) completed: usize,
    pub(super) pending: bool,
    pub(super) appended: bool,
    original_consumers: Vec<SourceConsumer>,
}
impl SegmentReceipt {
    pub(super) fn sealed(&self) -> bool {
        self.consumers == self.original_consumers
            && self.completed == self.consumers.len()
            && !self.pending
            && !self.appended
    }
}
#[derive(Clone, Debug)]
pub(super) struct Proof {
    pub(super) original_fact: UnitLoopNoCarryFact,
    pub(super) blocks: LoopBlocks,
    pub(super) edges: Vec<(BlockRef, BlockRef)>,
    pub(super) returns: Vec<BlockRef>,
    pub(super) source_parameter: usize,
    pub(super) flag_parameter: usize,
    pub(super) flag_owned: bool,
    pub(super) condition_used: bool,
    pub(super) source_type: SsaTypeId,
    pub(super) flag_type: SsaTypeId,
    pub(super) source_callee: FunctionId,
    pub(super) callees: Vec<FunctionId>,
    pub(super) condition_span: Span,
    pub(super) source_span: Span,
    pub(super) consumer_spans: Vec<Span>,
    pub(super) end_span: Span,
}
impl Receipt {
    #[cfg(all(test, feature = "loan-observation-test-support"))]
    pub(crate) fn recorded_fold_count(&self) -> usize {
        self.folds.len()
    }
    #[cfg(all(test, feature = "loan-observation-test-support"))]
    pub(crate) fn sequential_exit_links(&self) -> bool {
        self.loops.windows(2).all(|pair| {
            let from = pair[0].proof.blocks.exit;
            let to = pair[1].proof.blocks.entry;
            let actual = self.blocks.get(&self.canonical(from));
            from == to && actual.is_some() && actual == self.blocks.get(&self.canonical(to))
        })
    }
    pub(super) fn canonical(&self, mut block: BlockRef) -> BlockRef {
        for &(from, to) in &self.folds {
            if block == to {
                block = from;
            }
        }
        block
    }
}
#[derive(Default)]
pub(in crate::ssa::refine) struct Emitted {
    receipt: Option<Receipt>,
    expected: BTreeMap<BlockId, BTreeMap<Slot, EntityId>>,
    terminated: BTreeSet<BlockId>,
}
impl Emitted {
    pub(in crate::ssa::refine) fn new(
        plans: &Plans,
        function: &Function,
        span: Span,
    ) -> Result<Self, LoweringError> {
        plans.finish(span)?;
        if plans.loops.iter().any(|p| p.caller != function.id()) {
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
        let Some(first) = plans.loops.first() else {
            return Ok(());
        };
        if self.receipt.is_some() {
            return Err(invalid(span));
        }
        let mut expected = BTreeSet::new();
        for p in &plans.loops {
            let b = p.blocks.ok_or_else(|| invalid(span))?;
            expected.extend(
                [b.entry, b.header, b.body, b.finished, b.exit]
                    .into_iter()
                    .map(|b| plans.canonical(b)),
            );
        }
        if blocks.keys().copied().collect::<BTreeSet<_>>() != expected
            || parameters.len() != blocks.len()
            || entities.len() != blocks.len()
        {
            return Err(invalid(span));
        }
        let mut ps = BTreeMap::new();
        for (raw, id) in blocks {
            let slots = parameters.get(raw).ok_or_else(|| invalid(span))?;
            let ids = entities.get(raw).ok_or_else(|| invalid(span))?;
            if slots.len() != ids.len()
                || slots.iter().collect::<BTreeSet<_>>().len() != slots.len()
                || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
                || function.block(*id).map(|b| &b.parameters) != Some(ids)
            {
                return Err(invalid(span));
            }
            self.expected.insert(
                *id,
                slots.iter().copied().zip(ids.iter().copied()).collect(),
            );
            ps.insert(*id, slots.clone());
        }
        let entry = *blocks
            .get(&plans.canonical(first.blocks.ok_or_else(|| invalid(span))?.entry))
            .ok_or_else(|| invalid(span))?;
        let actual = function.block(entry).ok_or_else(|| invalid(span))?;
        let mappings = self.expected.get(&entry).ok_or_else(|| invalid(span))?;
        let abi_slots = plans.abi_slots.as_ref().ok_or_else(|| invalid(span))?;
        if abi_slots.len() != plans.abi_types.len()
            || abi_slots.len() != plans.abi_modes.len()
            || actual.parameters.len() != abi_slots.len()
            || abi_slots.iter().collect::<BTreeSet<_>>().len() != abi_slots.len()
        {
            return Err(invalid(span));
        }
        for (index, slot) in abi_slots.iter().enumerate() {
            let ty = match plans.abi_modes[index] {
                ParameterMode::Borrow => EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: plans.abi_types[index],
                },
                ParameterMode::Value => EntityType::Value(plans.abi_types[index]),
                _ => return Err(invalid(span)),
            };
            if mappings.get(slot) != actual.parameters.get(index)
                || actual
                    .parameters
                    .get(index)
                    .and_then(|id| function.entity(*id))
                    .map(|d| d.ty)
                    != Some(ty)
            {
                return Err(invalid(span));
            }
        }
        let mut loops = Vec::new();
        for p in &plans.loops {
            if p.source != abi_slots.get(p.source_parameter).copied()
                || p.flag != abi_slots.get(p.flag_parameter).copied()
                || plans.abi_types.get(p.source_parameter) != Some(&p.source_type)
                || plans.abi_types.get(p.flag_parameter) != Some(&p.flag_type)
                || plans.abi_modes.get(p.source_parameter) != Some(&ParameterMode::Borrow)
                || plans.abi_modes.get(p.flag_parameter)
                    != Some(&if p.flag_owned {
                        ParameterMode::Value
                    } else {
                        ParameterMode::Borrow
                    })
                || !p.condition_used
            {
                return Err(invalid(span));
            }
            let mut edges = Vec::new();
            let mut returns = Vec::new();
            for (raw, term) in &p.terms {
                match term {
                    RTerm::Goto(to) => edges.push((*raw, *to)),
                    RTerm::Branch {
                        when_true,
                        when_false,
                        ..
                    } => {
                        edges.push((*raw, *when_true));
                        edges.push((*raw, *when_false));
                    }
                    RTerm::Return(None) => returns.push(*raw),
                    _ => return Err(invalid(span)),
                }
            }
            loops.push(LoopReceipt {
                fact: p.fact.clone(),
                instructions: Vec::new(),
                proof: Proof {
                    original_fact: p.fact.clone(),
                    blocks: p.blocks.ok_or_else(|| invalid(span))?,
                    edges,
                    returns,
                    source_parameter: p.source_parameter,
                    flag_parameter: p.flag_parameter,
                    flag_owned: p.flag_owned,
                    condition_used: p.condition_used,
                    source_type: p.source_type,
                    flag_type: p.flag_type,
                    source_callee: p.source_callee,
                    callees: p.callees.clone(),
                    condition_span: p.condition_span,
                    source_span: p.source_span,
                    consumer_spans: p.consumer_spans.clone(),
                    end_span: p.end_span,
                },
            });
        }
        if plans.segments.len()
            != plans
                .loops
                .len()
                .checked_add(1)
                .ok_or_else(|| invalid(span))?
        {
            return Err(invalid(span));
        }
        let segments = plans
            .segments
            .iter()
            .map(|segment| {
                if segment.completed != segment.consumers.len()
                    || segment.pending
                    || segment.appended
                {
                    return Err(invalid(span));
                }
                Ok(SegmentReceipt {
                    block: segment.block.ok_or_else(|| invalid(span))?,
                    consumers: segment.consumers.clone(),
                    original_consumers: segment.consumers.clone(),
                    instructions: Vec::new(),
                    completed: segment.completed,
                    pending: segment.pending,
                    appended: segment.appended,
                })
            })
            .collect::<Result<Vec<_>, LoweringError>>()?;
        self.receipt = Some(Receipt {
            caller: first.caller,
            segments,
            loops,
            edges: Vec::new(),
            span,
            abi_types: plans.abi_types.clone(),
            abi_modes: plans.abi_modes.clone(),
            blocks: blocks.clone(),
            parameters: ps,
            folds: plans.folds.clone(),
        });
        Ok(())
    }
    pub(in crate::ssa::refine) fn statement(
        &mut self,
        plans: &Plans,
        tagged: (usize, Tag),
        function: &Function,
        instruction: InstructionId,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let (index, tag) = tagged;
        if let Tag::SourceConsumer(call) = tag {
            return self.source_statement(
                plans,
                (index, call),
                function,
                instruction,
                current,
                span,
            );
        }
        let p = plans.loops.get(index).ok_or_else(|| invalid(span))?;
        let r = self.receipt.as_mut().ok_or_else(|| invalid(span))?;
        if r.loops
            .iter()
            .flat_map(|l| &l.instructions)
            .any(|(_, id)| *id == instruction)
            || r.segments
                .iter()
                .any(|segment| segment.instructions.contains(&instruction))
        {
            return Err(invalid(span));
        }
        let local = r.loops.get_mut(index).ok_or_else(|| invalid(span))?;
        let d = function
            .instruction(instruction)
            .ok_or_else(|| invalid(span))?;
        if local
            .instructions
            .iter()
            .any(|(t, id)| *t == tag || *id == instruction)
            || !p.events.contains(&tag)
        {
            return Err(invalid(span));
        }
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
        let output = match tag {
            Tag::Condition => p.condition_value,
            Tag::Result => p.result,
            _ => None,
        };
        if let Some(slot) = output {
            let [id] = d.results.as_slice() else {
                return Err(invalid(span));
            };
            if expected.contains_key(&slot) || current.get(&slot) != Some(id) {
                return Err(invalid(span));
            }
            expected.insert(slot, *id);
        }
        let entity = |slot: Option<Slot>| {
            slot.and_then(|s| expected.get(&s).copied())
                .ok_or_else(|| invalid(span))
        };
        let empty = d.results.is_empty();
        let valid = match (tag, &d.operation) {
            (
                Tag::Condition,
                Operation::Read {
                    source: PlaceAccess::Loan(loan),
                },
            ) => EntityId::Loan(*loan) == entity(p.flag)? && d.results.len() == 1,
            (
                Tag::Result,
                Operation::BorrowCall {
                    callee,
                    source,
                    arguments,
                },
            ) => {
                *callee == p.source_callee
                    && EntityId::Loan(*source) == entity(p.source)?
                    && arguments.as_slice() == [entity(p.source)?]
                    && d.results.len() == 1
            }
            (
                Tag::Consumer(index),
                Operation::DirectCall {
                    callee,
                    receiver: None,
                    arguments,
                },
            ) => {
                p.callees.get(index) == Some(callee)
                    && arguments.as_slice() == [entity(p.result)?]
                    && empty
            }
            (Tag::ResultEnd, Operation::BorrowEnd { loan }) => {
                EntityId::Loan(*loan) == entity(p.result)? && empty
            }
            _ => false,
        };
        if !valid {
            return Err(invalid(span));
        }
        local.instructions.push((tag, instruction));
        Ok(())
    }
    fn source_statement(
        &mut self,
        plans: &Plans,
        indices: (usize, usize),
        function: &Function,
        instruction: InstructionId,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let (segment_index, call_index) = indices;
        let receipt = self.receipt.as_mut().ok_or_else(|| invalid(span))?;
        if receipt
            .loops
            .iter()
            .flat_map(|peer| &peer.instructions)
            .any(|(_, id)| *id == instruction)
            || receipt
                .segments
                .iter()
                .any(|segment| segment.instructions.contains(&instruction))
        {
            return Err(invalid(span));
        }
        let segment = receipt
            .segments
            .get(segment_index)
            .ok_or_else(|| invalid(span))?;
        let consumer = segment
            .consumers
            .get(call_index)
            .ok_or_else(|| invalid(span))?;
        let actual = function
            .instruction(instruction)
            .ok_or_else(|| invalid(span))?;
        let block = receipt
            .blocks
            .get(&plans.canonical(segment.block))
            .ok_or_else(|| invalid(span))?;
        let expected = self.expected.get(block).ok_or_else(|| invalid(span))?;
        let source_slot = plans
            .loops
            .first()
            .and_then(|plan| plan.source)
            .ok_or_else(|| invalid(span))?;
        let source = expected.get(&source_slot).ok_or_else(|| invalid(span))?;
        if segment.instructions.len() != call_index
            || !segment.sealed()
            || actual.block != *block
            || actual.origin.span() != consumer.span
            || !actual.results.is_empty()
            || expected
                .iter()
                .any(|(slot, id)| current.get(slot) != Some(id))
            || !matches!(&actual.operation, Operation::DirectCall { callee, receiver: None, arguments }
                if *callee == consumer.callee && arguments.as_slice() == [*source])
        {
            return Err(invalid(span));
        }
        receipt.segments[segment_index]
            .instructions
            .push(instruction);
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
        if plans.loops.is_empty() {
            return Ok(());
        }
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
        let candidates = plans
            .loops
            .iter()
            .flat_map(|p| &p.terms)
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
        if plans.loops.is_empty() {
            return Ok(());
        }
        plans.finish(span)?;
        let r = self.receipt.ok_or_else(|| invalid(span))?;
        if r.caller != function.id()
            || r.loops.len() != plans.loops.len()
            || r.segments.len() != plans.segments.len()
            || r.segments
                .iter()
                .zip(&plans.segments)
                .any(|(actual, plan)| {
                    !actual.sealed()
                        || actual.instructions.len() != plan.consumers.len()
                        || actual.consumers != plan.consumers
                        || Some(actual.block) != plan.block
                })
            || self.terminated.len() != r.blocks.len()
        {
            return Err(invalid(span));
        }
        for (local, p) in r.loops.iter().zip(&plans.loops) {
            if local.instructions.len() != p.events.len()
                || p.events.iter().any(|tag| {
                    local
                        .instructions
                        .iter()
                        .filter(|(actual, _)| actual == tag)
                        .count()
                        != 1
                })
            {
                return Err(invalid(span));
            }
        }
        r.validate(function)?;
        record(r);
        Ok(())
    }
}
