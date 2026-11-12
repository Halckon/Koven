//! Independent SSA evidence for the already authorized bounded parent write.
//! This neither emits operations nor proves arbitrary loop generations.
use super::body::{BlockRef, ROp, RefinedBody, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{BlockId, EntityId, Function, FunctionId, InstructionId, Operation, TerminatorKind},
};
use lang_frontend::{source::Span, type_checking::UnitStatementId};
use std::collections::BTreeMap;
mod carry;
pub(super) use carry::{Carry, CarryChecked};
fn invalid(span: Span) -> LoweringError {
    error(LoweringErrorKind::InvalidModel, span)
}
#[derive(Clone, Debug)]
pub(super) struct Plan {
    pub carry: Option<Carry>,
    pub parent: UnitStatementId,
    pub child: UnitStatementId,
    pub header: BlockRef,
    pub finished: BlockRef,
    pub resume: BlockRef,
    pub root: Slot,
    pub replacement: Slot,
    pub old: Slot,
    pub bound: Slot,
    pub positions: [usize; 3],
    pub spans: [Span; 3],
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
            if let Some(c) = &mut p.carry {
                c.fold(block, successor, prefix, span)?;
            }
            if p.header == successor {
                p.header = block;
            }
            if p.finished == successor {
                p.finished = block;
            }
            if p.resume == successor {
                p.resume = block;
                for position in &mut p.positions {
                    *position = position.checked_add(prefix).ok_or_else(|| invalid(span))?;
                }
            }
            if p.header == p.finished || p.header == p.resume {
                return Err(invalid(span));
            }
        }
        Ok(())
    }
}
#[derive(Default)]
pub(super) struct Emitted {
    checked: BTreeMap<BlockRef, (InstructionId, Operation)>,
    function: Option<FunctionId>,
    blocks: BTreeMap<BlockRef, BlockId>,
    inputs: BTreeMap<BlockRef, EntityId>,
    parameters: BTreeMap<BlockRef, Vec<EntityId>>,
    slots: BTreeMap<BlockRef, Vec<Slot>>,
    replace: Option<(InstructionId, EntityId, EntityId, EntityId, EntityId)>,
    replace_operation: Option<Operation>,
    alias: Option<EntityId>,
    drop: Option<InstructionId>,
}
fn recheck_plan(body: &RefinedBody, p: &Plan, span: Span) -> Result<(), LoweringError> {
    let block = body.blocks.get(p.resume.0).ok_or_else(|| invalid(span))?;
    for stage in 0..3 {
        let s = block
            .statements
            .get(p.positions[stage])
            .ok_or_else(|| invalid(span))?;
        let matches = match stage {
            0 => {
                s.results == [p.root, p.old]
                    && matches!(s.op, ROp::RootReplace { owner, replacement, .. } if owner == p.root && replacement == p.replacement)
            }
            1 => s.results == [p.bound] && matches!(s.op, ROp::Alias { source } if source == p.old),
            2 => s.results.is_empty() && matches!(s.op, ROp::Drop { owner } if owner == p.bound),
            _ => false,
        };
        if s.span != p.spans[stage] || !matches {
            return Err(invalid(span));
        }
    }
    Ok(())
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
        let Some(p) = &body.nested_resume_plans.0 else {
            return Ok(());
        };
        recheck_plan(body, p, span)?;
        if self.function.is_some()
            || p.parent == p.child
            || p.header == p.finished
            || p.header == p.resume
            || !(p.positions[0] < p.positions[1] && p.positions[1] < p.positions[2])
        {
            return Err(invalid(span));
        }
        for b in [p.header, p.finished, p.resume] {
            let id = *blocks.get(&b).ok_or_else(|| invalid(span))?;
            let ss = slots.get(&b).ok_or_else(|| invalid(span))?;
            let es = entities.get(&b).ok_or_else(|| invalid(span))?;
            let index = ss
                .iter()
                .position(|s| *s == p.root)
                .ok_or_else(|| invalid(span))?;
            if ss.iter().filter(|s| **s == p.root).count() != 1
                || ss.len() != es.len()
                || function.block(id).is_none_or(|b| b.parameters != *es)
            {
                return Err(invalid(span));
            }
            self.inputs
                .insert(b, *es.get(index).ok_or_else(|| invalid(span))?);
        }
        self.function = Some(function.id());
        self.blocks = blocks.clone();
        self.slots = slots.clone();
        self.parameters = entities.clone();
        Ok(())
    }
    pub(super) fn statement(
        &mut self,
        body: &RefinedBody,
        function: &Function,
        position: (BlockRef, usize),
        instruction: Option<InstructionId>,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &body.nested_resume_plans.0 else {
            return Ok(());
        };
        recheck_plan(body, p, span)?;
        if let Some(c) = &p.carry {
            c.observe(function, position, instruction, &mut self.checked, span)?;
        }
        if position.0 != p.resume {
            return Ok(());
        }
        let Some(stage) = p.positions.iter().position(|i| *i == position.1) else {
            return Ok(());
        };
        if self.function != Some(function.id()) || span != p.spans[stage] {
            return Err(invalid(span));
        }
        let get = |slot| current.get(&slot).copied().ok_or_else(|| invalid(span));
        match stage {
            0 => {
                let id = instruction.ok_or_else(|| invalid(span))?;
                let s = function.instruction(id).ok_or_else(|| invalid(span))?;
                let Operation::RootReplace {
                    owner, replacement, ..
                } = s.operation
                else {
                    return Err(invalid(span));
                };
                let old = get(p.old)?;
                let new = get(p.root)?;
                let rhs = get(p.replacement)?;
                if self.replace.is_some()
                    || s.block != self.blocks[&p.resume]
                    || s.results != [new, old]
                    || old == new
                    || EntityId::Value(owner) != self.inputs[&p.resume]
                    || EntityId::Value(replacement) != rhs
                {
                    return Err(invalid(span));
                }
                self.replace_operation = Some(s.operation.clone());
                self.replace = Some((id, EntityId::Value(owner), rhs, new, old));
            }
            1 => {
                let (_, _, _, _, old) = self.replace.ok_or_else(|| invalid(span))?;
                if instruction.is_some()
                    || self.alias.is_some()
                    || get(p.bound)? != old
                    || get(p.old)? != old
                {
                    return Err(invalid(span));
                }
                self.alias = Some(old);
            }
            2 => {
                let old = self.alias.ok_or_else(|| invalid(span))?;
                let id = instruction.ok_or_else(|| invalid(span))?;
                let s = function.instruction(id).ok_or_else(|| invalid(span))?;
                if self.drop.is_some()
                    || get(p.bound)? != old
                    || s.block != self.blocks[&p.resume]
                    || !matches!(s.operation,Operation::Drop{owner} if EntityId::Value(owner)==old)
                {
                    return Err(invalid(span));
                }
                self.drop = Some(id);
            }
            _ => return Err(invalid(span)),
        }
        Ok(())
    }
    pub(super) fn finish(
        &self,
        body: &RefinedBody,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &body.nested_resume_plans.0 else {
            return Ok(());
        };
        recheck_plan(body, p, span)?;
        if self.function != Some(function.id()) {
            return Err(invalid(span));
        }
        let (replace, owner, rhs, new, old) = self.replace.ok_or_else(|| invalid(span))?;
        let drop = self.drop.ok_or_else(|| invalid(span))?;
        if self.alias != Some(old) {
            return Err(invalid(span));
        }
        for b in [p.header, p.finished, p.resume] {
            if function
                .block(self.blocks[&b])
                .is_none_or(|v| v.parameters != self.parameters[&b])
            {
                return Err(invalid(span));
            }
        }
        let header = function
            .block(self.blocks[&p.header])
            .ok_or_else(|| invalid(span))?;
        let Some(term) = &header.terminator else {
            return Err(invalid(span));
        };
        let TerminatorKind::Conditional { when_false, .. } = &term.kind else {
            return Err(invalid(span));
        };
        self.edge(p.header, p.finished, when_false, span)?;
        if p.finished != p.resume {
            let finished = function
                .block(self.blocks[&p.finished])
                .ok_or_else(|| invalid(span))?;
            let Some(term) = &finished.terminator else {
                return Err(invalid(span));
            };
            let TerminatorKind::Branch(edge) = &term.kind else {
                return Err(invalid(span));
            };
            self.edge(p.finished, p.resume, edge, span)?;
        }
        let block = function
            .block(self.blocks[&p.resume])
            .ok_or_else(|| invalid(span))?;
        let unique = |id| {
            let indexes = block
                .instructions
                .iter()
                .enumerate()
                .filter_map(|(i, x)| (*x == id).then_some(i))
                .collect::<Vec<_>>();
            match indexes.as_slice() {
                [i] => Ok(*i),
                _ => Err(invalid(span)),
            }
        };
        if unique(replace)? >= unique(drop)? {
            return Err(invalid(span));
        }
        let r = function.instruction(replace).ok_or_else(|| invalid(span))?;
        let d = function.instruction(drop).ok_or_else(|| invalid(span))?;
        if r.block != block.id
            || d.block != block.id
            || r.origin.span() != p.spans[0]
            || d.origin.span() != p.spans[2]
            || self.replace_operation.as_ref() != Some(&r.operation)
            || r.results != [new, old]
            || !d.results.is_empty()
            || !matches!(r.operation,Operation::RootReplace{owner:o,replacement:v,..} if EntityId::Value(o)==owner && EntityId::Value(v)==rhs)
            || !matches!(d.operation,Operation::Drop{owner:o} if EntityId::Value(o)==old)
        {
            return Err(invalid(span));
        }
        // Alias has vanished: the actual detached result must be consumed only by the original Drop.
        for s in &function.instructions {
            if s.results.contains(&old) != (s.id == replace)
                || s.operation.entities().iter().filter(|e| **e == old).count()
                    != usize::from(s.id == drop)
            {
                return Err(invalid(span));
            }
        }
        for b in &function.blocks {
            if b.parameters.contains(&old)
                || b.terminator
                    .as_ref()
                    .is_some_and(|t| t.kind.entities().contains(&old))
            {
                return Err(invalid(span));
            }
        }
        if let Some(c) = &p.carry {
            c.finish(
                p,
                carry::CarryView {
                    function,
                    blocks: &self.blocks,
                    slots: &self.slots,
                    parameters: &self.parameters,
                    seen: &self.checked,
                },
                (replace, new),
                span,
            )?;
        }
        #[cfg(test)]
        super::build::owner_consumer_test_support::record_action(
            "nested-parent-resume-ssa-complete",
            1,
        );
        Ok(())
    }
    fn edge(
        &self,
        from: BlockRef,
        to: BlockRef,
        edge: &crate::ssa::model::Edge,
        span: Span,
    ) -> Result<(), LoweringError> {
        let slots = self.slots.get(&to).ok_or_else(|| invalid(span))?;
        // The root is identified by its captured entity, not vector ordering across blocks.
        let entities = self.parameters.get(&to).ok_or_else(|| invalid(span))?;
        let index = entities
            .iter()
            .position(|e| Some(e) == self.inputs.get(&to))
            .ok_or_else(|| invalid(span))?;
        if edge.target != self.blocks[&to]
            || edge.arguments.len() != slots.len()
            || edge.arguments.get(index) != self.inputs.get(&from)
        {
            return Err(invalid(span));
        }
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn inject_final(plans: &Plans, function: &mut Function) {
    use super::build::owner_consumer_test_support::{OwnerFault, record_action, take_fault};
    let Some(p) = &plans.0 else {
        return;
    };
    let Some((replace, new)) = function.instructions.iter().find_map(|i| {
        (i.origin.span() == p.spans[0] && matches!(i.operation, Operation::RootReplace { .. }))
            .then(|| (i.id, i.results[0]))
    }) else {
        return;
    };
    let Some((drop, block)) = function.instructions.iter().find_map(|i| {
        (i.origin.span() == p.spans[2] && matches!(i.operation, Operation::Drop { .. }))
            .then_some((i.id, i.block))
    }) else {
        return;
    };
    if take_fault(OwnerFault::NestedResumeSsaMissingDrop) {
        function
            .blocks
            .iter_mut()
            .find(|b| b.id == block)
            .unwrap()
            .instructions
            .retain(|id| *id != drop);
        record_action("nested-resume-ssa-fault-hit", 1);
    }
    if take_fault(OwnerFault::NestedResumeSsaDropNewRoot) {
        let EntityId::Value(new) = new else {
            panic!("owned result");
        };
        function
            .instructions
            .iter_mut()
            .find(|i| i.id == drop)
            .unwrap()
            .operation = Operation::Drop { owner: new };
        record_action("nested-resume-ssa-fault-hit", 1);
    }
    if take_fault(OwnerFault::NestedResumeSsaDuplicateReplace) {
        function
            .blocks
            .iter_mut()
            .find(|b| b.id == block)
            .unwrap()
            .instructions
            .push(replace);
        record_action("nested-resume-ssa-fault-hit", 1);
    }
    if let Some(carry) = &p.carry
        && let Some(checked) = carry.checked.first()
        && let Some(body) = function
            .instructions
            .iter()
            .find(|i| {
                i.origin.span() == checked.span
                    && matches!(i.operation, Operation::CheckedArithmetic { .. })
            })
            .map(|i| i.block)
    {
        if take_fault(OwnerFault::NestedCarrySsaWrongRootArgument)
            && let Some(term) = function
                .blocks
                .iter_mut()
                .find(|b| b.id == body)
                .and_then(|b| b.terminator.as_mut())
            && let TerminatorKind::Conditional { when_false, .. } = &mut term.kind
            && let Some(index) = when_false.arguments.iter().position(|v| *v == new)
            && when_false.arguments.len() > 1
        {
            when_false.arguments.swap(index, usize::from(index == 0));
            record_action("nested-resume-ssa-fault-hit", 1);
        }
        if take_fault(OwnerFault::NestedCarrySsaAbortToSuccess)
            && let Some(term) = function
                .blocks
                .iter_mut()
                .find(|b| b.id == body)
                .and_then(|b| b.terminator.as_mut())
            && let TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } = &mut term.kind
        {
            when_true.target = when_false.target;
            record_action("nested-resume-ssa-fault-hit", 1);
        }
    }
}
