//! Optional protocol-4 verifier. Existing producers remain the only effect emitters.
mod emitted {
    use super::*;
    #[cfg(test)]
    #[path = "loan_receipt/ci_tests.rs"]
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
                (0, Operation::RootPlace { owner: actual })
                    if EntityId::Value(*actual) == owner()? =>
                {
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
            for ((receipt, block), plan) in
                self.receipts.iter().zip(&self.blocks).zip(&plans.entries)
            {
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
}

use super::body::{BlockRef, ROp, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{FunctionId, LoanKind},
};
pub(super) use emitted::Emitted;
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        BorrowSourceLoan, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction,
        UnitLoanAction, UnitLoanActionCoverage, UnitLoanActionId, UnitLoanTarget,
    },
    source::Span,
    type_checking::{UnitCallableTarget, UnitExpressionId, UnitItemId},
};

/// A role stamped at the original statement producer; it moves with that statement.
#[derive(Clone, Copy, Debug)]
pub(super) struct Tag {
    pub(super) plan: usize,
    pub(super) role: u8,
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
    argument: UnitExpressionId,
    source_call: UnitExpressionId,
    source_argument: UnitExpressionId,
    root: UnitSymbolId,
    source_callee: FunctionId,
    consumer_callee: FunctionId,
    drop_index: usize,
    owner: Option<Slot>,
    place: Option<Slot>,
    source: Option<Slot>,
    result: Option<Slot>,
    stage: u8,
    drop_authorized: bool,
    block: Option<BlockRef>,
}

