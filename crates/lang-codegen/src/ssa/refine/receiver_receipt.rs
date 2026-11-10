//! Optional bounded protocol-2 verification. The existing producers alone emit effects.
mod admission;
mod emitted;
use super::body::{BlockRef, ROp, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{FunctionId, LoanKind},
};
pub(super) use emitted::Emitted;
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{UnitDropTarget, UnitLoanActionId, UnitReceiverOwnershipFact},
    source::Span,
    type_checking::{UnitExpressionId, UnitItemId},
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
thread_local! { static RECEIPTS: std::cell::RefCell<Option<Vec<emitted::Receipt>>> = const { std::cell::RefCell::new(None) }; }
#[cfg(test)]
pub(crate) fn observe<T>(lower: impl FnOnce() -> T) -> (T, Vec<emitted::Receipt>) {
    RECEIPTS.with(|r| {
        assert!(r.borrow().is_none());
        *r.borrow_mut() = Some(Vec::new());
    });
    let result = lower();
    (result, RECEIPTS.with(|r| r.borrow_mut().take().unwrap()))
}
