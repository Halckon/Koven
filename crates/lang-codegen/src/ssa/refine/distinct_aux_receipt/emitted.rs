//! Freeze eleven actual producer roles for two distinct owners, including eager auxiliary drop.
use super::*;
use crate::ssa::model::{BlockId, EntityId, Function, InstructionId, Operation};

#[derive(Clone, Debug)]
pub(crate) struct Receipt {
    pub(crate) ids: Vec<UnitLoanActionId>,
    pub(crate) callable: UnitItemId,
    pub(crate) caller: FunctionId,
    pub(crate) binding: UnitSymbolId,
    pub(crate) root: UnitSymbolId,
    pub(crate) auxiliary_root: UnitSymbolId,
    pub(crate) source_call: UnitExpressionId,
    pub(crate) auxiliary_argument: UnitExpressionId,
    pub(crate) consumer_call: UnitExpressionId,
    pub(crate) entry_statement: UnitStatementId,
    pub(crate) completion_statement: UnitStatementId,
    pub(crate) instructions: Vec<InstructionId>,
    /// Source owner, auxiliary owner, source place/loan, auxiliary place/loan, result.
    pub(crate) entities: Vec<EntityId>,
}
pub(in crate::ssa::refine) struct Emitted {
    receipts: Vec<Receipt>,
    blocks: Vec<Option<BlockId>>,
    /// Captured at each actual append, with semantic indices independent of argument order.
    frozen: Vec<[Option<EntityId>; 7]>,
}
fn slots(p: &Plan) -> [Option<Slot>; 7] {
    [
        p.owner,
        p.auxiliary_owner,
        p.place,
        p.source,
        p.auxiliary_place,
        p.auxiliary,
        p.result,
    ]
}
fn complete(p: &Plan) -> bool {
    p.source_index <= 1
        && p.stage == 11
        && p.completed
        && p.retired
        && p.committed
        && p.consumer_authorized
        && p.drop_authorized
        && p.auxiliary_drop_authorized
        && p.activated
        && p.source_authorized
        && p.auxiliary_authorized
        && p.root != p.auxiliary_root
        && p.drop_index != p.auxiliary_drop_index
        && slots(p).iter().all(Option::is_some)
        && slots(p).iter().collect::<BTreeSet<_>>().len() == 7
}
impl Emitted {
    pub(in crate::ssa::refine) fn new(
        plans: &Plans,
        function: &Function,
        span: Span,
    ) -> Result<Self, LoweringError> {
        let mut receipts = Vec::new();
        for p in &plans.entries {
            if !complete(p) || p.caller != function.id() {
                return Err(invalid(span));
            }
            receipts.push(Receipt {
                ids: p.ids.clone(),
                callable: p.callable,
                caller: p.caller,
                binding: p.binding,
                root: p.root,
                auxiliary_root: p.auxiliary_root,
                source_call: p.source_call,
                auxiliary_argument: p.auxiliary_argument,
                consumer_call: p.consumer_call,
                entry_statement: p.entry_statement,
                completion_statement: p.completion_statement,
                instructions: Vec::new(),
                entities: Vec::new(),
            });
        }
        Ok(Self {
            blocks: vec![None; receipts.len()],
            frozen: vec![[None; 7]; receipts.len()],
            receipts,
        })
    }
    fn check_frozen(
        p: &Plan,
        captured: &[Option<EntityId>; 7],
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        for (slot, frozen) in slots(p).into_iter().zip(captured) {
            if let Some(frozen) = frozen
                && slot.and_then(|s| current.get(&s)) != Some(frozen)
            {
                return Err(invalid(span));
            }
        }
        Ok(())
    }
    pub(in crate::ssa::refine) fn alias(
        &self,
        plans: &Plans,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.frozen.len() != plans.entries.len() || self.receipts.len() != plans.entries.len() {
            return Err(invalid(span));
        }
        for (p, captured) in plans.entries.iter().zip(&self.frozen) {
            Self::check_frozen(p, captured, current, span)?;
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
        let frozen = self.frozen.get_mut(tag.plan).ok_or_else(|| invalid(span))?;
        let block = self.blocks.get_mut(tag.plan).ok_or_else(|| invalid(span))?;
        let data = function
            .instruction(instruction)
            .ok_or_else(|| invalid(span))?;
        if p.source_index > 1
            || r.caller != function.id()
            || r.instructions.len() != usize::from(tag.role)
            || block.is_some_and(|b| b != data.block)
        {
            return Err(invalid(span));
        }
        Self::check_frozen(p, frozen, current, span)?;
        let entity = |slot: Option<Slot>| {
            slot.and_then(|s| current.get(&s).copied())
                .ok_or_else(|| invalid(span))
        };
        match (tag.role, &data.operation) {
            (role @ (0 | 2), Operation::RootPlace { owner }) => {
                let source = usize::from(role / 2) == p.source_index;
                let (owner_index, place_index, owner_slot, place_slot) = if source {
                    (0, 2, p.owner, p.place)
                } else {
                    (1, 4, p.auxiliary_owner, p.auxiliary_place)
                };
                let owner_entity = EntityId::Value(*owner);
                let [place @ EntityId::Place(_)] = data.results.as_slice() else {
                    return Err(invalid(span));
                };
                let owners = [entity(p.owner)?, entity(p.auxiliary_owner)?];
                if !owners.iter().all(|owner| matches!(owner, EntityId::Value(value) if value.function() == function.id()))
                    || owners[0] == owners[1]
                    || owner_entity != entity(owner_slot)?
                    || *place != entity(place_slot)?
                    || owner.function() != function.id()
                    || (role == 0 && frozen.iter().any(Option::is_some))
                    || (role == 2 && (frozen[0] != Some(owners[0]) || frozen[1] != Some(owners[1])))
                    || frozen[place_index].is_some()
                    || frozen.iter().flatten().any(|e| e == place)
                    || owners[owner_index] != owner_entity
                {
                    return Err(invalid(span));
                }
                // Both owners already exist at this boundary. Freeze both before an alias
                // can run between the two RootPlace appends, regardless of source index.
                if role == 0 {
                    frozen[0] = Some(owners[0]);
                    frozen[1] = Some(owners[1]);
                }
                frozen[place_index] = Some(*place);
            }
            (
                role @ (1 | 3),
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
            ) => {
                let source = usize::from(role / 2) == p.source_index;
                let (place_index, loan_index, place_slot, loan_slot) = if source {
                    (2, 3, p.place, p.source)
                } else {
                    (4, 5, p.auxiliary_place, p.auxiliary)
                };
                let [loan @ EntityId::Loan(id)] = data.results.as_slice() else {
                    return Err(invalid(span));
                };
                if frozen[place_index] != Some(EntityId::Place(*place))
                    || EntityId::Place(*place) != entity(place_slot)?
                    || *loan != entity(loan_slot)?
                    || id.function() != function.id()
                    || frozen[loan_index].is_some()
                    || frozen.iter().flatten().any(|e| e == loan)
                {
                    return Err(invalid(span));
                }
                frozen[loan_index] = Some(*loan);
            }
            (
                4,
                Operation::BorrowCall {
                    callee,
                    arguments,
                    source,
                },
            ) if *callee == p.source_callee
                && EntityId::Loan(*source) == entity(p.source)?
                && arguments.as_slice()
                    == [entity(p.ordered_loans()[0])?, entity(p.ordered_loans()[1])?] =>
            {
                let [loan @ EntityId::Loan(id)] = data.results.as_slice() else {
                    return Err(invalid(span));
                };
                if *loan != entity(p.result)?
                    || id.function() != function.id()
                    || frozen[..6].iter().any(Option::is_none)
                    || frozen[6].is_some()
                    || frozen.iter().flatten().any(|e| e == loan)
                {
                    return Err(invalid(span));
                }
                frozen[6] = Some(*loan);
                r.entities = frozen
                    .iter()
                    .copied()
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| invalid(span))?;
            }
            (5, Operation::BorrowEnd { loan })
                if EntityId::Loan(*loan) == entity(p.auxiliary)? && data.results.is_empty() => {}
            (6, Operation::Drop { owner })
                if EntityId::Value(*owner) == entity(p.auxiliary_owner)?
                    && data.results.is_empty() => {}
            (
                7,
                Operation::DirectCall {
                    callee,
                    receiver: None,
                    arguments,
                },
            ) if *callee == p.consumer_callee
                && arguments.as_slice() == [entity(p.result)?]
                && data.results.is_empty() => {}
            (8, Operation::BorrowEnd { loan })
                if EntityId::Loan(*loan) == entity(p.result)? && data.results.is_empty() => {}
            (9, Operation::BorrowEnd { loan })
                if EntityId::Loan(*loan) == entity(p.source)? && data.results.is_empty() => {}
            (10, Operation::Drop { owner })
                if EntityId::Value(*owner) == entity(p.owner)? && data.results.is_empty() => {}
            _ => return Err(invalid(span)),
        }
        *block = Some(data.block);
        r.instructions.push(instruction);
        Ok(())
    }
    pub(in crate::ssa::refine) fn finish(
        self,
        plans: &Plans,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.receipts.len() != plans.entries.len()
            || self.blocks.len() != plans.entries.len()
            || self.frozen.len() != plans.entries.len()
        {
            return Err(invalid(span));
        }
        for (((r, b), captured), p) in self
            .receipts
            .iter()
            .zip(&self.blocks)
            .zip(&self.frozen)
            .zip(&plans.entries)
        {
            if !complete(p)
                || r.ids != p.ids
                || r.callable != p.callable
                || r.caller != function.id()
                || r.binding != p.binding
                || r.root != p.root
                || r.auxiliary_root != p.auxiliary_root
                || r.source_call != p.source_call
                || r.auxiliary_argument != p.auxiliary_argument
                || r.consumer_call != p.consumer_call
                || r.entry_statement != p.entry_statement
                || r.completion_statement != p.completion_statement
                || r.instructions.len() != 11
                || r.entities.len() != 7
                || captured
                    .iter()
                    .copied()
                    .collect::<Option<Vec<_>>>()
                    .as_ref()
                    != Some(&r.entities)
                || r.entities.iter().collect::<BTreeSet<_>>().len() != 7
            {
                return Err(invalid(span));
            }
            let block = b
                .and_then(|b| function.block(b))
                .ok_or_else(|| invalid(span))?;
            if !block
                .instructions
                .windows(11)
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
                output.pending.extend(self.receipts);
            }
        });
        Ok(())
    }
}

#[cfg(test)]
#[path = "ci_tests.rs"]
mod ci_tests;
