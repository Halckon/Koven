//! Optional bounded protocol-2 verification. The existing producers alone emit effects.
mod emitted;

use super::body::{BlockRef, ROp, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{FunctionId, LoanKind},
};
pub(super) use emitted::Emitted;
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction, UnitLoanAction,
        UnitLoanActionCoverage, UnitLoanActionId, UnitReceiverOwnershipFact,
        UnitReceiverOwnershipTarget,
    },
    source::Span,
    type_checking::{BuiltinType, UnitCallableTarget, UnitExpressionId, UnitItemId, UnitTypeKind},
};

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
    callee: FunctionId,
    fact: UnitReceiverOwnershipFact,
    root: UnitSymbolId,
    drop_index: usize,
    returns_value: bool,
    owner: Option<Slot>,
    place: Option<Slot>,
    loan: Option<Slot>,
    stage: u8,
    drop_authorized: bool,
    block: Option<BlockRef>,
}

impl Plans {
    pub(in crate::ssa::refine) fn new(
        inputs: &super::RefineInputs<'_>,
    ) -> Result<Self, LoweringError> {
        let mut result = Self::default();
        let Some(call) = super::subset::bounded_receiver_call(inputs) else {
            return Ok(result);
        };
        let item = inputs.function_item.ok_or_else(|| invalid(inputs.span))?;
        let callable = UnitItemId::new(inputs.source_unit, item);
        let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
        for observation in inputs.owned.loan_action_observations() {
            for region in observation.regions().iter().filter(|r| {
                r.protocol_version() == 2
                    && r.coverage() == UnitLoanActionCoverage::Complete
                    && r.callable() == Some(callable)
            }) {
                if observation.source() != inputs.source_unit
                    || !observation.owns_region(region)
                    || region.call() != call
                    || !result.entries.is_empty()
                {
                    return Err(fail());
                }
                let ids = region.action_ids().collect::<Vec<_>>();
                let actions = ids
                    .iter()
                    .map(|id| observation.action(id).ok_or_else(fail))
                    .collect::<Result<Vec<_>, _>>()?;
                let [
                    UnitLoanAction::ReceiverBegin(begin),
                    UnitLoanAction::ReceiverEnd(end),
                    UnitLoanAction::Cleanup {
                        point,
                        action: UnitIterationCleanupAction::Drop(drop),
                    },
                ] = actions.as_slice()
                else {
                    return Err(fail());
                };
                let UnitReceiverOwnershipTarget::Place(root) = begin.target() else {
                    return Err(fail());
                };
                if begin.call() != call
                    || end.receiver() != begin
                    || inputs.owned.receiver_fact(call) != Some(begin)
                    || !inputs.owned.receiver_call_return_ends().contains(end)
                    || !root.is_root()
                    || root.root().source_unit() != inputs.source_unit
                    || *point != UnitDropPoint::CallReturn(call)
                    || end.point() != *point
                    || drop.point() != *point
                    || drop.target() != UnitDropTarget::Named(root.root())
                {
                    return Err(fail());
                }
                let indices = inputs
                    .owned
                    .drops()
                    .iter()
                    .enumerate()
                    .filter(|(_, fact)| **fact == *drop)
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                let [drop_index] = indices.as_slice() else {
                    return Err(fail());
                };
                let caller_key = inputs
                    .source_plan
                    .source(inputs.source_token)
                    .ok_or_else(fail)?;
                let UnitCallableTarget::Declaration(id) = caller_key.target() else {
                    return Err(fail());
                };
                let declaration = inputs
                    .names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.id() == id)
                    .ok_or_else(fail)?;
                if UnitItemId::new(declaration.source_unit(), declaration.root()) != callable {
                    return Err(fail());
                }
                let caller = *inputs.function_ids.get(caller_key).ok_or_else(fail)?;
                let site = inputs
                    .source_plan
                    .call_site(inputs.source_token, call)
                    .ok_or_else(fail)?;
                if !site.delegation().is_empty() {
                    return Err(fail());
                }
                let callee = *inputs
                    .function_ids
                    .get(site.dispatch_key().ok_or_else(fail)?)
                    .ok_or_else(fail)?;
                if callee == caller {
                    return Err(fail());
                }
                let ty = inputs
                    .concrete(inputs.typed.call(call).ok_or_else(fail)?.return_type())
                    .ok_or_else(fail)?;
                let returns_value = match inputs.typed.types().get(ty) {
                    Some(UnitTypeKind::Builtin(BuiltinType::Int)) => true,
                    Some(UnitTypeKind::Builtin(BuiltinType::Unit)) => false,
                    _ => return Err(fail()),
                };
                result.entries.push(Plan {
                    ids,
                    callable,
                    caller,
                    call,
                    callee,
                    fact: (*begin).clone(),
                    root: root.root(),
                    drop_index: *drop_index,
                    returns_value,
                    owner: None,
                    place: None,
                    loan: None,
                    stage: 0,
                    drop_authorized: false,
                    block: None,
                });
            }
        }
        Ok(result)
    }

    pub(super) fn enter(
        &mut self,
        call: UnitExpressionId,
        fact: Option<&UnitReceiverOwnershipFact>,
        bindings: &std::collections::BTreeMap<UnitSymbolId, Slot>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(plan) = self.entries.iter_mut().find(|p| p.call == call) else {
            return Ok(());
        };
        if plan.owner.is_some() || fact != Some(&plan.fact) {
            return Err(invalid(span));
        }
        plan.owner = Some(*bindings.get(&plan.root).ok_or_else(|| invalid(span))?);
        Ok(())
    }

    pub(super) fn drop_fact(
        &mut self,
        index: usize,
        target: UnitDropTarget,
        slot: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        for plan in &mut self.entries {
            if plan.drop_index == index {
                if plan.stage != 4
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

    pub(super) fn statement(
        &mut self,
        op: &ROp,
        results: &[Slot],
        block: BlockRef,
        span: Span,
    ) -> Result<Option<Tag>, LoweringError> {
        let mut tag = None;
        for (index, plan) in self.entries.iter_mut().enumerate() {
            let Some(owner) = plan.owner else {
                continue;
            };
            let involved = match op {
                ROp::RootPlace { owner: actual } | ROp::Drop { owner: actual } => *actual == owner,
                ROp::BorrowBegin { place, .. } => Some(*place) == plan.place,
                ROp::DirectCall { receiver, .. } => receiver.is_some() && *receiver == plan.loan,
                ROp::BorrowEnd { loan } => Some(*loan) == plan.loan,
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
                ) => plan.loan = Some(single()?),
                (
                    2,
                    ROp::DirectCall {
                        callee,
                        receiver,
                        arguments,
                    },
                ) if *callee == plan.callee
                    && *receiver == plan.loan
                    && arguments.is_empty()
                    && results.len() == usize::from(plan.returns_value) => {}
                (3, ROp::BorrowEnd { .. }) if results.is_empty() => {}
                (4, ROp::Drop { .. }) if plan.drop_authorized && results.is_empty() => {}
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
        consumed: &std::collections::BTreeSet<usize>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self
            .entries
            .iter()
            .any(|p| p.stage != 5 || !consumed.contains(&p.drop_index))
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
    RECEIPTS.with(|r| {
        assert!(r.borrow().is_none());
        *r.borrow_mut() = Some(Vec::new());
    });
    let result = lower();
    (result, RECEIPTS.with(|r| r.borrow_mut().take().unwrap()))
}
