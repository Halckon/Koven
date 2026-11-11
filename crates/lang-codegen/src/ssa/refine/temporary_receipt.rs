//! Optional bounded protocol-1 verification. Existing producers alone emit effects.
mod admission;
mod emitted {
    use super::*;
    #[cfg(test)]
    #[path = "temporary_receipt/ci_tests.rs"]
    mod ci_tests;
    use crate::ssa::model::{BlockId, EntityId, Function, InstructionId, Operation};
    use std::collections::BTreeMap;
    #[derive(Clone, Debug)]
    pub(crate) struct Receipt {
        pub(crate) ids: Vec<UnitLoanActionId>,
        pub(crate) callable: UnitItemId,
        pub(crate) caller: FunctionId,
        pub(crate) call: UnitExpressionId,
        pub(crate) instructions: Vec<InstructionId>,
        /// Source-order triples: formed owner, root place, caller-local argument loan.
        pub(crate) entities: Vec<EntityId>,
    }
    pub(in crate::ssa::refine) struct Emitted {
        receipts: Vec<Receipt>,
        blocks: Vec<Option<BlockId>>,
    }
    impl Emitted {
        pub(in crate::ssa::refine) fn new(
            plans: &Plans,
            function: &Function,
            span: Span,
        ) -> Result<Self, LoweringError> {
            let mut receipts = Vec::new();
            for p in &plans.entries {
                if instruction_count(p.operands.len()) != Some(p.stage) || p.caller != function.id() {
                    return Err(invalid(span));
                }
                receipts.push(Receipt {
                    ids: p.ids.clone(),
                    callable: p.callable,
                    caller: p.caller,
                    call: p.call,
                    instructions: Vec::new(),
                    entities: Vec::new(),
                });
            }
            Ok(Self {
                blocks: vec![None; receipts.len()],
                receipts,
            })
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
            let p = plans.entries.get(tag.plan).ok_or_else(|| invalid(span))?;
            let r = self
                .receipts
                .get_mut(tag.plan)
                .ok_or_else(|| invalid(span))?;
            let data = function
                .instruction(instruction)
                .ok_or_else(|| invalid(span))?;
            if r.caller != function.id()
                || r.instructions.len() != tag.role
                || self.blocks[tag.plan].is_some_and(|b| b != data.block)
            {
                return Err(invalid(span));
            }
            let entity = |slot: Option<Slot>| {
                slot.and_then(|s| current.get(&s).copied())
                    .ok_or_else(|| invalid(span))
            };
            for (slot, frozen) in p.slots().zip(&r.entities) {
                if entity(slot)? != *frozen {
                    return Err(invalid(span));
                }
            }
            let step = p.step(tag.role).ok_or_else(|| invalid(span))?;
            let mut produced = None;
            match (step, &data.operation) {
                (Step::Literal(i), Operation::StringLiteral { bytes, .. })
                    if Some(bytes) == p.operands[i].literal.as_ref() =>
                {
                    let [owner @ EntityId::Value(_)] = data.results.as_slice() else {
                        return Err(invalid(span));
                    };
                    if r.entities.len() != 3 * i || *owner != entity(p.operands[i].owner)? {
                        return Err(invalid(span));
                    }
                    produced = Some(*owner);
                }
                (Step::Root(i), Operation::RootPlace { owner })
                    if Some(&EntityId::Value(*owner)) == r.entities.get(3 * i) =>
                {
                    let [place @ EntityId::Place(_)] = data.results.as_slice() else {
                        return Err(invalid(span));
                    };
                    if r.entities.len() != 3 * i + 1 || *place != entity(p.operands[i].place)? {
                        return Err(invalid(span));
                    }
                    produced = Some(*place);
                }
                (
                    Step::Begin(i),
                    Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                ) if Some(&EntityId::Place(*place)) == r.entities.get(3 * i + 1) => {
                    let [loan @ EntityId::Loan(id)] = data.results.as_slice() else {
                        return Err(invalid(span));
                    };
                    if r.entities.len() != 3 * i + 2
                        || *loan != entity(p.operands[i].loan)?
                        || id.function() != function.id()
                    {
                        return Err(invalid(span));
                    }
                    produced = Some(*loan);
                }
                (
                    Step::Call,
                    Operation::DirectCall {
                        callee,
                        receiver,
                        arguments,
                    },
                ) if *callee == p.callee
                    && receiver.is_none()
                    && *arguments
                        == p.parameter_loans(span)?
                            .into_iter()
                            .map(|slot| entity(Some(slot)))
                            .collect::<Result<Vec<_>, _>>()?
                    && data.results.len() == usize::from(p.returns_value)
                    && data.results.iter().all(|e| matches!(e, EntityId::Value(_))) => {}
                (Step::End(i), Operation::BorrowEnd { loan })
                    if EntityId::Loan(*loan) == entity(p.operands[i].loan)?
                        && data.results.is_empty() => {}
                (Step::Drop(i), Operation::Drop { owner })
                    if EntityId::Value(*owner) == entity(p.operands[i].owner)?
                        && data.results.is_empty() => {}
                _ => return Err(invalid(span)),
            }
            if let Some(e) = produced {
                if r.entities.contains(&e) {
                    return Err(invalid(span));
                }
                r.entities.push(e);
            }
            self.blocks[tag.plan] = Some(data.block);
            r.instructions.push(instruction);
            Ok(())
        }
        pub(in crate::ssa::refine) fn finish(
            self,
            plans: &Plans,
            function: &Function,
            span: Span,
        ) -> Result<(), LoweringError> {
            if self.receipts.len() != plans.entries.len() || self.blocks.len() != plans.entries.len() {
                return Err(invalid(span));
            }
            for ((r, b), p) in self.receipts.iter().zip(&self.blocks).zip(&plans.entries) {
                if r.ids != p.ids
                    || r.callable != p.callable
                    || r.call != p.call
                    || r.caller != function.id()
                    || instruction_count(p.operands.len()) != Some(r.instructions.len())
                    || r.entities.len() != 3 * p.operands.len()
                {
                    return Err(invalid(span));
                }
                let block = b
                    .and_then(|b| function.block(b))
                    .ok_or_else(|| invalid(span))?;
                if r.instructions
                    .iter()
                    .any(|id| !block.instructions.contains(id))
                {
                    return Err(invalid(span));
                }
            }
            #[cfg(test)]
            ci_tests::record(plans, function, &self.receipts);
            #[cfg(test)]
            RECEIPTS.with(|out| {
                if let Some(out) = out.borrow_mut().as_mut() {
                    out.extend(self.receipts);
                }
            });
            Ok(())
        }
    }
}
use super::body::{BlockRef, ROp, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{FunctionId, LoanKind},
};
pub(super) use emitted::Emitted;
use lang_frontend::{
    ownership_checking::{UnitDropTarget, UnitLoanActionId, UnitLoanFact},
    source::Span,
    type_checking::{UnitExpressionId, UnitItemId},
};
#[derive(Clone, Copy, Debug)]
pub(super) struct Tag {
    pub(super) plan: usize,
    pub(super) role: usize,
}
#[derive(Clone, Debug, Default)]
pub(super) struct Plans {
    entries: Vec<Plan>,
}
#[derive(Clone, Debug)]
struct Plan {
    ids: Vec<UnitLoanActionId>,
    callable: UnitItemId,
    caller: FunctionId,
    call: UnitExpressionId,
    callee: FunctionId,
    returns_value: bool,
    operands: Vec<Operand>,
    stage: usize,
    block: Option<BlockRef>,
}
#[derive(Clone, Debug)]
struct Operand {
    fact: UnitLoanFact,
    parameter_index: usize,
    literal: Option<Vec<u8>>,
    borrowed: bool,
    drop_index: usize,
    owner: Option<Slot>,
    place: Option<Slot>,
    loan: Option<Slot>,
    drop_authorized: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Literal(usize),
    Root(usize),
    Begin(usize),
    Call,
    End(usize),
    Drop(usize),
}
// This single budget also bounds every 2N/3N/4N and 3i+2 index below.
// It is an internal representation check, not a caller routing condition.
fn instruction_count(n: usize) -> Option<usize> {
    if n == 0 {
        return None;
    }
    n.checked_mul(5)?.checked_add(1)
}
impl Plan {
    fn step(&self, stage: usize) -> Option<Step> {
        let n = self.operands.len();
        instruction_count(n)?;
        if stage < 3 * n {
            return Some(match stage % 3 {
                0 => Step::Literal(stage / 3),
                1 => Step::Root(stage / 3),
                _ => Step::Begin(stage / 3),
            });
        }
        if stage == 3 * n {
            Some(Step::Call)
        } else if stage <= 4 * n {
            Some(Step::End(4 * n - stage))
        } else if stage <= 5 * n {
            Some(Step::Drop(5 * n - stage))
        } else {
            None
        }
    }
    fn slots(&self) -> impl Iterator<Item = Option<Slot>> + '_ {
        self.operands
            .iter()
            .flat_map(|a| [a.owner, a.place, a.loan])
    }
    fn parameter_loans(&self, span: Span) -> Result<Vec<Slot>, LoweringError> {
        let mut slots = vec![None; self.operands.len()];
        for operand in &self.operands {
            let slot = slots
                .get_mut(operand.parameter_index)
                .ok_or_else(|| invalid(span))?;
            let loan = operand.loan.ok_or_else(|| invalid(span))?;
            if slot.replace(loan).is_some() {
                return Err(invalid(span));
            }
        }
        slots
            .into_iter()
            .map(|slot| slot.ok_or_else(|| invalid(span)))
            .collect()
    }
}
impl Plans {
    pub(super) fn form(
        &mut self,
        argument: UnitExpressionId,
        bytes: &[u8],
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            let Some(i) = p
                .operands
                .iter()
                .position(|a| a.fact.argument() == argument)
            else {
                continue;
            };
            if p.step(p.stage) != Some(Step::Literal(i)) || p.operands[i].literal.is_some() {
                return Err(invalid(span));
            }
            p.operands[i].literal = Some(bytes.to_vec());
        }
        Ok(())
    }
    pub(super) fn enter(
        &mut self,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        owner: Slot,
        temporaries: &std::collections::BTreeMap<UnitExpressionId, Slot>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = self.entries.iter_mut().find(|p| p.call == call) else {
            return Ok(());
        };
        let i = p
            .operands
            .iter()
            .position(|a| a.fact.argument() == argument)
            .ok_or_else(|| invalid(span))?;
        if p.step(p.stage) != Some(Step::Root(i)) {
            return Err(invalid(span));
        }
        let a = &mut p.operands[i];
        if a.borrowed
            || a.owner != Some(owner)
            || temporaries.get(&argument).copied() != Some(owner)
        {
            return Err(invalid(span));
        }
        a.borrowed = true;
        Ok(())
    }
    pub(super) fn drop_fact(
        &mut self,
        index: usize,
        target: UnitDropTarget,
        slot: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.entries {
            let Some(i) = p.operands.iter().position(|a| a.drop_index == index) else {
                continue;
            };
            if p.step(p.stage) != Some(Step::Drop(i)) {
                return Err(invalid(span));
            }
            let a = &mut p.operands[i];
            if a.drop_authorized
                || a.owner != Some(slot)
                || target != UnitDropTarget::Temporary(a.fact.argument())
            {
                return Err(invalid(span));
            }
            a.drop_authorized = true;
        }
        Ok(())
    }
    pub(super) fn statement(
        &mut self,
        op: &ROp,
        results: &[Slot],
        block: BlockRef,
        span: Span,
    ) -> Result<Option<Tag>, LoweringError> {
        let mut tag = None;
        for (index, p) in self.entries.iter_mut().enumerate() {
            let involved = p.operands.iter().any(|a| {
                if a.literal.is_some() && a.owner.is_none() {
                    return true;
                }
                match op {
                    ROp::RootPlace { owner } | ROp::Drop { owner } => Some(*owner) == a.owner,
                    ROp::BorrowBegin { place, .. } => Some(*place) == a.place,
                    ROp::DirectCall { arguments, .. } => {
                        a.loan.is_some_and(|loan| arguments.contains(&loan))
                    }
                    ROp::BorrowEnd { loan } => Some(*loan) == a.loan,
                    _ => false,
                }
            });
            if !involved {
                continue;
            }
            if tag.is_some() || p.block.is_some_and(|b| b != block) {
                return Err(invalid(span));
            }
            let single = || match results {
                [slot] => Ok(*slot),
                _ => Err(invalid(span)),
            };
            let step = p.step(p.stage).ok_or_else(|| invalid(span))?;
            if matches!(step, Step::Literal(_) | Step::Root(_) | Step::Begin(_))
                && p.slots().flatten().any(|s| results.contains(&s))
            {
                return Err(invalid(span));
            }
            match (step, op) {
                (Step::Literal(i), ROp::StringLiteral { bytes, .. })
                    if Some(bytes) == p.operands[i].literal.as_ref() =>
                {
                    p.operands[i].owner = Some(single()?)
                }
                (Step::Root(i), ROp::RootPlace { owner })
                    if p.operands[i].borrowed && Some(*owner) == p.operands[i].owner =>
                {
                    p.operands[i].place = Some(single()?)
                }
                (
                    Step::Begin(i),
                    ROp::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                ) if Some(*place) == p.operands[i].place => p.operands[i].loan = Some(single()?),
                (
                    Step::Call,
                    ROp::DirectCall {
                        callee,
                        receiver,
                        arguments,
                    },
                ) if *callee == p.callee
                    && receiver.is_none()
                    && *arguments == p.parameter_loans(span)?
                    && results.len() == usize::from(p.returns_value) => {}
                (Step::End(i), ROp::BorrowEnd { loan })
                    if Some(*loan) == p.operands[i].loan && results.is_empty() => {}
                (Step::Drop(i), ROp::Drop { owner })
                    if p.operands[i].drop_authorized
                        && Some(*owner) == p.operands[i].owner
                        && results.is_empty() => {}
                _ => return Err(invalid(span)),
            }
            p.block = Some(block);
            tag = Some(Tag {
                plan: index,
                role: p.stage,
            });
            p.stage += 1;
        }
        Ok(tag)
    }
    pub(super) fn finish(
        &self,
        consumed: &std::collections::BTreeSet<usize>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.entries.iter().any(|p| {
            instruction_count(p.operands.len()) != Some(p.stage)
                || p.operands
                    .iter()
                    .any(|a| !a.drop_authorized || !consumed.contains(&a.drop_index))
        }) {
            return Err(invalid(span));
        }
        Ok(())
    }
}
fn invalid(span: Span) -> LoweringError {
    error(LoweringErrorKind::InvalidModel, span)
}
#[cfg(test)]
thread_local! {static RECEIPTS:std::cell::RefCell<Option<Vec<emitted::Receipt>>>=const{std::cell::RefCell::new(None)};}
#[cfg(test)]
pub(crate) fn observe<T>(lower: impl FnOnce() -> T) -> (T, Vec<emitted::Receipt>) {
    RECEIPTS.with(|r| {
        assert!(r.borrow().is_none());
        *r.borrow_mut() = Some(Vec::new());
    });
    let result = lower();
    (result, RECEIPTS.with(|r| r.borrow_mut().take().unwrap()))
}
