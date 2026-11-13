//! Separate selected if/else producer and detached SSA closure receipts.
use super::RefineInputs;
use super::body::{BlockRef, RefinedBody, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{BlockId, EntityId, Function, FunctionId, InstructionId},
};
use lang_frontend::{
    ownership_checking::{UnitDropPoint, UnitLoopBranchNoCarryFact},
    parser::{Expression, Statement},
    source::Span,
    type_checking::{ParameterMode, UnitCallTarget, UnitExpressionId},
};
use std::collections::BTreeMap;
mod validation;
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
    conditions: [UnitExpressionId; 2],
    roles: [usize; 3],
    pub(super) source: Option<Slot>,
    pub(super) blocks: Option<[BlockRef; 5]>,
    split: Option<[BlockRef; 3]>,
    join: Option<BlockRef>,
    used: [Option<(BlockRef, Slot)>; 2],
}
#[derive(Clone, Debug, Default)]
pub(super) struct Plans {
    pub(super) loops: Vec<Plan>,
}
impl Plans {
    pub(super) fn new(inputs: &RefineInputs<'_>) -> Result<Self, LoweringError> {
        let mut loops = Vec::new();
        for fact in inputs.owned.no_carry_branches().iter().filter(|p| {
            Some(p.arms()[0].callable().item()) == inputs.function_item
                && p.branch().source_unit() == inputs.source_unit
        }) {
            let fail = || invalid(inputs.span);
            if !inputs.owned.owns_no_carry_branch(fact) {
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
            let mut role = Vec::new();
            role.push(
                inputs
                    .parameters
                    .iter()
                    .position(|p| *p == fact.arms()[0].source())
                    .ok_or_else(fail)?,
            );
            for expr in [outer, condition] {
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
                conditions: [
                    UnitExpressionId::new(inputs.source_unit, *outer),
                    UnitExpressionId::new(inputs.source_unit, *condition),
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
    pub(super) fn fold(&mut self, block: BlockRef, successor: BlockRef) {
        for p in &mut self.loops {
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
#[derive(Default)]
pub(super) struct Emitted {
    selected: Vec<UnitLoopBranchNoCarryFact>,
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
        if body.arm_loop_required.is_empty() {
            return Ok(());
        }
        if !self.blocks.is_empty() {
            return Err(invalid(span));
        }
        for p in &body.arm_loop_plans.loops {
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
    pub(super) fn finish(
        self,
        body: &RefinedBody,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
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
            self.validate(body, function, p, span)?;
        }
        Ok(())
    }
}
