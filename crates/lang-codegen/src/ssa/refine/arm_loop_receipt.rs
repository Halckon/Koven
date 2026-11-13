//! Separate selected if/else producer and detached SSA closure receipts.
use super::RefineInputs;
use super::body::{BlockRef, RefinedBody, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{BlockId, EntityId, Function, FunctionId, InstructionId},
};
use lang_frontend::{
    ownership_checking::{
        UnitDropPoint, UnitLoopBranchNoCarryFact, UnitLoopBreakBranchNoCarryFact,
        UnitLoopCounterBranchNoCarryFact, UnitLoopCounterBreakBranchNoCarryFact,
    },
    parser::{Expression, Statement},
    source::Span,
    type_checking::{ParameterMode, UnitCallTarget, UnitExpressionId},
};
use std::collections::BTreeMap;
#[cfg(test)]
mod break_ci;
mod break_hooks;
#[cfg(test)]
mod ci_tests;
use break_hooks::BreakPlan;
mod break_validation;
mod composed_prefix;
mod counter;
mod counter_break;
#[cfg(test)]
mod counter_break_ci;
mod counter_break_validation;
#[cfg(test)]
mod counter_ci;
mod counter_hooks;
mod counter_validation;
mod prefix;
#[cfg(test)]
mod prefix_ci;
mod prefix_fact;
#[cfg(test)]
mod publication;
mod validation;
#[cfg(test)]
pub(crate) use publication::observe;
#[cfg(test)]
pub(in crate::ssa) use publication::{UnitPublication, before_final_verifier};
#[cfg(test)]
pub(in crate::ssa::refine) use publication::{skip_break_capture, skip_counter_break_capture};
fn invalid(s: Span) -> LoweringError {
    error(LoweringErrorKind::InvalidModel, s)
}
#[derive(Clone, Debug)]
struct Arm {
    callee: FunctionId,
    consumers: Vec<FunctionId>,
    spans: Vec<Span>,
}
#[derive(Clone, Debug)]
pub(super) struct Plan {
    pub(super) fact: UnitLoopBranchNoCarryFact,
    arms: [Arm; 2],
    prefix: Option<prefix::Prefix>,
    counter: Option<counter::Counter>,
    pub(super) counter_break: Option<counter_break::CounterBreak>,
    pub(super) tail_break: Option<BreakPlan>,
    counter_fact: Option<UnitLoopCounterBranchNoCarryFact>,
    conditions: [UnitExpressionId; 2],
    condition_spans: [Span; 2],
    roles: [usize; 3],
    pub(super) source: Option<Slot>,
    pub(super) blocks: Option<[BlockRef; 5]>,
    split: Option<[BlockRef; 3]>,
    pub(super) join: Option<BlockRef>,
    used: [Option<(BlockRef, Slot)>; 2],
}
#[derive(Clone, Debug, Default)]
pub(super) struct Plans {
    pub(super) loops: Vec<Plan>,
}
impl Plans {
    pub(super) fn new(inputs: &RefineInputs<'_>) -> Result<Self, LoweringError> {
        let mut loops = Vec::new();
        for fact in
            crate::ssa::borrow_result_support::branch_no_carry::pairs(inputs.owned).filter(|p| {
                Some(p.arms()[0].callable().item()) == inputs.function_item
                    && p.branch().source_unit() == inputs.source_unit
            })
        {
            let fail = || invalid(inputs.span);
            if !crate::ssa::borrow_result_support::branch_no_carry::owns_pair(inputs.owned, fact) {
                return Err(fail());
            }
            let Expression::If {
                condition,
                then_branch,
                else_branch: Some(other),
                ..
            } = inputs
                .parsed
                .ast()
                .expressions()
                .get(fact.branch().expression())
                .map_err(|_| fail())?
                .payload()
            else {
                return Err(fail());
            };
            if [*then_branch, *other] != fact.arms().clone().map(|a| a.body().statement()) {
                return Err(fail());
            }
            let Statement::While {
                condition: outer, ..
            } = inputs
                .parsed
                .ast()
                .statements()
                .get(fact.arms()[0].loop_statement().statement())
                .map_err(|_| fail())?
                .payload()
            else {
                return Err(fail());
            };
            let counter_break = Self::counter_break_plan(inputs, fact)?;
            let counter_fact = inputs
                .owned
                .no_carry_counter_branches()
                .iter()
                .find(|c| c.pair().same_identity(fact))
                .cloned();
            if counter_fact
                .as_ref()
                .or_else(|| counter_break.as_ref().map(|c| c.fact.counter()))
                .is_some_and(|c| c.condition().expression() != *outer)
            {
                return Err(fail());
            }
            let tail_break = Self::break_plan(inputs, fact)?;
            if tail_break.is_some() && counter_fact.is_some() {
                return Err(fail());
            }
            let counter = counter_fact
                .as_ref()
                .or_else(|| counter_break.as_ref().map(|c| c.fact.counter()))
                .map(|c| Self::counter_spec(inputs, c))
                .transpose()?
                .map(counter::Counter::new);
            let mut role = Vec::new();
            role.push(
                inputs
                    .parameters
                    .iter()
                    .position(|p| *p == fact.arms()[0].source())
                    .ok_or_else(fail)?,
            );
            let initial = counter_fact
                .as_ref()
                .or_else(|| counter_break.as_ref().map(|c| c.fact.counter()))
                .map(|c| c.initializer().expression())
                .unwrap_or(*outer);
            for expr in [&initial, condition] {
                let node = inputs
                    .parsed
                    .ast()
                    .expressions()
                    .get(*expr)
                    .map_err(|_| fail())?;
                if !matches!(node.payload(), Expression::Name) {
                    return Err(fail());
                }
                let symbol = inputs
                    .references
                    .get(&(node.span().start(), node.span().end()))
                    .ok_or_else(fail)?;
                role.push(
                    inputs
                        .parameters
                        .iter()
                        .position(|p| p == symbol)
                        .ok_or_else(fail)?,
                );
            }
            if counter_fact.is_some()
                && !matches!(
                    inputs.modes.get(role[1]),
                    Some(ParameterMode::Borrow | ParameterMode::Value)
                )
            {
                return Err(fail());
            }
            if inputs.modes.get(role[0]) != Some(&ParameterMode::Borrow) {
                return Err(fail());
            }
            let mut arms = Vec::new();
            for f in fact.arms() {
                let mut bindings = inputs
                    .owned
                    .borrow_results()
                    .bindings()
                    .iter()
                    .filter(|b| b.binding() == f.binding());
                let binding = bindings.next().ok_or_else(fail)?;
                if bindings.next().is_some()
                    || crate::ssa::borrow_result_support::branch_no_carry::admitted(
                        inputs.parsed,
                        inputs.owned,
                        binding,
                    ) != Some((f.loop_statement().statement(), fact.branch().expression()))
                {
                    return Err(fail());
                }
                if f.consumers().is_empty()
                    || f.consumers()
                        .last()
                        .map(|c| UnitDropPoint::AfterStatement(c.0))
                        != Some(f.end())
                {
                    return Err(fail());
                }
                let calls = std::iter::once(f.initializer())
                    .chain(f.consumers().iter().map(|c| c.1))
                    .collect::<Vec<_>>();
                let mut callees = Vec::new();
                let mut spans = Vec::new();
                for c in calls {
                    let d = inputs.typed.call(c).ok_or_else(fail)?;
                    if d.prints_line()
                        || d.text_primitive().is_some()
                        || d.receiver().is_some()
                        || !matches!(d.target(), UnitCallTarget::Declaration(_))
                    {
                        return Err(fail());
                    }
                    callees.push(super::fresh_loop_receipt::callee(inputs, c)?);
                    spans.push(
                        inputs
                            .parsed
                            .ast()
                            .expressions()
                            .get(c.expression())
                            .map_err(|_| fail())?
                            .span(),
                    );
                }
                spans.push(binding.marker_span());
                arms.push(Arm {
                    callee: callees[0],
                    consumers: callees[1..].to_vec(),
                    spans,
                });
            }
            let arms: [Arm; 2] = arms.try_into().map_err(|_| fail())?;
            loops.push(Plan {
                fact: fact.clone(),
                arms,
                prefix: Self::prefix_plan(inputs, fact)?,
                counter,
                counter_fact,
                counter_break,
                tail_break,
                conditions: [
                    UnitExpressionId::new(inputs.source_unit, *outer),
                    UnitExpressionId::new(inputs.source_unit, *condition),
                ],
                condition_spans: [
                    inputs
                        .parsed
                        .ast()
                        .expressions()
                        .get(*outer)
                        .map_err(|_| fail())?
                        .span(),
                    inputs
                        .parsed
                        .ast()
                        .expressions()
                        .get(*condition)
                        .map_err(|_| fail())?
                        .span(),
                ],
                roles: [role[0], role[1], role[2]],
                source: None,
                blocks: None,
                split: None,
                join: None,
                used: [None, None],
            });
        }
        Ok(Self { loops })
    }
    pub(super) fn split(
        &mut self,
        id: UnitExpressionId,
        b: [BlockRef; 3],
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.loops {
            if let Some(cb) = &mut p.counter_break {
                cb.split(id, b, p.join, span)?;
            }
            if let Some(prefix) = &mut p.prefix {
                prefix.split(id, b, span)?;
            }
        }
        let Some(p) = self.loops.iter_mut().find(|p| p.fact.branch() == id) else {
            return Ok(());
        };
        if p.split.is_some()
            || p.blocks.is_none()
            || p.blocks.map(|b| b[2]) != Some(b[0])
            || b[1] == b[2]
        {
            return Err(invalid(span));
        }
        p.split = Some(b);
        Ok(())
    }
    pub(super) fn join(
        &mut self,
        id: UnitExpressionId,
        b: BlockRef,
        preds: &[BlockRef],
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.loops {
            if let Some(cb) = &mut p.counter_break {
                cb.join(id, b, preds, span)?;
            }
            if let Some(prefix) = &mut p.prefix {
                prefix.join(id, b, preds, span)?;
            }
        }
        let Some(p) = self.loops.iter_mut().find(|p| p.fact.branch() == id) else {
            return Ok(());
        };
        let s = p.split.ok_or_else(|| invalid(span))?;
        if p.join.is_some() || preds != [s[1], s[2]] {
            return Err(invalid(span));
        }
        p.join = Some(b);
        Ok(())
    }
    pub(super) fn condition(
        &mut self,
        id: UnitExpressionId,
        b: BlockRef,
        value: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.loops {
            if let Some(cb) = &mut p.counter_break {
                cb.condition(id, b, value, p.join, span)?;
            }
            if let Some(prefix) = &mut p.prefix {
                prefix.condition(id, b, value, span)?;
            }
            if let Some(i) = p.conditions.iter().position(|c| *c == id) {
                let blocks = p.blocks.ok_or_else(|| invalid(span))?;
                if p.used[i].is_some() || b != if i == 0 { blocks[1] } else { blocks[2] } {
                    return Err(invalid(span));
                }
                p.used[i] = Some((b, value));
            }
        }
        Ok(())
    }
    pub(super) fn fold(&mut self, block: BlockRef, successor: BlockRef, offset: usize) {
        for p in &mut self.loops {
            if let Some(cb) = &mut p.counter_break {
                cb.fold(block, successor);
            }
            if let Some(prefix) = &mut p.prefix {
                prefix.fold(block, successor);
            }
            if let Some(b) = &mut p.tail_break
                && let Some((from, to)) = &mut b.observed
            {
                if *from == successor {
                    *from = block;
                }
                if *to == successor {
                    *to = block;
                }
            }
            if let Some(c) = &mut p.counter {
                c.fold(block, successor, offset);
            }
            if let Some(bs) = &mut p.blocks {
                for b in bs {
                    if *b == successor {
                        *b = block;
                    }
                }
            }
            if let Some(bs) = &mut p.split {
                for b in bs {
                    if *b == successor {
                        *b = block;
                    }
                }
            }
            if p.join == Some(successor) {
                p.join = Some(block);
            }
            for (b, _) in p.used.iter_mut().flatten() {
                if *b == successor {
                    *b = block;
                }
            }
        }
    }
}
#[derive(Clone, Debug, Default)]
pub(super) struct Emitted {
    selected: Vec<UnitLoopBranchNoCarryFact>,
    counter_selected: Vec<UnitLoopCounterBranchNoCarryFact>,
    counter_break_selected: Vec<UnitLoopCounterBreakBranchNoCarryFact>,
    break_selected: Vec<UnitLoopBreakBranchNoCarryFact>,
    composed_prefix_selected:
        Vec<lang_frontend::ownership_checking::UnitLoopPrefixCounterBreakFact>,
    prefix_selected: Vec<lang_frontend::ownership_checking::UnitLoopBreakPrefixFact>,
    aliases: BTreeMap<(BlockRef, usize), (Slot, Slot, EntityId)>,
    blocks: BTreeMap<BlockRef, BlockId>,
    parameters: BTreeMap<BlockRef, Vec<Slot>>,
    entities: BTreeMap<BlockRef, Vec<EntityId>>,
    instructions: BTreeMap<(BlockRef, usize), InstructionId>,
    ends: BTreeMap<BlockId, BTreeMap<Slot, EntityId>>,
}
impl Emitted {
    pub(super) fn layout(
        &mut self,
        body: &RefinedBody,
        blocks: &BTreeMap<BlockRef, BlockId>,
        parameters: &BTreeMap<BlockRef, Vec<Slot>>,
        entities: &BTreeMap<BlockRef, Vec<EntityId>>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if body.arm_loop_required.len() != body.arm_loop_plans.loops.len()
            || body
                .arm_loop_required
                .iter()
                .zip(&body.arm_loop_plans.loops)
                .any(|(a, b)| !a.same_identity(&b.fact))
        {
            return Err(invalid(span));
        }
        self.check_counter_required(body, span)?;
        self.check_counter_break_required(body, span)?;
        self.check_break_required(body, span)?;
        self.check_prefix_required(body, span)?;
        self.check_composed_prefix_required(body, span)?;
        if body.arm_loop_required.is_empty() {
            return Ok(());
        }
        if !self.blocks.is_empty() {
            return Err(invalid(span));
        }
        for p in &body.arm_loop_plans.loops {
            if p.tail_break.as_ref().is_some_and(|b| b.observed.is_none()) {
                return Err(invalid(span));
            }
            if let Some(c) = &p.counter {
                c.ready(span)?;
            }
            if p.blocks.is_none()
                || p.split.is_none()
                || p.join.is_none()
                || p.used.iter().any(Option::is_none)
                || p.source != body.parameters.get(p.roles[0]).copied()
            {
                return Err(invalid(span));
            }
        }
        self.selected = body
            .arm_loop_plans
            .loops
            .iter()
            .map(|p| p.fact.clone())
            .collect();
        self.counter_selected = body.arm_counter_required.clone();
        self.counter_break_selected = body.arm_counter_break_required.clone();
        self.break_selected = body.arm_break_required.clone();
        self.prefix_selected = body.arm_prefix_required.clone();
        self.composed_prefix_selected = body.arm_composed_prefix_required.clone();
        self.blocks = blocks.clone();
        self.parameters = parameters.clone();
        self.entities = entities.clone();
        Ok(())
    }
    pub(super) fn statement(
        &mut self,
        body: &RefinedBody,
        position: (BlockRef, usize),
        id: InstructionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if body.arm_loop_plans.loops.is_empty() {
            return Ok(());
        }
        if self.instructions.insert(position, id).is_some() {
            return Err(invalid(span));
        }
        Ok(())
    }
    pub(super) fn terminator(
        &mut self,
        body: &RefinedBody,
        target: BlockId,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if body.arm_loop_plans.loops.is_empty() {
            return Ok(());
        }
        if self.ends.insert(target, current.clone()).is_some() {
            return Err(invalid(span));
        }
        Ok(())
    }
    #[allow(unused_mut)] // CI-only one-shot receipt-loss hook.
    pub(super) fn finish(
        mut self,
        body: &RefinedBody,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        #[cfg(test)]
        publication::before_finish(&mut self);
        self.check_counter_required(body, span)?;
        self.check_counter_break_required(body, span)?;
        self.check_break_required(body, span)?;
        self.check_prefix_required(body, span)?;
        self.check_composed_prefix_required(body, span)?;
        if self.counter_break_selected.len() != body.arm_counter_break_required.len()
            || self
                .counter_break_selected
                .iter()
                .zip(&body.arm_counter_break_required)
                .any(|(a, b)| !a.same_identity(b))
        {
            return Err(invalid(span));
        }
        if self.composed_prefix_selected.len() != body.arm_composed_prefix_required.len()
            || self
                .composed_prefix_selected
                .iter()
                .zip(&body.arm_composed_prefix_required)
                .any(|(a, b)| !a.same_identity(b))
        {
            return Err(invalid(span));
        }
        if self.prefix_selected.len() != body.arm_prefix_required.len()
            || self
                .prefix_selected
                .iter()
                .zip(&body.arm_prefix_required)
                .any(|(a, b)| !a.same_identity(b))
        {
            return Err(invalid(span));
        }
        if self.break_selected.len() != body.arm_break_required.len()
            || self
                .break_selected
                .iter()
                .zip(&body.arm_break_required)
                .any(|(a, b)| !a.same_identity(b))
        {
            return Err(invalid(span));
        }
        if self.counter_selected.len() != body.arm_counter_required.len()
            || self
                .counter_selected
                .iter()
                .zip(&body.arm_counter_required)
                .any(|(a, b)| !a.same_identity(b))
        {
            return Err(invalid(span));
        }
        if self.selected.len() != body.arm_loop_plans.loops.len()
            || self
                .selected
                .iter()
                .zip(&body.arm_loop_plans.loops)
                .any(|(a, b)| !a.same_identity(&b.fact))
        {
            return Err(invalid(span));
        }
        for p in &body.arm_loop_plans.loops {
            let receipt = Receipt {
                caller: function.id,
                evidence: self.clone(),
                parameters: body.parameters.clone(),
                plan: p.clone(),
                span,
            };
            receipt.validate(function)?;
            #[cfg(test)]
            publication::record(receipt);
        }
        Ok(())
    }
}

