//! Freeze the seven producer roles at append_instruction, never by a later origin scan.
use super::*;
#[cfg(test)]
#[path = "ci_tests.rs"]
mod ci_tests;
use crate::ssa::model::{BlockId, EntityId, Function, InstructionId, Operation};

#[derive(Clone, Debug)]
pub(crate) struct Receipt {
    pub(crate) ids: Vec<UnitLoanActionId>,
    pub(crate) callable: UnitItemId,
    pub(crate) caller: FunctionId,
    pub(crate) binding: UnitSymbolId,
    pub(crate) root: UnitSymbolId,
    pub(crate) source_call: UnitExpressionId,
    pub(crate) consumer_call: UnitExpressionId,
    pub(crate) entry_statement: UnitStatementId,
    pub(crate) completion_statement: UnitStatementId,
    pub(crate) instructions: Vec<InstructionId>,
    /// Existing owner, root place, source loan, borrowed result. No consumer loan.
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
            if p.stage != 7
                || !p.completed
                || !p.retired
                || !p.committed
                || !p.consumer_authorized
                || !p.drop_authorized
                || !p.activated
                || !p.source_authorized
                || p.caller != function.id()
            {
                return Err(invalid(span));
            }
            receipts.push(Receipt {
                ids: p.ids.clone(),
                callable: p.callable,
                caller: p.caller,
                binding: p.binding,
                root: p.root,
                source_call: p.source_call,
                consumer_call: p.consumer_call,
                entry_statement: p.entry_statement,
                completion_statement: p.completion_statement,
                instructions: Vec::new(),
                entities: Vec::new(),
            });
        }
        Ok(Self {
            blocks: vec![None; receipts.len()],
            receipts,
        })
    }
    fn frozen(
        p: &Plan,
        r: &Receipt,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        for (slot, frozen) in [p.owner, p.place, p.source, p.result]
            .into_iter()
            .zip(&r.entities)
        {
            if slot.and_then(|s| current.get(&s)) != Some(frozen) {
                return Err(invalid(span));
            }
        }
        Ok(())
    }
    /// Alias creates no instruction, but must never replace an already witnessed entity.
    pub(in crate::ssa::refine) fn alias(
        &self,
        plans: &Plans,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        for (p, r) in plans.entries.iter().zip(&self.receipts) {
            Self::frozen(p, r, current, span)?;
        }
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
        Self::frozen(p, r, current, span)?;
        let entity = |slot: Option<Slot>| {
            slot.and_then(|s| current.get(&s).copied())
                .ok_or_else(|| invalid(span))
        };
        match (tag.role, &data.operation) {
            (0, Operation::RootPlace { owner }) if EntityId::Value(*owner) == entity(p.owner)? => {
                let [place @ EntityId::Place(_)] = data.results.as_slice() else {
                    return Err(invalid(span));
                };
                if *place != entity(p.place)? || owner.function() != function.id() {
                    return Err(invalid(span));
                }
                // The local owner was established before region entry, not a function parameter.
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
                if *loan != entity(p.source)? || id.function() != function.id() {
                    return Err(invalid(span));
                }
                r.entities.push(*loan);
            }
            (
                2,
                Operation::BorrowCall {
                    callee,
                    arguments,
                    source,
                },
            ) if *callee == p.source_callee
                && EntityId::Loan(*source) == entity(p.source)?
                && arguments.as_slice() == [entity(p.source)?] =>
            {
                let [loan @ EntityId::Loan(id)] = data.results.as_slice() else {
                    return Err(invalid(span));
                };
                if *loan != entity(p.result)?
                    || *loan == entity(p.source)?
                    || id.function() != function.id()
                {
                    return Err(invalid(span));
                }
                r.entities.push(*loan);
            }
            (
                3,
                Operation::DirectCall {
                    callee,
                    receiver: None,
                    arguments,
                },
            ) if *callee == p.consumer_callee
                && arguments.as_slice() == [entity(p.result)?]
                && data.results.is_empty() => {}
            (4, Operation::BorrowEnd { loan })
                if EntityId::Loan(*loan) == entity(p.result)? && data.results.is_empty() => {}
            (5, Operation::BorrowEnd { loan })
                if EntityId::Loan(*loan) == entity(p.source)? && data.results.is_empty() => {}
            (6, Operation::Drop { owner })
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
            if !p.completed
                || p.stage != 7
                || r.ids != p.ids
                || r.callable != p.callable
                || r.caller != function.id()
                || r.binding != p.binding
                || r.root != p.root
                || r.source_call != p.source_call
                || r.consumer_call != p.consumer_call
                || r.entry_statement != p.entry_statement
                || r.completion_statement != p.completion_statement
                || r.instructions.len() != 7
                || r.entities.len() != 4
            {
                return Err(invalid(span));
            }
            let block = b
                .and_then(|b| function.block(b))
                .ok_or_else(|| invalid(span))?;
            if !block
                .instructions
                .windows(7)
                .any(|ids| ids == r.instructions.as_slice())
            {
                return Err(invalid(span));
            }
        }
        #[cfg(test)]
        ci_tests::record(plans, function, &self.receipts, span);
        #[cfg(test)]
        RECEIPTS.with(|output| {
            if let Some(output) = output.borrow_mut().as_mut() {
                output.extend(self.receipts);
            }
        });
        Ok(())
    }
}
