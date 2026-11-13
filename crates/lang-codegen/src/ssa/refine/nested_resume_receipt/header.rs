//! Whole parent-header parameter transport. This is not a parent exit/induction proof.
use super::*;
use std::collections::BTreeSet;

pub(super) struct Header {
    #[cfg(test)]
    root: Slot,
    function: FunctionId,
    target: BlockId,
    slots: Vec<Slot>,
    parameters: Vec<EntityId>,
    predecessors: BTreeSet<BlockId>,
    seen: BTreeMap<BlockId, Vec<EntityId>>,
}
impl Header {
    pub(super) fn select(
        body: &RefinedBody,
        p: &Plan,
        function: &Function,
        blocks: &BTreeMap<BlockRef, BlockId>,
        slots: &BTreeMap<BlockRef, Vec<Slot>>,
        parameters: &BTreeMap<BlockRef, Vec<EntityId>>,
        span: Span,
    ) -> Result<Option<Self>, LoweringError> {
        let Some(carry) = &p.carry else {
            return Ok(None);
        };
        let fail = || invalid(span);
        let target = *blocks.get(&carry.header).ok_or_else(fail)?;
        let mut predecessors = BTreeSet::new();
        for (raw, id) in blocks {
            let b = body.blocks.get(raw.0).ok_or_else(fail)?;
            let (term, _) = b.terminator.as_ref().ok_or_else(fail)?;
            let count = term
                .successors()
                .iter()
                .filter(|b| **b == carry.header)
                .count();
            if count > 1 {
                return Err(fail());
            }
            if count == 1 {
                predecessors.insert(*id);
            }
        }
        // Only the existing two-predecessor direct-parent shape is selected.
        if predecessors.len() != 2 {
            return Ok(None);
        }
        let (backedge, end) = carry.edges.last().ok_or_else(fail)?;
        if *end != carry.header || !predecessors.contains(blocks.get(backedge).ok_or_else(fail)?) {
            return Err(fail());
        }
        let ss = slots.get(&carry.header).ok_or_else(fail)?;
        let es = parameters.get(&carry.header).ok_or_else(fail)?;
        if ss.is_empty()
            || ss.len() != es.len()
            || ss.iter().collect::<BTreeSet<_>>().len() != ss.len()
            || es.iter().collect::<BTreeSet<_>>().len() != es.len()
            || function.block(target).is_none_or(|b| b.parameters != *es)
        {
            return Err(fail());
        }
        Ok(Some(Self {
            #[cfg(test)]
            root: p.root,
            function: function.id(),
            target,
            slots: ss.clone(),
            parameters: es.clone(),
            predecessors,
            seen: BTreeMap::new(),
        }))
    }
    pub(super) fn observe(
        &mut self,
        function: &Function,
        from: BlockId,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if !self.predecessors.contains(&from) {
            return Ok(());
        }
        let fail = || invalid(span);
        let args = self
            .slots
            .iter()
            .map(|s| current.get(s).copied().ok_or_else(fail))
            .collect::<Result<Vec<_>, _>>()?;
        if function.id() != self.function || self.seen.insert(from, args.clone()).is_some() {
            return Err(fail());
        }
        self.check_edge(function, from, &args, span)
    }
    fn check_edge(
        &self,
        function: &Function,
        from: BlockId,
        args: &[EntityId],
        span: Span,
    ) -> Result<(), LoweringError> {
        let fail = || invalid(span);
        let term = &function
            .block(from)
            .ok_or_else(fail)?
            .terminator
            .as_ref()
            .ok_or_else(fail)?
            .kind;
        let edges = match term {
            TerminatorKind::Branch(e) => vec![e],
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => vec![when_true, when_false],
            _ => Vec::new(),
        };
        let matching = edges
            .into_iter()
            .filter(|e| e.target == self.target)
            .collect::<Vec<_>>();
        let [edge] = matching.as_slice() else {
            return Err(fail());
        };
        if edge.arguments != args {
            return Err(fail());
        }
        Ok(())
    }
    pub(super) fn finish(&self, function: &Function, span: Span) -> Result<(), LoweringError> {
        let fail = || invalid(span);
        if function.id() != self.function
            || self.seen.keys().copied().collect::<BTreeSet<_>>() != self.predecessors
            || function
                .block(self.target)
                .is_none_or(|b| b.parameters != self.parameters)
        {
            return Err(fail());
        }
        let mut actual = BTreeSet::new();
        for b in &function.blocks {
            let Some(t) = &b.terminator else {
                return Err(fail());
            };
            if matches!(t.kind, TerminatorKind::NullableBranch { .. })
                && t.kind.targets().contains(&self.target)
            {
                return Err(fail());
            }
            let edges = match &t.kind {
                TerminatorKind::Branch(e) => vec![e],
                TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } => vec![when_true, when_false],
                _ => Vec::new(),
            };
            for edge in edges.into_iter().filter(|e| e.target == self.target) {
                if !actual.insert(b.id) || self.seen.get(&b.id) != Some(&edge.arguments) {
                    return Err(fail());
                }
            }
        }
        if actual != self.predecessors {
            return Err(fail());
        }
        for (from, args) in &self.seen {
            self.check_edge(function, *from, args, span)?;
        }
        #[cfg(test)]
        super::super::build::owner_consumer_test_support::record_action(
            "nested-parent-header-vector-complete",
            self.parameters.len(),
        );
        Ok(())
    }
}