/// Replayable evidence: no builder state or live RefinedBody is needed.
#[derive(Clone, Debug)]
pub(crate) struct Receipt {
    pub(crate) caller: FunctionId,
    evidence: Emitted,
    parameters: Vec<Slot>,
    plan: Plan,
    span: Span,
}
impl Receipt {
    pub(crate) fn validate(&self, function: &Function) -> Result<(), LoweringError> {
        if function.id != self.caller
            || !self
                .evidence
                .selected
                .iter()
                .any(|f| f.same_identity(&self.plan.fact))
            || self.plan.tail_break.as_ref().is_some_and(|b| {
                !self
                    .evidence
                    .break_selected
                    .iter()
                    .any(|f| f.same_identity(&b.fact))
            })
            || self.plan.counter.is_some()
                != (self.plan.counter_fact.is_some() || self.plan.counter_break.is_some())
            || self.plan.counter_break.as_ref().is_some_and(|b| {
                !self
                    .evidence
                    .counter_break_selected
                    .iter()
                    .any(|f| f.same_identity(&b.fact))
            })
            || self.plan.counter_fact.as_ref().is_some_and(|f| {
                !self
                    .evidence
                    .counter_selected
                    .iter()
                    .any(|p| p.same_identity(f))
            })
        {
            return Err(invalid(self.span));
        }
        let mut prefixes = self
            .evidence
            .prefix_selected
            .iter()
            .filter(|p| p.pair().pair().same_identity(&self.plan.fact));
        match (prefixes.next(), self.plan.prefix.as_ref().map(|p| &p.fact)) {
            (None, None) | (None, Some(prefix_fact::PrefixFact::Counter(_))) => {}
            (Some(a), Some(prefix_fact::PrefixFact::Tail(b))) if a.same_identity(b) => {}
            _ => return Err(invalid(self.span)),
        }
        if prefixes.next().is_some() {
            return Err(invalid(self.span));
        }
        let mut composed = self.evidence.composed_prefix_selected.iter().filter(|p| {
            p.counter_break()
                .counter()
                .pair()
                .same_identity(&self.plan.fact)
        });
        match (composed.next(), self.plan.prefix.as_ref().map(|p| &p.fact)) {
            (None, None) | (None, Some(prefix_fact::PrefixFact::Tail(_))) => {}
            (Some(a), Some(prefix_fact::PrefixFact::Counter(b))) if a.same_identity(b) => {}
            _ => return Err(invalid(self.span)),
        }
        if composed.next().is_some() {
            return Err(invalid(self.span));
        }
        self.evidence
            .validate(&self.parameters, function, &self.plan, self.span)
    }
}
