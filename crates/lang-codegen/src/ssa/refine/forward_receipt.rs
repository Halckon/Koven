//! Local live-delivery verification; the existing call/return remain the only producers.
mod admission;
mod emitted;
use super::body::{BlockRef, ROp, RTerm, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::FunctionId,
};
pub(super) use emitted::Emitted;
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{BorrowSourceLoan, UnitLoanActionId},
    source::Span,
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub(super) struct Tag;
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
    source_loan: BorrowSourceLoan<UnitExpressionId>,
    root: UnitSymbolId,
    callee: FunctionId,
    source: Option<Slot>,
    result: Option<Slot>,
    block: Option<BlockRef>,
    returned: bool,
    stage: u8,
}
impl Plans {
    pub(super) fn enter(
        &mut self,
        control: UnitExpressionId,
        value: UnitExpressionId,
        borrowed: &BTreeMap<UnitSymbolId, Slot>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(plan) = self.entries.first_mut() else {
            return Ok(());
        };
        if plan.stage != 0 || control != plan.call || value != plan.call {
            return Err(invalid(span));
        }
        plan.source = Some(*borrowed.get(&plan.root).ok_or_else(|| invalid(span))?);
        plan.stage = 1;
        Ok(())
    }
    pub(super) fn source_call(
        &self,
        actual: UnitExpressionId,
        source: BorrowSourceLoan<UnitExpressionId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(plan) = self.entries.first() else {
            return Ok(());
        };
        if plan.stage != 1 || actual != plan.call || source != plan.source_loan {
            return Err(invalid(span));
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
        let Some(plan) = self.entries.first_mut() else {
            return Ok(None);
        };
        let ROp::BorrowCall {
            callee,
            arguments,
            source,
        } = op
        else {
            return Err(invalid(span));
        };
        let [result] = results else {
            return Err(invalid(span));
        };
        if plan.stage != 1
            || block != BlockRef(0)
            || *callee != plan.callee
            || Some(*source) != plan.source
            || arguments != &[*source]
            || source == result
        {
            return Err(invalid(span));
        }
        plan.result = Some(*result);
        plan.block = Some(block);
        plan.stage = 2;
        Ok(Some(Tag))
    }
    pub(super) fn terminate(
        &mut self,
        term: &RTerm,
        block: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(plan) = self.entries.first_mut() else {
            return Ok(());
        };
        let RTerm::BorrowReturn { loan } = term else {
            return Err(invalid(span));
        };
        if plan.stage != 2
            || plan.returned
            || Some(block) != plan.block
            || Some(*loan) != plan.result
        {
            return Err(invalid(span));
        }
        plan.returned = true;
        plan.stage = 3;
        Ok(())
    }
    pub(super) fn finish(&self, span: Span) -> Result<(), LoweringError> {
        if self.entries.iter().any(|p| p.stage != 3 || !p.returned) {
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
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            RECEIPTS.with(|v| {
                v.borrow_mut().take();
            });
        }
    }
    RECEIPTS.with(|v| {
        assert!(v.borrow().is_none());
        *v.borrow_mut() = Some(Vec::new());
    });
    let _reset = Reset;
    let result = lower();
    let receipts = RECEIPTS.with(|v| v.borrow_mut().take().unwrap());
    (result, receipts)
}