impl Plans {
    pub(in crate::ssa::refine) fn new(
        inputs: &super::RefineInputs<'_>,
    ) -> Result<Self, LoweringError> {
        let mut result = Self::default();
        let Some(item) = inputs.function_item else {
            return Ok(result);
        };
        let callable = UnitItemId::new(inputs.source_unit, item);
        let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
        for observation in inputs.owned.loan_action_observations() {
            for region in observation.regions().iter().filter(|region| {
                region.protocol_version() == 4
                    && region.coverage() == UnitLoanActionCoverage::Complete
                    && region.callable() == Some(callable)
            }) {
                if observation.source() != inputs.source_unit || !observation.owns_region(region) {
                    return Err(fail());
                }
                let ids = region.action_ids().collect::<Vec<_>>();
                let actions = ids
                    .iter()
                    .map(|id| observation.action(id).ok_or_else(fail))
                    .collect::<Result<Vec<_>, _>>()?;
                let [
                    UnitLoanAction::Begin(source),
                    UnitLoanAction::BorrowConsumerTransfer {
                        source: transferred,
                        consumer: call,
                        argument,
                    },
                    UnitLoanAction::Begin(consumer),
                    UnitLoanAction::BorrowConsumerRetired(retired),
                    UnitLoanAction::Cleanup {
                        point,
                        action: UnitIterationCleanupAction::Drop(drop),
                    },
                ] = actions.as_slice()
                else {
                    return Err(fail());
                };
                let UnitLoanTarget::Place(root) = source.target() else {
                    return Err(fail());
                };
                if source != transferred
                    || consumer.call() != region.call()
                    || *call != region.call()
                    || *argument != source.call()
                    || consumer.argument() != source.call()
                    || source.target() != consumer.target()
                    || !root.is_root()
                    || root.root().source_unit() != inputs.source_unit
                    || retired.source_begin().as_ref() != ids.first()
                    || retired.transfer().as_ref() != ids.get(1)
                    || retired.consumer_begin().as_ref() != ids.get(2)
                    || retired.transaction().ended_loans() != [consumer, source]
                    || *point != UnitDropPoint::CallReturn(*call)
                    || drop.point() != *point
                    || drop.target() != UnitDropTarget::Named(root.root())
                    || result.entries.iter().any(|entry| entry.call == *call)
                {
                    return Err(fail());
                }
                let indices = inputs
                    .owned
                    .drops()
                    .iter()
                    .enumerate()
                    .filter(|(_, fact)| **fact == *drop)
                    .map(|(index, _)| index)
                    .collect::<Vec<_>>();
                let [drop_index] = indices.as_slice() else {
                    return Err(fail());
                };
                let caller_key = inputs
                    .source_plan
                    .source(inputs.source_token)
                    .ok_or_else(fail)?;
                let UnitCallableTarget::Declaration(declaration) = caller_key.target() else {
                    return Err(fail());
                };
                let declaration = inputs
                    .names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|candidate| candidate.id() == declaration)
                    .ok_or_else(fail)?;
                if UnitItemId::new(declaration.source_unit(), declaration.root()) != callable {
                    return Err(fail());
                }
                let caller = *inputs.function_ids.get(caller_key).ok_or_else(fail)?;
                if [*call, *argument, source.call(), source.argument()]
                    .iter()
                    .any(|expression| expression.source_unit() != inputs.source_unit)
                {
                    return Err(fail());
                }
                let callee = |call| {
                    let site = inputs
                        .source_plan
                        .call_site(inputs.source_token, call)
                        .ok_or_else(fail)?;
                    if !site.delegation().is_empty() {
                        return Err(fail());
                    }
                    let key = site.dispatch_key().ok_or_else(fail)?;
                    inputs.function_ids.get(key).copied().ok_or_else(fail)
                };
                result.entries.push(Plan {
                    ids,
                    callable,
                    caller,
                    call: *call,
                    argument: *argument,
                    source_call: source.call(),
                    source_argument: source.argument(),
                    root: root.root(),
                    source_callee: callee(source.call())?,
                    consumer_callee: callee(*call)?,
                    drop_index: *drop_index,
                    owner: None,
                    place: None,
                    source: None,
                    result: None,
                    stage: 0,
                    drop_authorized: false,
                    block: None,
                });
            }
        }
        Ok(result)
    }

    /// Called from the actual validated nested call-use, before source lowering starts.
    pub(super) fn enter(
        &mut self,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        source: BorrowSourceLoan<UnitExpressionId>,
        bindings: &std::collections::BTreeMap<UnitSymbolId, Slot>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(plan) = self.entries.iter_mut().find(|plan| plan.call == call) else {
            return Ok(());
        };
        if plan.owner.is_some()
            || argument != plan.argument
            || source.call() != plan.source_call
            || source.argument() != plan.source_argument
        {
            return Err(invalid(span));
        }
        plan.owner = Some(*bindings.get(&plan.root).ok_or_else(|| invalid(span))?);
        Ok(())
    }

    /// Called from the existing named Drop producer, without changing its authorization.
    pub(super) fn drop_fact(
        &mut self,
        index: usize,
        target: UnitDropTarget,
        slot: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        for plan in &mut self.entries {
            if plan.drop_index == index {
                if plan.stage != 6
                    || plan.drop_authorized
                    || plan.owner != Some(slot)
                    || target != UnitDropTarget::Named(plan.root)
                {
                    return Err(invalid(span));
                }
                plan.drop_authorized = true;
            }
        }
        Ok(())
    }

    /// Observe a real producer invocation, never manufacture an operation or search by Span.
    pub(super) fn statement(
        &mut self,
        op: &ROp,
        results: &[Slot],
        block: BlockRef,
        span: Span,
    ) -> Result<Option<Tag>, LoweringError> {
        let mut tag = None;
        for (index, plan) in self.entries.iter_mut().enumerate() {
            let Some(owner) = plan.owner else { continue };
            let involved = match op {
                ROp::RootPlace { owner: actual } | ROp::Drop { owner: actual } => *actual == owner,
                ROp::BorrowBegin { place, .. } => Some(*place) == plan.place,
                ROp::BorrowCall { source, .. } => Some(*source) == plan.source,
                ROp::DirectCall { arguments, .. } => {
                    plan.result.is_some_and(|r| arguments.contains(&r))
                }
                ROp::BorrowEnd { loan } => Some(*loan) == plan.source || Some(*loan) == plan.result,
                _ => false,
            };
            if !involved {
                continue;
            }
            if tag.is_some() || plan.block.is_some_and(|previous| previous != block) {
                return Err(invalid(span));
            }
            let single = || match results {
                [slot] => Ok(*slot),
                _ => Err(invalid(span)),
            };
            match (plan.stage, op) {
                (0, ROp::RootPlace { .. }) => plan.place = Some(single()?),
                (
                    1,
                    ROp::BorrowBegin {
                        kind: LoanKind::Shared,
                        ..
                    },
                ) => plan.source = Some(single()?),
                (
                    2,
                    ROp::BorrowCall {
                        callee,
                        arguments,
                        source,
                    },
                ) if *callee == plan.source_callee && arguments == &[*source] => {
                    plan.result = Some(single()?);
                    if plan.result == plan.source {
                        return Err(invalid(span));
                    }
                }
                (
                    3,
                    ROp::DirectCall {
                        callee,
                        receiver: None,
                        arguments,
                    },
                ) if *callee == plan.consumer_callee
                    && arguments.as_slice() == plan.result.as_slice() => {}
                (4, ROp::BorrowEnd { loan })
                    if Some(*loan) == plan.result && results.is_empty() => {}
                (5, ROp::BorrowEnd { loan })
                    if Some(*loan) == plan.source && results.is_empty() => {}
                (6, ROp::Drop { .. }) if plan.drop_authorized && results.is_empty() => {}
                _ => return Err(invalid(span)),
            }
            plan.block = Some(block);
            tag = Some(Tag {
                plan: index,
                role: plan.stage,
            });
            plan.stage += 1;
        }
        Ok(tag)
    }

    pub(super) fn finish(
        &self,
        consumed_drops: &std::collections::BTreeSet<usize>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self
            .entries
            .iter()
            .any(|plan| plan.stage != 7 || !consumed_drops.contains(&plan.drop_index))
        {
            return Err(invalid(span));
        }
        Ok(())
    }
}

fn invalid(span: Span) -> LoweringError {
    error(LoweringErrorKind::InvalidModel, span)
}

#[cfg(test)]
thread_local! {
    static RECEIPTS: std::cell::RefCell<Option<Vec<emitted::Receipt>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn observe<T>(lower: impl FnOnce() -> T) -> (T, Vec<emitted::Receipt>) {
    RECEIPTS.with(|receipts| {
        assert!(receipts.borrow().is_none());
        *receipts.borrow_mut() = Some(Vec::new());
    });
    let result = lower();
    let receipts = RECEIPTS.with(|receipts| receipts.borrow_mut().take().unwrap());
    (result, receipts)
}
