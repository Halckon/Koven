//! Late SSA retirement evidence for the already authorized child normal body.
//! No loop entry, exit, parent recovery or new owner permission is created here.
use super::body::{BlockRef, ROp, RefinedBody, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{BlockId, EntityId, Function, FunctionId, InstructionId, Operation},
};
use lang_frontend::{source::Span, type_checking::UnitStatementId};
use std::collections::{BTreeMap, BTreeSet};
fn invalid(span: Span) -> LoweringError {
    error(LoweringErrorKind::InvalidModel, span)
}
#[derive(Clone, Debug)]
pub(super) struct Step {
    pub root: Slot,
    pub replacement: Slot,
    pub old: Slot,
    pub bound: Slot,
    pub positions: [usize; 3],
    pub spans: [Span; 3],
}
#[derive(Clone, Debug)]
pub(super) struct Plan {
    pub cycle_required: bool,
    pub control: UnitStatementId,
    pub body: BlockRef,
    pub steps: Vec<Step>,
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
        if let Some(p) = &mut self.0
            && p.body == successor
        {
            p.body = block;
            for s in &mut p.steps {
                for position in &mut s.positions {
                    *position = position.checked_add(prefix).ok_or_else(|| invalid(span))?;
                }
            }
        }
        Ok(())
    }
}
fn recheck(body: &RefinedBody, p: &Plan, span: Span) -> Result<(), LoweringError> {
    let fail = || invalid(span);
    let block = body.blocks.get(p.body.0).ok_or_else(fail)?;
    if p.steps.is_empty() {
        return Err(fail());
    }
    let mut roots = BTreeSet::new();
    let mut previous = None;
    for s in &p.steps {
        if !roots.insert(s.root)
            || s.old == s.bound
            || s.root == s.old
            || s.root == s.bound
            || previous.is_some_and(|v| v >= s.positions[0])
            || !(s.positions[0] < s.positions[1] && s.positions[1] < s.positions[2])
        {
            return Err(fail());
        }
        previous = Some(s.positions[2]);
        for stage in 0..3 {
            let actual = block.statements.get(s.positions[stage]).ok_or_else(fail)?;
            let matches = match stage {
                0 => {
                    actual.results == [s.root, s.old]
                        && matches!(actual.op,ROp::RootReplace{owner,replacement,..} if owner==s.root && replacement==s.replacement)
                }
                1 => {
                    actual.results == [s.bound]
                        && matches!(actual.op,ROp::Alias{source} if source==s.old)
                }
                2 => {
                    actual.results.is_empty()
                        && matches!(actual.op,ROp::Drop{owner} if owner==s.bound)
                }
                _ => false,
            };
            if !matches || actual.span != s.spans[stage] {
                return Err(fail());
            }
        }
    }
    Ok(())
}
#[derive(Default)]
struct Observed {
    replace: Option<(InstructionId, Operation, EntityId, EntityId)>,
    alias: bool,
    drop: Option<InstructionId>,
}
#[derive(Default)]
pub(super) struct Emitted {
    function: Option<FunctionId>,
    control: Option<UnitStatementId>,
    block: Option<BlockId>,
    parameters: Vec<EntityId>,
    inputs: Vec<EntityId>,
    observed: Vec<Observed>,
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
        let Some(p) = &body.nested_body_plans.0 else {
            return Ok(());
        };
        recheck(body, p, span)?;
        let fail = || invalid(span);
        if self.function.is_some() {
            return Err(fail());
        }
        let id = *blocks.get(&p.body).ok_or_else(fail)?;
        let ss = slots.get(&p.body).ok_or_else(fail)?;
        let es = entities.get(&p.body).ok_or_else(fail)?;
        if ss.len() != es.len() || function.block(id).is_none_or(|b| b.parameters != *es) {
            return Err(fail());
        }
        let mut inputs = Vec::new();
        for step in &p.steps {
            let matches = ss
                .iter()
                .enumerate()
                .filter(|(_, s)| **s == step.root)
                .collect::<Vec<_>>();
            let [(index, _)] = matches.as_slice() else {
                return Err(fail());
            };
            inputs.push(*es.get(*index).ok_or_else(fail)?);
        }
        self.function = Some(function.id());
        self.control = Some(p.control);
        self.block = Some(id);
        self.parameters = es.clone();
        self.inputs = inputs;
        self.observed = (0..p.steps.len()).map(|_| Observed::default()).collect();
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
        let Some(p) = &body.nested_body_plans.0 else {
            return Ok(());
        };
        if position.0 != p.body {
            return Ok(());
        }
        let Some((index, stage)) = p.steps.iter().enumerate().find_map(|(i, s)| {
            s.positions
                .iter()
                .position(|x| *x == position.1)
                .map(|stage| (i, stage))
        }) else {
            return Ok(());
        };
        recheck(body, p, span)?;
        let fail = || invalid(span);
        if self.function != Some(function.id()) || self.control != Some(p.control) {
            return Err(fail());
        }
        let s = &p.steps[index];
        let o = self.observed.get_mut(index).ok_or_else(fail)?;
        let get = |slot| current.get(&slot).copied().ok_or_else(fail);
        if span != s.spans[stage] {
            return Err(fail());
        }
        match stage {
            0 => {
                let id = instruction.ok_or_else(fail)?;
                let actual = function.instruction(id).ok_or_else(fail)?;
                let new = get(s.root)?;
                let old = get(s.old)?;
                let rhs = get(s.replacement)?;
                if o.replace.is_some()
                    || old == new
                    || !matches!((new, old), (EntityId::Value(_), EntityId::Value(_)))
                    || Some(actual.block) != self.block
                    || actual.results != [new, old]
                    || !matches!(actual.operation,Operation::RootReplace{owner,replacement,..} if Some(&EntityId::Value(owner))==self.inputs.get(index) && EntityId::Value(replacement)==rhs)
                {
                    return Err(fail());
                }
                o.replace = Some((id, actual.operation.clone(), new, old));
            }
            1 => {
                #[cfg(test)]
                if super::build::owner_consumer_test_support::take_fault(super::build::owner_consumer_test_support::OwnerFault::NestedBodySsaMissingAlias) {
                    super::build::owner_consumer_test_support::record_action("nested-body-ssa-fault-hit",1);
                    return Ok(());
                }
                let old = o.replace.as_ref().ok_or_else(fail)?.3;
                if instruction.is_some() || o.alias || get(s.old)? != old || get(s.bound)? != old {
                    return Err(fail());
                }
                o.alias = true;
            }
            2 => {
                let old = o.replace.as_ref().ok_or_else(fail)?.3;
                let id = instruction.ok_or_else(fail)?;
                let actual = function.instruction(id).ok_or_else(fail)?;
                if !o.alias
                    || o.drop.is_some()
                    || get(s.bound)? != old
                    || Some(actual.block) != self.block
                    || !actual.results.is_empty()
                    || !matches!(actual.operation,Operation::Drop{owner} if EntityId::Value(owner)==old)
                {
                    return Err(fail());
                }
                o.drop = Some(id);
            }
            _ => return Err(fail()),
        }
        Ok(())
    }
    // Caller must first complete finish: these are original emission observations,
    // not generations reconstructed from the final CFG.
    pub(super) fn generations(
        &self,
        body: &RefinedBody,
        span: Span,
    ) -> Result<Vec<(Slot, EntityId, EntityId)>, LoweringError> {
        let Some(plan) = &body.nested_body_plans.0 else {
            return Ok(Vec::new());
        };
        plan.steps
            .iter()
            .enumerate()
            .map(|(i, step)| {
                let input = self.inputs.get(i).copied().ok_or_else(|| invalid(span))?;
                let new = self
                    .observed
                    .get(i)
                    .and_then(|o| o.replace.as_ref())
                    .map(|r| r.2)
                    .ok_or_else(|| invalid(span))?;
                Ok((step.root, input, new))
            })
            .collect()
    }
    pub(super) fn finish(
        &self,
        body: &RefinedBody,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &body.nested_body_plans.0 else {
            return Ok(());
        };
        recheck(body, p, span)?;
        let fail = || invalid(span);
        if self.function != Some(function.id())
            || self.control != Some(p.control)
            || self.observed.len() != p.steps.len()
        {
            return Err(fail());
        }
        let block = function
            .block(self.block.ok_or_else(fail)?)
            .ok_or_else(fail)?;
        if block.parameters != self.parameters {
            return Err(fail());
        }
        let unique = |id| {
            let positions = block
                .instructions
                .iter()
                .enumerate()
                .filter_map(|(i, x)| (*x == id).then_some(i))
                .collect::<Vec<_>>();
            match positions.as_slice() {
                [i] => Ok(*i),
                _ => Err(fail()),
            }
        };
        let mut previous = None;
        let mut retired = BTreeSet::new();
        for (s, o) in p.steps.iter().zip(&self.observed) {
            let (id, operation, new, old) = o.replace.as_ref().ok_or_else(fail)?;
            let drop = o.drop.ok_or_else(fail)?;
            let at = unique(*id)?;
            let end = unique(drop)?;
            if !o.alias || !retired.insert(*old) || previous.is_some_and(|v| v >= at) || at >= end {
                return Err(fail());
            }
            previous = Some(end);
            let r = function.instruction(*id).ok_or_else(fail)?;
            let d = function.instruction(drop).ok_or_else(fail)?;
            if r.block != block.id
                || d.block != block.id
                || r.origin.span() != s.spans[0]
                || d.origin.span() != s.spans[2]
                || &r.operation != operation
                || r.results != [*new, *old]
                || !d.results.is_empty()
                || !matches!(d.operation,Operation::Drop{owner} if EntityId::Value(owner)==*old)
            {
                return Err(fail());
            }
            // The eliminated Alias maps to this same SSA Value, not a fabricated second owner.
            for i in &function.instructions {
                if i.results.contains(old) != (i.id == *id)
                    || i.operation.entities().iter().filter(|e| *e == old).count()
                        != usize::from(i.id == drop)
                {
                    return Err(fail());
                }
            }
            for b in &function.blocks {
                if b.parameters.contains(old)
                    || b.terminator
                        .as_ref()
                        .is_some_and(|t| t.kind.entities().contains(old))
                {
                    return Err(fail());
                }
            }
        }
        #[cfg(test)]
        super::build::owner_consumer_test_support::record_action(
            "nested-body-retirement-ssa-complete",
            p.steps.len(),
        );
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn inject_final(plans: &Plans, function: &mut Function) {
    use super::build::owner_consumer_test_support::{OwnerFault, record_action, take_fault};
    let Some(plan) = &plans.0 else {
        return;
    };
    let Some(first) = plan.steps.first() else {
        return;
    };
    let Some(last) = plan.steps.last() else {
        return;
    };
    let replacements = function
        .instructions
        .iter()
        .filter(|i| {
            i.origin.span() == first.spans[0]
                && matches!(i.operation, Operation::RootReplace { .. })
        })
        .collect::<Vec<_>>();
    let [replace] = replacements.as_slice() else {
        return;
    };
    let [_, EntityId::Value(old)] = replace.results.as_slice() else {
        return;
    };
    let old = *old;
    let drops = function
        .instructions
        .iter()
        .filter(|i| {
            i.origin.span() == last.spans[2] && matches!(i.operation, Operation::Drop { .. })
        })
        .map(|i| (i.id, i.block))
        .collect::<Vec<_>>();
    let [(id, block)] = drops.as_slice() else {
        return;
    };
    let (id, block) = (*id, *block);
    if take_fault(OwnerFault::NestedBodySsaMissingLastDrop)
        && let Some(b) = function.blocks.iter_mut().find(|b| b.id == block)
    {
        b.instructions.retain(|i| *i != id);
        record_action("nested-body-ssa-fault-hit", 1);
    }
    if plan.steps.len() >= 2
        && take_fault(OwnerFault::NestedBodySsaForeignRetiredValue)
        && let Some(i) = function.instructions.iter_mut().find(|i| i.id == id)
    {
        i.operation = Operation::Drop { owner: old };
        record_action("nested-body-ssa-fault-hit", 1);
    }
}
