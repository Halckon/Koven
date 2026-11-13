//! Record actual append and set_terminator outcomes, never discover them by scanning.
use super::*;
#[cfg(test)]
#[path = "ci_tests.rs"]
mod ci_tests;
use crate::ssa::model::{
    BlockId, EntityId, Function, InstructionId, LoanId, Operation, TerminatorKind,
};
#[derive(Clone, Debug)]
pub(crate) struct Receipt {
    pub(crate) ids: Vec<UnitLoanActionId>,
    pub(crate) callable: UnitItemId,
    pub(crate) caller: FunctionId,
    pub(crate) call: UnitExpressionId,
    pub(crate) block: BlockId,
    pub(crate) instruction: InstructionId,
    pub(crate) source: LoanId,
    pub(crate) result: LoanId,
}
pub(in crate::ssa::refine) struct Emitted {
    receipt: Option<Receipt>,
    returned: bool,
}
impl Emitted {
    pub(in crate::ssa::refine) fn new(
        plans: &Plans,
        function: &Function,
        span: Span,
    ) -> Result<Self, LoweringError> {
        plans.finish(span)?;
        if plans.entries.iter().any(|p| p.caller != function.id()) {
            return Err(invalid(span));
        }
        Ok(Self {
            receipt: None,
            returned: false,
        })
    }
    pub(in crate::ssa::refine) fn statement(
        &mut self,
        plans: &Plans,
        _tag: Tag,
        function: &Function,
        instruction: InstructionId,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let [plan] = plans.entries.as_slice() else {
            return Err(invalid(span));
        };
        if self.receipt.is_some() || function.id() != plan.caller {
            return Err(invalid(span));
        }
        let data = function
            .instruction(instruction)
            .ok_or_else(|| invalid(span))?;
        let entity = |s: Option<Slot>| {
            s.and_then(|s| current.get(&s).copied())
                .ok_or_else(|| invalid(span))
        };
        let (EntityId::Loan(source), EntityId::Loan(result)) =
            (entity(plan.source)?, entity(plan.result)?)
        else {
            return Err(invalid(span));
        };
        if source.function() != plan.caller
            || result.function() != plan.caller
            || source == result
            || data.operation
                != (Operation::BorrowCall {
                    callee: plan.callee,
                    arguments: vec![EntityId::Loan(source)],
                    source,
                })
            || data.results != [EntityId::Loan(result)]
        {
            return Err(invalid(span));
        }
        let block = function.block(data.block).ok_or_else(|| invalid(span))?;
        if block.parameters != [EntityId::Loan(source)] {
            return Err(invalid(span));
        }
        self.receipt = Some(Receipt {
            ids: plan.ids.clone(),
            callable: plan.callable,
            caller: plan.caller,
            call: plan.call,
            block: data.block,
            instruction,
            source,
            result,
        });
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
        let Some(plan) = plans.entries.first() else {
            return Ok(());
        };
        let r = self.receipt.as_ref().ok_or_else(|| invalid(span))?;
        if self.returned
            || r.block != block
            || function.id() != r.caller
            || plan.source.and_then(|s| current.get(&s)) != Some(&EntityId::Loan(r.source))
            || plan.result.and_then(|s| current.get(&s)) != Some(&EntityId::Loan(r.result))
        {
            return Err(invalid(span));
        }
        let actual = function
            .block(block)
            .and_then(|b| b.terminator.as_ref())
            .ok_or_else(|| invalid(span))?;
        if actual.kind != (TerminatorKind::BorrowReturn { loan: r.result }) {
            return Err(invalid(span));
        }
        self.returned = true;
        Ok(())
    }
    pub(in crate::ssa::refine) fn finish(
        self,
        plans: &Plans,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(plan) = plans.entries.first() else {
            return if self.receipt.is_none() && !self.returned {
                Ok(())
            } else {
                Err(invalid(span))
            };
        };
        let r = self.receipt.ok_or_else(|| invalid(span))?;
        if !self.returned
            || function.id() != r.caller
            || function.blocks.len() != 1
            || function.instructions.len() != 1
            || r.ids != plan.ids
            || r.callable != plan.callable
            || r.call != plan.call
            || function.instructions[0].id != r.instruction
        {
            return Err(invalid(span));
        }
        let data = function
            .instruction(r.instruction)
            .ok_or_else(|| invalid(span))?;
        if data.block != r.block
            || data.results != [EntityId::Loan(r.result)]
            || data.operation
                != (Operation::BorrowCall {
                    callee: plan.callee,
                    arguments: vec![EntityId::Loan(r.source)],
                    source: r.source,
                })
            || function.blocks[0].parameters != [EntityId::Loan(r.source)]
            || function.blocks[0].terminator.as_ref().map(|t| &t.kind)
                != Some(&TerminatorKind::BorrowReturn { loan: r.result })
        {
            return Err(invalid(span));
        }
        #[cfg(test)]
        ci_tests::record(plans, function, &r, span);
        #[cfg(test)]
        RECEIPTS.with(|v| {
            if let Some(v) = v.borrow_mut().as_mut() {
                v.push(r);
            }
        });
        Ok(())
    }
}
