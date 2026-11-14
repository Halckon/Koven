//! Actual lowering evidence for a partial frontend loop lineage, never a completeness claim.
use super::body::{BlockRef, ROp, RTerm, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::FunctionId,
};
use lang_frontend::{
    name_resolution::UnitSymbolId, ownership_checking::UnitLoanLoopInvariantLineage, source::Span,
    type_checking::UnitExpressionId,
};
use std::collections::{BTreeMap, BTreeSet};
mod admission;
mod builder;
mod emitted;
mod proof;
mod roles;
mod validation;
mod whole_function;
pub(super) use emitted::Emitted;
pub(crate) use emitted::Receipt;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Tag {
    Owner,
    Place,
    Source,
    Result,
    Condition,
    Consumer(usize),
    ResultEnd,
    SourceEnd,
    OwnerDrop,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct LoopBlocks {
    pub(super) entry: BlockRef,
    pub(super) header: BlockRef,
    pub(super) body: BlockRef,
    pub(super) finished: BlockRef,
    pub(super) exit: BlockRef,
}
/// Actual entry maps captured together before the first loop edge.
pub(super) struct EntryLease<'a> {
    pub(super) owner: Option<Slot>,
    pub(super) sources: &'a [Slot],
    pub(super) result: Option<Slot>,
    pub(super) flag: Slot,
}
#[derive(Clone, Debug)]
pub(super) struct Plan {
    pub(super) caller: FunctionId,
    pub(super) binding: UnitSymbolId,
    pub(super) root: UnitSymbolId,
    pub(super) lineage: UnitLoanLoopInvariantLineage,
    pub(super) callees: Vec<FunctionId>,
    pub(super) source_callee: FunctionId,
    pub(super) condition: UnitExpressionId,
    pub(super) owner_span: Span,
    pub(super) owner_binding_span: Span,
    pub(super) literal: Option<Slot>,
    pub(super) owner_alias: Option<(Slot, Slot)>,
    pub(super) condition_span: Span,
    pub(super) flag_parameter: usize,
    pub(super) owner: Option<Slot>,
    pub(super) place: Option<Slot>,
    pub(super) source: Option<Slot>,
    pub(super) result: Option<Slot>,
    pub(super) flag: Option<Slot>,
    pub(super) condition_value: Option<Slot>,
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
