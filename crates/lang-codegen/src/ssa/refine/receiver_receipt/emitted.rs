//! Check producer-stamped P2 statements at the actual SSA append result.
use super::*;
#[cfg(test)]
#[path = "ci_tests.rs"]
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
    /// Original parameter owner, root place, and caller-local receiver loan.
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
            if p.stage != 5 || p.caller != function.id() {
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
            || r.instructions.len() != usize::from(tag.role)
            || self.blocks[tag.plan].is_some_and(|b| b != data.block)
        {
            return Err(invalid(span));
        }
        let entity = |slot: Option<Slot>| {
            slot.and_then(|s| current.get(&s).copied())
                .ok_or_else(|| invalid(span))
        };
        for (slot, frozen) in [p.owner, p.place, p.loan].into_iter().zip(&r.entities) {
            if entity(slot)? != *frozen {
                return Err(invalid(span));
            }
        }
        match (tag.role, &data.operation) {
            (0, Operation::RootPlace { owner }) if EntityId::Value(*owner) == entity(p.owner)? => {
                let [place @ EntityId::Place(_)] = data.results.as_slice() else {
                    return Err(invalid(span));
                };
                if *place != entity(p.place)? {
                    return Err(invalid(span));
                }
                // The root is the existing owned parameter, never a recreated payload owner.
                if !function
                    .blocks
                    .first()
                    .is_some_and(|b| b.parameters.contains(&EntityId::Value(*owner)))
                {
                    return Err(invalid(span));
                }
                r.entities.extend([EntityId::Value(*owner), *place]);
            }
            (
                1,
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
            ) if Some(&EntityId::Place(*place)) == r.entities.get(1) => {
                let [loan @ EntityId::Loan(id)] = data.results.as_slice() else {
                    return Err(invalid(span));
                };
                if *loan != entity(p.loan)? || id.function() != function.id() {
                    return Err(invalid(span));
                }
                r.entities.push(*loan);
            }
            (
                2,
                Operation::DirectCall {
                    callee,
                    receiver,
                    arguments,
                },
            ) if *callee == p.callee
                && *receiver == Some(entity(p.loan)?)
                && arguments.is_empty()
                && data.results.len() == usize::from(p.returns_value)
                && data.results.iter().all(|e| matches!(e, EntityId::Value(_))) => {}
            (3, Operation::BorrowEnd { loan })
                if EntityId::Loan(*loan) == entity(p.loan)? && data.results.is_empty() => {}
            (4, Operation::Drop { owner })
                if EntityId::Value(*owner) == entity(p.owner)? && data.results.is_empty() => {}
            _ => return Err(invalid(span)),
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
                || r.instructions.len() != 5
                || r.entities.len() != 3
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
        RECEIPTS.with(|output| {
            if let Some(output) = output.borrow_mut().as_mut() {
                output.extend(self.receipts);
            }
        });
        Ok(())
    }
}
