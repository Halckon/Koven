//! Consume producer tags plus actual SSA block/slot mappings, including every parent edge.
use super::*;
use crate::ssa::model::{EntityId, Function, Operation, PlaceAccess, TerminatorKind};
#[derive(Clone, Debug)]
struct Layout {
    blocks: BTreeMap<BlockRef, BlockId>,
    parameters: BTreeMap<BlockId, Vec<Slot>>,
    entities: BTreeMap<BlockId, Vec<EntityId>>,
    split_blocks: Vec<BlockId>,
    leaf_blocks: Vec<BlockId>,
    source: LoanId,
    flags: Vec<LoanId>,
    sources: Vec<LoanId>,
}
#[derive(Default)]
pub(in crate::ssa::refine) struct Emitted {
    layout: Option<Layout>,
    reads: Vec<Option<(InstructionId, ValueId)>>,
    calls: Vec<Option<(InstructionId, LoanId)>>,
    edges: Vec<Option<[Edge; 2]>>,
    returned: Vec<bool>,
    gotos: BTreeMap<BlockId, Edge>,
}
fn loan(value: Option<EntityId>, span: Span) -> Result<LoanId, LoweringError> {
    match value {
        Some(EntityId::Loan(id)) => Ok(id),
        _ => Err(invalid(span)),
    }
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
        let Some(p) = &plans.plan else {
            return Ok(Self::default());
        };
        let (splits, leaves) = p.counts();
        Ok(Self {
            reads: vec![None; splits],
            calls: vec![None; leaves],
            edges: vec![None; splits],
            returned: vec![false; leaves],
            layout: None,
            gotos: BTreeMap::new(),
        })
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
        plans.finish(span)?;
        let remaining = plans.remaining_edges().ok_or_else(|| invalid(span))?;
        let canonical = |b| plans.canonical(b);
        let expected_blocks = expected_blocks(plans, span)?;
        if self.layout.is_some()
            || blocks
                .keys()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                != expected_blocks
            || p.paths.last().and_then(|p| p.block) != p.splits[0].join
            || remaining.is_empty()
            || remaining.iter().any(|(_, target)| {
                remaining
                    .iter()
                    .filter(|(_, other)| other == target)
                    .count()
                    < 2
            })
        {
            return Err(invalid(span));
        }
        let resolve = |b: Option<BlockRef>| {
            b.and_then(|b| blocks.get(&canonical(b)).copied())
                .ok_or_else(|| invalid(span))
        };
        let mapped = |b: Option<BlockRef>, slot: Option<Slot>| -> Result<LoanId, LoweringError> {
            let b = canonical(b.ok_or_else(|| invalid(span))?);
            let slot = slot.ok_or_else(|| invalid(span))?;
            let slots = parameters.get(&b).ok_or_else(|| invalid(span))?;
            let values = entities.get(&b).ok_or_else(|| invalid(span))?;
            if slots.len() != values.len()
                || function.block(resolve(Some(b))?).map(|b| &b.parameters) != Some(values)
            {
                return Err(invalid(span));
            }
            let indices = slots
                .iter()
                .enumerate()
                .filter(|(_, s)| **s == slot)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            let [index] = indices.as_slice() else {
                return Err(invalid(span));
            };
            loan(values.get(*index).copied(), span)
        };
        let split_blocks = p
            .splits
            .iter()
            .map(|s| resolve(s.entry))
            .collect::<Result<Vec<_>, _>>()?;
        let leaf_blocks = p
            .paths
            .iter()
            .map(|p| resolve(p.block))
            .collect::<Result<Vec<_>, _>>()?;
        let unique = split_blocks
            .iter()
            .chain(&leaf_blocks)
            .collect::<std::collections::BTreeSet<_>>();
        if unique.len() != p.counts().0 + p.counts().1
            || function.blocks.first().map(|b| b.id) != Some(split_blocks[0])
        {
            return Err(invalid(span));
        }
        let source = mapped(p.splits[0].entry, p.source)?;
        let entry_flags = p
            .splits
            .iter()
            .map(|s| mapped(p.splits[0].entry, s.flag))
            .collect::<Result<Vec<_>, _>>()?;
        let flags = p
            .splits
            .iter()
            .map(|s| mapped(s.entry, s.flag))
            .collect::<Result<Vec<_>, _>>()?;
        let sources = p
            .paths
            .iter()
            .map(|p2| mapped(p2.block, p.source))
            .collect::<Result<Vec<_>, _>>()?;
        let expected_entry = std::iter::once(source)
            .chain(entry_flags)
            .map(EntityId::Loan)
            .collect::<Vec<_>>();
        if expected_entry
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != p.flags.len() + 1
            || function.block(split_blocks[0]).map(|b| &b.parameters) != Some(&expected_entry)
        {
            return Err(invalid(span));
        }
        for (i, block) in split_blocks.iter().enumerate().skip(1) {
            let mut expected = vec![mapped(p.splits[i].entry, p.source)?];
            for flag in p.needed_flags(i).ok_or_else(|| invalid(span))? {
                expected.push(mapped(p.splits[i].entry, p.splits[flag].flag)?);
            }
            let actual = function
                .block(*block)
                .ok_or_else(|| invalid(span))?
                .parameters
                .iter()
                .map(|e| loan(Some(*e), span))
                .collect::<Result<Vec<_>, _>>()?;
            if expected.len() != actual.len()
                || actual
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != actual.len()
                || expected
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != expected.len()
                || actual.iter().any(|v| !expected.contains(v))
            {
                return Err(invalid(span));
            }
        }
        if leaf_blocks.iter().zip(&sources).any(|(block, source)| {
            function.block(*block).map(|b| b.parameters.as_slice())
                != Some(&[EntityId::Loan(*source)][..])
        }) {
            return Err(invalid(span));
        }
        // Every continuing block is source-only, including retained multi-predecessor joins.
        for original in blocks
            .keys()
            .filter(|b| !p.splits.iter().any(|s| s.entry == Some(**b)))
        {
            let value = mapped(Some(*original), p.source)?;
            if function
                .block(resolve(Some(*original))?)
                .map(|b| b.parameters.as_slice())
                != Some(&[EntityId::Loan(value)][..])
            {
                return Err(invalid(span));
            }
        }
        let mut final_parameters = BTreeMap::new();
        let mut final_entities = BTreeMap::new();
        for (b, id) in blocks {
            final_parameters.insert(
                *id,
                parameters.get(b).cloned().ok_or_else(|| invalid(span))?,
            );
            final_entities.insert(*id, entities.get(b).cloned().ok_or_else(|| invalid(span))?);
        }
        self.layout = Some(Layout {
            blocks: blocks.clone(),
            parameters: final_parameters,
            entities: final_entities,
            split_blocks,
            leaf_blocks,
            source,
            flags,
            sources,
        });
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
        let l = self.layout.as_ref().ok_or_else(|| invalid(span))?;
        let data = function
            .instruction(instruction)
            .ok_or_else(|| invalid(span))?;
        if function.id() != p.caller {
            return Err(invalid(span));
        }
        match tag {
            Tag::Condition(i) => {
                let s = p.splits.get(i).ok_or_else(|| invalid(span))?;
                let [EntityId::Value(value)] = data.results.as_slice() else {
                    return Err(invalid(span));
                };
                if i >= p.counts().0
                    || self.reads[i].is_some()
                    || data.block != l.split_blocks[i]
                    || data.operation
                        != (Operation::Read {
                            source: PlaceAccess::Loan(l.flags[i]),
                        })
                    || s.value.and_then(|s| current.get(&s)) != Some(&EntityId::Value(*value))
                {
                    return Err(invalid(span));
                }
                self.reads[i] = Some((instruction, *value));
            }
            Tag::Call(i) => {
                let path = p.paths.get(i).ok_or_else(|| invalid(span))?;
                let source = loan(path.source.and_then(|s| current.get(&s).copied()), span)?;
                let result = loan(path.result.and_then(|s| current.get(&s).copied()), span)?;
                if i >= p.counts().1
                    || self.calls[i].is_some()
                    || data.block != l.leaf_blocks[i]
                    || source != l.sources[i]
                    || source == result
                    || data.results != [EntityId::Loan(result)]
                    || data.operation
                        != (Operation::BorrowCall {
                            callee: path.callee,
                            arguments: vec![EntityId::Loan(source)],
                            source,
                        })
                {
                    return Err(invalid(span));
                }
                self.calls[i] = Some((instruction, result));
            }
        }
        Ok(())
    }
    pub(in crate::ssa::refine) fn terminator(
        &mut self,
        plans: &Plans,
        function: &Function,
        block: BlockId,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &plans.plan else {
            return Ok(());
        };
        let l = self.layout.as_ref().ok_or_else(|| invalid(span))?;
        let term = &function
            .block(block)
            .and_then(|b| b.terminator.as_ref())
            .ok_or_else(|| invalid(span))?
            .kind;
        if let Some(i) = l.split_blocks.iter().position(|b| *b == block) {
            let (_, value) = self.reads[i].ok_or_else(|| invalid(span))?;
            let TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } = term
            else {
                return Err(invalid(span));
            };
            let arms = p.splits[i].arms.ok_or_else(|| invalid(span))?;
            if self.edges[i].is_some() || *condition != value {
                return Err(invalid(span));
            }
            for (edge, target) in [when_true, when_false].into_iter().zip(arms) {
                if l.blocks.get(&target) != Some(&edge.target) {
                    return Err(invalid(span));
                }
                let expected = l
                    .parameters
                    .get(&edge.target)
                    .ok_or_else(|| invalid(span))?
                    .iter()
                    .map(|s| current.get(s).copied().ok_or_else(|| invalid(span)))
                    .collect::<Result<Vec<_>, _>>()?;
                if edge.arguments != expected
                    || function.block(edge.target).map(|b| &b.parameters)
                        != l.entities.get(&edge.target)
                {
                    return Err(invalid(span));
                }
            }
            self.edges[i] = Some([when_true.clone(), when_false.clone()]);
        } else if let Some(i) = l.leaf_blocks.iter().position(|b| *b == block) {
            let (_, result) = self.calls[i].ok_or_else(|| invalid(span))?;
            if self.returned[i]
                || *term != (TerminatorKind::BorrowReturn { loan: result })
                || p.paths[i].result.and_then(|s| current.get(&s)) != Some(&EntityId::Loan(result))
            {
                return Err(invalid(span));
            }
            self.returned[i] = true;
        } else {
            let remaining = plans.remaining_edges().ok_or_else(|| invalid(span))?;
            let matches = remaining
                .iter()
                .filter(|(from, _)| l.blocks.get(from) == Some(&block))
                .collect::<Vec<_>>();
            let [(_, target)] = matches.as_slice() else {
                return Err(invalid(span));
            };
            let TerminatorKind::Branch(edge) = term else {
                return Err(invalid(span));
            };
            if self.gotos.contains_key(&block)
                || l.blocks.get(target) != Some(&edge.target)
                || l.parameters
                    .get(&edge.target)
                    .ok_or_else(|| invalid(span))?
                    .iter()
                    .map(|s| current.get(s).copied().ok_or_else(|| invalid(span)))
                    .collect::<Result<Vec<_>, _>>()?
                    != edge.arguments
                || function.block(edge.target).map(|b| &b.parameters)
                    != l.entities.get(&edge.target)
            {
                return Err(invalid(span));
            }
            self.gotos.insert(block, edge.clone());
        }
        Ok(())
    }
    pub(in crate::ssa::refine) fn finish(
        self,
        plans: &Plans,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &plans.plan else {
            return if self.layout.is_none()
                && self.reads.iter().all(Option::is_none)
                && self.calls.iter().all(Option::is_none)
                && self.edges.iter().all(Option::is_none)
                && self.returned.is_empty()
                && self.gotos.is_empty()
            {
                Ok(())
            } else {
                Err(invalid(span))
            };
        };
        plans.finish(span)?;
        if self.reads.len() != p.counts().0
            || self.edges.len() != p.counts().0
            || self.calls.len() != p.counts().1
        {
            return Err(invalid(span));
        }
        let l = self.layout.ok_or_else(|| invalid(span))?;
        if l.split_blocks.len() != p.counts().0
            || l.flags.len() != p.counts().0
            || l.leaf_blocks.len() != p.counts().1
            || l.sources.len() != p.counts().1
        {
            return Err(invalid(span));
        }

        if self.returned.len() != p.counts().1
            || self.returned.iter().any(|returned| !returned)
            || function.blocks.len() != l.blocks.len()
            || function.instructions.len() != p.counts().0 + p.counts().1
        {
            return Err(invalid(span));
        }
        let edges = self
            .edges
            .into_iter()
            .map(|edge| edge.ok_or_else(|| invalid(span)))
            .collect::<Result<Vec<_>, _>>()?;
        let mut splits = Vec::new();
        for (i, s) in p.splits.iter().enumerate() {
            let (read, value) = self.reads[i].ok_or_else(|| invalid(span))?;
            splits.push(SplitReceipt {
                split: s.split,
                condition: s.condition,
                block: l.split_blocks[i],
                flag: l.flags[i],
                read,
                value,
                edges: edges[i].clone(),
            });
        }
        let mut paths = Vec::new();
        for (i, path) in p.paths.iter().enumerate() {
            let (instruction, result) = self.calls[i].ok_or_else(|| invalid(span))?;
            paths.push(PathReceipt {
                control: path.control,
                value: path.value,
                ids: path.ids.clone(),
                callee: path.callee,
                block: l.leaf_blocks[i],
                instruction,
                source: l.sources[i],
                result,
                edges: path
                    .route
                    .iter()
                    .copied()
                    .map(|(split, arm)| edges[split][arm].clone())
                    .collect(),
            });
        }
        let remaining = plans.remaining_edges().ok_or_else(|| invalid(span))?;
        if self.gotos.len() != remaining.len() {
            return Err(invalid(span));
        }
        let continuation_edges = remaining
            .iter()
            .map(|(from, to)| {
                let from = *l.blocks.get(from).ok_or_else(|| invalid(span))?;
                let edge = self
                    .gotos
                    .get(&from)
                    .cloned()
                    .ok_or_else(|| invalid(span))?;
                if l.blocks.get(to) != Some(&edge.target) {
                    return Err(invalid(span));
                }
                Ok((from, edge))
            })
            .collect::<Result<Vec<_>, LoweringError>>()?;
        let final_tail = *l.leaf_blocks.last().ok_or_else(|| invalid(span))?;
        let origins = p
            .hole_routes
            .iter()
            .map(|route| {
                let mut edges = route
                    .iter()
                    .map(|(i, arm)| splits[*i].edges[*arm].clone())
                    .collect::<Vec<_>>();
                let mut block = edges.last().ok_or_else(|| invalid(span))?.target;
                let mut visited = std::collections::BTreeSet::new();
                while block != final_tail {
                    if !visited.insert(block) || visited.len() > p.splits.len() {
                        return Err(invalid(span));
                    }
                    let edge = self.gotos.get(&block).ok_or_else(|| invalid(span))?.clone();
                    block = edge.target;
                    edges.push(edge);
                }
                Ok(edges)
            })
            .collect::<Result<Vec<_>, LoweringError>>()?;
        let receipt = Receipt {
            blocks: l.blocks.clone(),
            continuation_edges,
            origins,
            original_gotos: p.gotos.clone(),
            folds: plans.folds.clone(),
            original_tail: p.splits[0].join.ok_or_else(|| invalid(span))?,
            final_tail: *l.leaf_blocks.last().ok_or_else(|| invalid(span))?,
            callable: p.callable,
            caller: p.caller,
            entry: l.split_blocks[0],
            source: l.source,
            splits,
            paths,
        };
        receipt.validate(plans, function, span)?;
        #[cfg(test)]
        ci_tests::record(plans, function, &receipt, span);
        #[cfg(test)]
        RECEIPTS.with(|v| {
            if let Some(v) = v.borrow_mut().as_mut() {
                v.pending.push(receipt);
            }
        });
        Ok(())
    }
}
impl Receipt {
    pub(super) fn validate(
        &self,
        plans: &Plans,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        plans.finish(span)?;
        let p = plans.plan.as_ref().ok_or_else(|| invalid(span))?;
        let (s, l) = p.counts();
        let remaining = plans.remaining_edges().ok_or_else(|| invalid(span))?;
        let expected_refs = expected_blocks(plans, span)?;
        if remaining.is_empty()
            || remaining.iter().any(|(_, target)| {
                remaining
                    .iter()
                    .filter(|(_, other)| other == target)
                    .count()
                    < 2
            })
        {
            return Err(invalid(span));
        }
        if self.original_gotos != p.gotos
            || self.folds != plans.folds
            || Some(self.original_tail) != p.splits[0].join
            || p.paths.last().and_then(|p| p.block) != Some(self.original_tail)
            || self.blocks.get(&plans.canonical(self.original_tail)) != Some(&self.final_tail)
            || self.paths.last().map(|p| p.block) != Some(self.final_tail)
            || self
                .blocks
                .keys()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                != expected_refs
            || self
                .blocks
                .values()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.blocks.len()
            || self
                .blocks
                .values()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                != function.blocks.iter().map(|b| b.id).collect()
            || self.callable != p.callable
            || self.caller != p.caller
            || self.caller != function.id()
            || self.splits.len() != s
            || self.paths.len() != l
            || function.blocks.len() != expected_refs.len()
            || function.instructions.len() != s + l
            || function.blocks.first().map(|b| b.id) != Some(self.entry)
            || self.splits.first().map(|s| s.block) != Some(self.entry)
            || self.continuation_edges.len() != remaining.len()
            || self.origins.len() != p.hole_routes.len()
        {
            return Err(invalid(span));
        }
        for ((from, to), (block, edge)) in remaining.iter().zip(&self.continuation_edges) {
            let actual = function.block(*block).ok_or_else(|| invalid(span))?;
            if self.blocks.get(from) != Some(block)
                || self.blocks.get(to) != Some(&edge.target)
                || actual.terminator.as_ref().map(|t| &t.kind)
                    != Some(&TerminatorKind::Branch(edge.clone()))
                || !actual.instructions.is_empty()
                || actual.parameters.len() != 1
                || edge.arguments != actual.parameters
                || function
                    .block(edge.target)
                    .is_none_or(|b| b.parameters.len() != 1)
            {
                return Err(invalid(span));
            }
            loan(actual.parameters.first().copied(), span)?;
            loan(
                function
                    .block(edge.target)
                    .and_then(|b| b.parameters.first())
                    .copied(),
                span,
            )?;
        }
        let mapped = |edge: &Edge, source: LoanId| -> Result<LoanId, LoweringError> {
            let positions = edge
                .arguments
                .iter()
                .enumerate()
                .filter(|(_, e)| **e == EntityId::Loan(source))
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            let [i] = positions.as_slice() else {
                return Err(invalid(span));
            };
            loan(
                function
                    .block(edge.target)
                    .and_then(|b| b.parameters.get(*i))
                    .copied(),
                span,
            )
        };
        let entry = function.block(self.entry).ok_or_else(|| invalid(span))?;
        if entry.parameters.len() != s + 1
            || loan(entry.parameters.first().copied(), span)? != self.source
        {
            return Err(invalid(span));
        }
        let mut ids = std::collections::BTreeSet::new();
        for (i, split) in self.splits.iter().enumerate() {
            if !ids.insert(split.block) {
                return Err(invalid(span));
            }
            let route = p.split_route(i).ok_or_else(|| invalid(span))?;
            let mut flag = loan(entry.parameters.get(i + 1).copied(), span)?;
            let mut source = self.source;
            for (parent, arm) in &route {
                let edge = &self.splits[*parent].edges[*arm];
                flag = mapped(edge, flag)?;
                source = mapped(edge, source)?;
            }
            if route
                .last()
                .is_some_and(|(parent, arm)| self.splits[*parent].edges[*arm].target != split.block)
                || split.flag != flag
            {
                return Err(invalid(span));
            }
            let mut expected = vec![source];
            for j in p.needed_flags(i).ok_or_else(|| invalid(span))? {
                let mut value = loan(entry.parameters.get(j + 1).copied(), span)?;
                for (parent, arm) in &route {
                    value = mapped(&self.splits[*parent].edges[*arm], value)?;
                }
                expected.push(value);
            }
            let block = function.block(split.block).ok_or_else(|| invalid(span))?;
            let actual = block
                .parameters
                .iter()
                .map(|e| loan(Some(*e), span))
                .collect::<Result<Vec<_>, _>>()?;
            if actual.len() != expected.len()
                || actual
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != actual.len()
                || expected
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != expected.len()
                || actual.iter().any(|v| !expected.contains(v))
            {
                return Err(invalid(span));
            }
            let read = function
                .instruction(split.read)
                .ok_or_else(|| invalid(span))?;
            if self
                .blocks
                .get(&plans.canonical(p.splits[i].entry.ok_or_else(|| invalid(span))?))
                != Some(&split.block)
                || p.splits[i]
                    .arms
                    .ok_or_else(|| invalid(span))?
                    .iter()
                    .zip(&split.edges)
                    .any(|(arm, edge)| {
                        self.blocks.get(&plans.canonical(*arm)) != Some(&edge.target)
                    })
                || split.split != p.splits[i].split
                || split.condition != p.splits[i].condition
                || read.block != split.block
                || !block.instructions.contains(&split.read)
                || read.results != [EntityId::Value(split.value)]
                || read.operation
                    != (Operation::Read {
                        source: PlaceAccess::Loan(flag),
                    })
                || block.terminator.as_ref().map(|t| &t.kind)
                    != Some(&TerminatorKind::Conditional {
                        condition: split.value,
                        when_true: split.edges[0].clone(),
                        when_false: split.edges[1].clone(),
                    })
            {
                return Err(invalid(span));
            }
        }
        for (i, path) in self.paths.iter().enumerate() {
            if !ids.insert(path.block) {
                return Err(invalid(span));
            }
            let expected = p.paths[i]
                .route
                .iter()
                .map(|(s, a)| self.splits[*s].edges[*a].clone())
                .collect::<Vec<_>>();
            let mut source = self.source;
            for edge in &path.edges {
                source = mapped(edge, source)?;
            }
            if i + 1 == l {
                source = loan(
                    function
                        .block(self.final_tail)
                        .and_then(|b| b.parameters.first())
                        .copied(),
                    span,
                )?;
            }
            let call = function
                .instruction(path.instruction)
                .ok_or_else(|| invalid(span))?;
            let block = function.block(path.block).ok_or_else(|| invalid(span))?;
            if path.control != p.paths[i].control
                || path.value != p.paths[i].value
                || path.ids != p.paths[i].ids
                || path.callee != p.paths[i].callee
                || path.edges != expected
                || (i + 1 != l && path.edges.last().map(|e| e.target) != Some(path.block))
                || self
                    .blocks
                    .get(&plans.canonical(p.paths[i].block.ok_or_else(|| invalid(span))?))
                    != Some(&path.block)
                || path.source != source
                || path.result == source
                || call.block != path.block
                || !block.instructions.contains(&path.instruction)
                || block.parameters != [EntityId::Loan(source)]
                || call.results != [EntityId::Loan(path.result)]
                || call.operation
                    != (Operation::BorrowCall {
                        callee: path.callee,
                        arguments: vec![EntityId::Loan(source)],
                        source,
                    })
                || block.terminator.as_ref().map(|t| &t.kind)
                    != Some(&TerminatorKind::BorrowReturn { loan: path.result })
            {
                return Err(invalid(span));
            }
        }
        let mut traversed = std::collections::BTreeSet::new();
        for (route, actual) in p.hole_routes.iter().zip(&self.origins) {
            let mut expected = route
                .iter()
                .map(|(split, arm)| self.splits[*split].edges[*arm].clone())
                .collect::<Vec<_>>();
            let mut block = expected.last().ok_or_else(|| invalid(span))?.target;
            let mut seen = std::collections::BTreeSet::new();
            while block != self.final_tail {
                if !seen.insert(block) || seen.len() > s {
                    return Err(invalid(span));
                }
                let matches = self
                    .continuation_edges
                    .iter()
                    .filter(|(from, _)| *from == block)
                    .collect::<Vec<_>>();
                let [(_, edge)] = matches.as_slice() else {
                    return Err(invalid(span));
                };
                traversed.insert(block);
                block = edge.target;
                expected.push(edge.clone());
            }
            if *actual != expected {
                return Err(invalid(span));
            }
            let mut source = self.source;
            for edge in actual {
                source = mapped(edge, source)?;
            }
            if self.paths.last().map(|p| p.source) != Some(source) {
                return Err(invalid(span));
            }
        }
        if traversed.len() != self.continuation_edges.len() {
            return Err(invalid(span));
        }
        // No unclassified live block or extra instruction can hide at a continuing join.
        ids.extend(self.continuation_edges.iter().map(|(from, _)| *from));
        if ids != function.blocks.iter().map(|b| b.id).collect() {
            return Err(invalid(span));
        }
        Ok(())
    }
}

fn expected_blocks(
    plans: &Plans,
    span: Span,
) -> Result<std::collections::BTreeSet<BlockRef>, LoweringError> {
    let p = plans.plan.as_ref().ok_or_else(|| invalid(span))?;
    let mut blocks = std::collections::BTreeSet::new();
    for split in &p.splits {
        blocks.insert(plans.canonical(split.entry.ok_or_else(|| invalid(span))?));
        for arm in split.arms.ok_or_else(|| invalid(span))? {
            blocks.insert(plans.canonical(arm));
        }
    }
    for (_, target) in &p.gotos {
        blocks.insert(plans.canonical(*target));
    }
    Ok(blocks)
}

#[cfg(test)]
mod ci_tests;
