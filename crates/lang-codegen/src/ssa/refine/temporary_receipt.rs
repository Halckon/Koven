//! Optional bounded protocol-1 verification. The existing producers alone emit effects.
mod admission;
mod emitted;
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
    fact: UnitLoanFact,
    literal: Option<Vec<u8>>,
    borrowed: bool,
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
    pub(super) fn form(
        &mut self,
        argument: UnitExpressionId,
        bytes: &[u8],
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(plan) = self
            .entries
            .iter_mut()
            .find(|p| p.fact.argument() == argument)
        else {
            return Ok(());
        };
        if plan.literal.is_some() || plan.stage != 0 {
            return Err(invalid(span));
        }
        plan.literal = Some(bytes.to_vec());
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
        let Some(plan) = self.entries.iter_mut().find(|p| p.call == call) else {
            return Ok(());
        };
        if plan.borrowed
            || plan.stage != 1
            || argument != plan.fact.argument()
            || plan.owner != Some(owner)
            || temporaries.get(&argument).copied() != Some(owner)
        {
            return Err(invalid(span));
        }
        plan.borrowed = true;
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
                if plan.stage != 5
                    || plan.drop_authorized
                    || plan.owner != Some(slot)
                    || target != UnitDropTarget::Temporary(plan.fact.argument())
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
            if plan.literal.is_none() {
                continue;
            }
            let Some(owner) = plan.owner else {
                let (ROp::StringLiteral { bytes, .. }, [slot]) = (op, results) else {
                    return Err(invalid(span));
                };
                if plan.stage != 0 || Some(bytes) != plan.literal.as_ref() || tag.is_some() {
                    return Err(invalid(span));
                }
                plan.owner = Some(*slot);
                plan.block = Some(block);
                tag = Some(Tag {
                    plan: index,
                    role: 0,
                });
                plan.stage = 1;
                continue;
            };
            let involved = match op {
                ROp::RootPlace { owner: actual } | ROp::Drop { owner: actual } => *actual == owner,
                ROp::BorrowBegin { place, .. } => Some(*place) == plan.place,
                ROp::DirectCall { arguments, .. } => {
                    plan.loan.is_some_and(|loan| arguments.contains(&loan))
                }
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
                (1, ROp::RootPlace { .. }) if plan.borrowed => plan.place = Some(single()?),
                (
                    2,
                    ROp::BorrowBegin {
                        kind: LoanKind::Shared,
                        ..
                    },
                ) => plan.loan = Some(single()?),
                (
                    3,
                    ROp::DirectCall {
                        callee,
                        receiver,
                        arguments,
                    },
                ) if *callee == plan.callee
                    && receiver.is_none()
                    && arguments.as_slice() == [plan.loan.ok_or_else(|| invalid(span))?]
                    && results.len() == usize::from(plan.returns_value) => {}
                (4, ROp::BorrowEnd { .. }) if results.is_empty() => {}
                (5, ROp::Drop { .. }) if plan.drop_authorized && results.is_empty() => {}
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
            .any(|p| p.stage != 6 || !consumed.contains(&p.drop_index))
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
