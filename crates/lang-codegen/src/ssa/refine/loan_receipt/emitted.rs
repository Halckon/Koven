//! Bind producer-stamped statements to the exact SSA append result, not a later Span scan.
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
    /// Owner, root place, source loan, and result loan, in distinct local roles.
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
        for plan in &plans.entries {
            if plan.stage != 7 || plan.caller != function.id() {
                return Err(invalid(span));
            }
            receipts.push(Receipt {
                ids: plan.ids.clone(),
                callable: plan.callable,
                caller: function.id(),
                call: plan.call,
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
        let plan = plans.entries.get(tag.plan).ok_or_else(|| invalid(span))?;
        let receipt = self
            .receipts
            .get_mut(tag.plan)
            .ok_or_else(|| invalid(span))?;
        let data = function
            .instruction(instruction)
            .ok_or_else(|| invalid(span))?;
        let block = data.block;
        if receipt.caller != function.id()
            || receipt.instructions.len() != usize::from(tag.role)
            || self.blocks[tag.plan].is_some_and(|previous| previous != block)
        {
            return Err(invalid(span));
        }
        let entity = |slot: Option<Slot>| {
            slot.and_then(|slot| current.get(&slot).copied())
                .ok_or_else(|| invalid(span))
        };
        // Once a role has an entity, even an unmarked Alias must not rebind it.
        for (slot, frozen) in [plan.owner, plan.place, plan.source, plan.result]
            .into_iter()
            .zip(&receipt.entities)
        {
            if entity(slot)? != *frozen {
                return Err(invalid(span));
            }
        }
        let owner = || entity(plan.owner);
        let source = || entity(plan.source);
        let result = || entity(plan.result);
        match (tag.role, &data.operation) {
            (0, Operation::RootPlace { owner: actual }) if EntityId::Value(*actual) == owner()? => {
                let [place @ EntityId::Place(_)] = data.results.as_slice() else {
                    return Err(invalid(span));
                };
                if Some(*place)
                    != current
                        .get(&plan.place.ok_or_else(|| invalid(span))?)
                        .copied()
                {
                    return Err(invalid(span));
                }
                receipt.entities.extend([owner()?, *place]);
            }
            (
                1,
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
            ) if Some(&EntityId::Place(*place)) == receipt.entities.get(1) => {
                let [loan @ EntityId::Loan(_)] = data.results.as_slice() else {
                    return Err(invalid(span));
                };
                if *loan != source()? {
                    return Err(invalid(span));
                }
                receipt.entities.push(*loan);
            }
            (
                2,
                Operation::BorrowCall {
                    callee,
                    arguments,
                    source: actual,
                },
            ) if *callee == plan.source_callee
                && EntityId::Loan(*actual) == source()?
                && arguments == &[source()?] =>
            {
                let [loan @ EntityId::Loan(_)] = data.results.as_slice() else {
                    return Err(invalid(span));
                };
                if *loan != result()? || *loan == source()? {
                    return Err(invalid(span));
                }
                receipt.entities.push(*loan);
            }
            (
                3,
                Operation::DirectCall {
                    callee,
                    receiver: None,
                    arguments,
                },
            ) if *callee == plan.consumer_callee && arguments == &[result()?] => {}
            (4, Operation::BorrowEnd { loan })
                if EntityId::Loan(*loan) == result()? && data.results.is_empty() => {}
            (5, Operation::BorrowEnd { loan })
                if EntityId::Loan(*loan) == source()? && data.results.is_empty() => {}
            (6, Operation::Drop { owner: value })
                if EntityId::Value(*value) == owner()? && data.results.is_empty() => {}
            _ => return Err(invalid(span)),
        }
        self.blocks[tag.plan] = Some(block);
        receipt.instructions.push(instruction);
        Ok(())
    }

    pub(in crate::ssa::refine) fn finish(
        self,
        plans: &Plans,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        for ((receipt, block), plan) in self.receipts.iter().zip(&self.blocks).zip(&plans.entries) {
            if receipt.ids != plan.ids
                || receipt.callable != plan.callable
                || receipt.call != plan.call
                || receipt.caller != function.id()
                || receipt.instructions.len() != 7
                || receipt.entities.len() != 4
            {
                return Err(invalid(span));
            }
            let block = block
                .and_then(|block| function.block(block))
                .ok_or_else(|| invalid(span))?;
            // This region closes at the consumer's normal continuation. Later plain
            // calls may terminate the function; they do not undo the already closed region.
            if receipt
                .instructions
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
