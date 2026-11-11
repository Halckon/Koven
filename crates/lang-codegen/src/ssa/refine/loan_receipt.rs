//! Optional protocol-4 verifier. Existing producers remain the only effect emitters.
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
    ownership_checking::{BorrowSourceLoan, UnitDropPoint, UnitDropTarget, UnitLoanActionId},
    source::Span,
    type_checking::{UnitExpressionId, UnitItemId},
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