#[cfg(test)]
impl Header {
    pub(super) fn inject(&self, function: &mut Function) {
        use super::super::build::owner_consumer_test_support::{
            OwnerFault, record_action, take_fault,
        };
        let mut injected = false;
        let from = *self.predecessors.first().unwrap();
        for fault in [
            OwnerFault::NestedHeaderSwapArguments,
            OwnerFault::NestedHeaderMissingArgument,
        ] {
            if take_fault(fault) {
                let root = self.slots.iter().position(|s| *s == self.root).unwrap();
                let root_type = function.entity(self.parameters[root]).unwrap().ty;
                let peer = self
                    .parameters
                    .iter()
                    .enumerate()
                    .find_map(|(i, e)| {
                        (i != root && function.entity(*e).unwrap().ty == root_type).then_some(i)
                    })
                    .unwrap();
                let block = function.blocks.iter_mut().find(|b| b.id == from).unwrap();
                let edge = match &mut block.terminator.as_mut().unwrap().kind {
                    TerminatorKind::Branch(e) => e,
                    TerminatorKind::Conditional {
                        when_true,
                        when_false,
                        ..
                    } => {
                        if when_true.target == self.target {
                            when_true
                        } else {
                            when_false
                        }
                    }
                    _ => panic!("captured predecessor"),
                };
                if fault == OwnerFault::NestedHeaderSwapArguments {
                    edge.arguments.swap(root, peer);
                } else {
                    edge.arguments.pop();
                }
                injected = true;
                record_action("nested-resume-ssa-fault-hit", 1);
            }
        }
        if take_fault(OwnerFault::NestedHeaderExtraPredecessor) {
            let block = function
                .blocks
                .iter_mut()
                .find(|b| b.id != self.target && !self.predecessors.contains(&b.id))
                .unwrap();
            block.terminator.as_mut().unwrap().kind =
                TerminatorKind::Branch(crate::ssa::model::Edge {
                    target: self.target,
                    arguments: self.seen[&from].clone(),
                });
            injected = true;
            record_action("nested-resume-ssa-fault-hit", 1);
        }
        if injected {
            let span = function
                .block(self.target)
                .unwrap()
                .terminator
                .as_ref()
                .unwrap()
                .origin
                .span();
            assert!(
                self.finish(function, span).is_err(),
                "independent header receipt must reject mutation before generic verifier"
            );
        }
    }
}
