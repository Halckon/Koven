//! Independent branch certificate consumer; existing producers retain execution responsibility.
mod admission;
mod builder;
mod emitted;
use super::body::{BlockRef, ROp, RTerm, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::FunctionId,
};
pub(super) use emitted::Emitted;
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        BorrowSourceLoan, UnitBorrowBranchForwardTopology, UnitLoanActionId, UnitLoanForwardBranch,
    },
    source::Span,
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug)]
pub(super) enum Tag {
    Condition,
    Call(usize),
}
#[derive(Clone, Debug, Default)]
pub(super) struct Plans {
    pub(super) plan: Option<Plan>,
    pub(super) folds: Vec<(BlockRef, BlockRef)>,
}
#[derive(Clone, Debug)]
pub(super) struct Plan {
    pub(super) topology: UnitBorrowBranchForwardTopology,
    pub(super) callable: UnitItemId,
    pub(super) caller: FunctionId,
    pub(super) root: UnitSymbolId,
    pub(super) flag: UnitSymbolId,
    pub(super) split: UnitExpressionId,
    pub(super) condition: UnitExpressionId,
    pub(super) paths: Vec<PathPlan>,
    pub(super) entry: Option<BlockRef>,
    pub(super) source: Option<Slot>,
    pub(super) flag_slot: Option<Slot>,
    pub(super) condition_slot: Option<Slot>,
    pub(super) arms: Option<[BlockRef; 2]>,
    pub(super) join: Option<BlockRef>,
    pub(super) split_done: bool,
    pub(super) goto_done: bool,
}
#[derive(Clone, Debug)]
pub(super) struct PathPlan {
    pub(super) ids: Vec<UnitLoanActionId>,
    pub(super) branch: UnitLoanForwardBranch,
    pub(super) source_loan: BorrowSourceLoan<UnitExpressionId>,
    pub(super) callee: FunctionId,
    pub(super) source: Option<Slot>,
    pub(super) result: Option<Slot>,
    pub(super) block: Option<BlockRef>,
    pub(super) stage: u8,
}
fn invalid(span: Span) -> LoweringError {
    error(LoweringErrorKind::InvalidModel, span)
}
#[cfg(test)]
#[derive(Default)]
struct Observations {
    pending: Vec<emitted::Receipt>,
    published: Vec<emitted::Receipt>,
}
#[cfg(test)]
thread_local! { static RECEIPTS: std::cell::RefCell<Option<Observations>> = const {std::cell::RefCell::new(None)}; }
/// Observation publication follows the original unit verifier; no extra verifier invocation.
#[cfg(test)]
pub(in crate::ssa) struct UnitPublication {
    start: usize,
    committed: bool,
}
#[cfg(test)]
impl UnitPublication {
    pub(in crate::ssa) fn begin() -> Self {
        Self {
            start: RECEIPTS.with(|v| v.borrow().as_ref().map_or(0, |v| v.pending.len())),
            committed: false,
        }
    }
    pub(in crate::ssa) fn commit(mut self) {
        RECEIPTS.with(|v| {
            if let Some(v) = v.borrow_mut().as_mut() {
                v.published.extend(v.pending.drain(self.start..));
            }
        });
        self.committed = true;
    }
}
#[cfg(test)]
impl Drop for UnitPublication {
    fn drop(&mut self) {
        if !self.committed {
            RECEIPTS.with(|v| {
                if let Some(v) = v.borrow_mut().as_mut() {
                    v.pending.truncate(self.start);
                }
            });
        }
    }
}
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
        *v.borrow_mut() = Some(Observations::default());
    });
    let _reset = Reset;
    let result = lower();
    let receipts = RECEIPTS.with(|v| v.borrow_mut().take().unwrap().published);
    (result, receipts)
}
