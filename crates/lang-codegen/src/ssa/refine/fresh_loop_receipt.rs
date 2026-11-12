//! Actual producer evidence for a sealed core no-carry fact, separate from carried loops.
use super::body::{BlockRef, ROp, RTerm, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{FunctionId, SsaTypeId},
};
use lang_frontend::{
    ownership_checking::UnitLoopNoCarryFact, source::Span, type_checking::UnitExpressionId,
};
use std::collections::{BTreeMap, BTreeSet};
mod admission;
mod builder;
#[cfg(test)]
mod ci_tests;
mod emitted;
mod validation;
mod whole_function;
pub(super) use emitted::Emitted;
pub(crate) use emitted::Receipt;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Tag {
    Condition,
    Result,
    Consumer(usize),
    ResultEnd,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct LoopBlocks {
    pub(super) entry: BlockRef,
    pub(super) header: BlockRef,
    pub(super) body: BlockRef,
    pub(super) finished: BlockRef,
    pub(super) exit: BlockRef,
}
#[derive(Clone, Debug)]
pub(super) struct Plan {
    pub(super) caller: FunctionId,
    pub(super) fact: UnitLoopNoCarryFact,
    pub(super) source_callee: FunctionId,
    pub(super) callees: Vec<FunctionId>,
    pub(super) condition: UnitExpressionId,
    pub(super) condition_span: Span,
    pub(super) source_span: Span,
    pub(super) consumer_spans: Vec<Span>,
    pub(super) end_span: Span,
    pub(super) source_parameter: usize,
    pub(super) flag_parameter: usize,
    pub(super) source_type: SsaTypeId,
    pub(super) flag_type: SsaTypeId,
    pub(super) source: Option<Slot>,
    pub(super) flag: Option<Slot>,
    pub(super) condition_value: Option<Slot>,
    pub(super) result: Option<Slot>,
    pub(super) blocks: Option<LoopBlocks>,
    pub(super) events: Vec<Tag>,
    pub(super) terms: Vec<(BlockRef, RTerm)>,
    pub(super) folds: Vec<(BlockRef, BlockRef)>,
    pub(super) boundary: usize,
}
#[derive(Clone, Debug, Default)]
pub(super) struct Plans {
    pub(super) plan: Option<Plan>,
}
fn invalid(span: Span) -> LoweringError {
    error(LoweringErrorKind::InvalidModel, span)
}
#[cfg(test)]
#[derive(Default)]
struct Observations {
    pending: Vec<Receipt>,
    published: Vec<Receipt>,
}
#[cfg(test)]
thread_local! {static RECEIPTS:std::cell::RefCell<Option<Observations>>=const {std::cell::RefCell::new(None)};}
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
pub(crate) fn observe<T>(lower: impl FnOnce() -> T) -> (T, Vec<Receipt>) {
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
    let values = RECEIPTS.with(|v| v.borrow_mut().take().unwrap().published);
    (result, values)
}
pub(super) fn record(receipt: Receipt) {
    #[cfg(test)]
    RECEIPTS.with(|v| {
        if let Some(v) = v.borrow_mut().as_mut() {
            v.pending.push(receipt);
        }
    });
    #[cfg(not(test))]
    let _ = receipt;
}
