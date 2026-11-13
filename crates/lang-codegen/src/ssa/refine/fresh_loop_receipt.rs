//! Actual producer evidence for a sealed core no-carry fact, separate from carried loops.
use super::body::{BlockRef, ROp, RTerm, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{FunctionId, SsaTypeId},
};
use lang_frontend::{
    ownership_checking::UnitLoopNoCarryFact,
    source::Span,
    type_checking::{ParameterMode, UnitExpressionId, UnitStatementId},
};
use std::collections::{BTreeMap, BTreeSet};
mod admission;
pub(super) use admission::callee;
mod builder;
#[cfg(test)]
mod ci_tests;
mod emitted;
#[cfg(test)]
mod publication_ci;
mod source_admission;
mod source_segments;
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
    SourceConsumer(usize),
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
    pub(super) flag_owned: bool,
    pub(super) condition_used: bool,
    pub(super) source_type: SsaTypeId,
    pub(super) flag_type: SsaTypeId,
    pub(super) source: Option<Slot>,
    pub(super) flag: Option<Slot>,
    pub(super) condition_value: Option<Slot>,
    pub(super) result: Option<Slot>,
    pub(super) blocks: Option<LoopBlocks>,
    pub(super) events: Vec<Tag>,
    pub(super) terms: Vec<(BlockRef, RTerm)>,
    pub(super) boundary: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SourceConsumer {
    pub(super) statement: UnitStatementId,
    pub(super) call: UnitExpressionId,
    pub(super) argument: UnitExpressionId,
    pub(super) callee: FunctionId,
    pub(super) span: Span,
    pub(super) argument_span: Span,
}
#[derive(Clone, Debug, Default)]
pub(super) struct SourceSegment {
    pub(super) consumers: Vec<SourceConsumer>,
    pub(super) block: Option<BlockRef>,
    pub(super) completed: usize,
    pub(super) pending: bool,
    pub(super) appended: bool,
}
#[derive(Clone, Debug, Default)]
pub(super) struct Plans {
    pub(super) abi_types: Vec<SsaTypeId>,
    pub(super) abi_modes: Vec<ParameterMode>,
    pub(super) abi_slots: Option<Vec<Slot>>,
    pub(super) loops: Vec<Plan>,
    pub(super) segments: Vec<SourceSegment>,
    pub(super) active: Option<usize>,
    pub(super) folds: Vec<(BlockRef, BlockRef)>,
}
fn invalid(span: Span) -> LoweringError {
    error(LoweringErrorKind::InvalidModel, span)
}
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PublicationStage {
    Recorded,
    BeforeVerifier,
    Committed,
    RolledBack,
}
#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct PublicationEvent {
    pub(crate) stage: PublicationStage,
    pub(crate) pending: Vec<FunctionId>,
    pub(crate) published: Vec<FunctionId>,
}
#[cfg(test)]
impl Observations {
    fn trace(&mut self, stage: PublicationStage) {
        self.trace.push(PublicationEvent {
            stage,
            pending: self.pending.iter().map(|r| r.caller).collect(),
            published: self.published.iter().map(|r| r.caller).collect(),
        });
    }
}
#[cfg(test)]
pub(in crate::ssa) fn before_final_verifier(program: &mut crate::ssa::model::Program) {
    RECEIPTS.with(|v| {
        if let Some(v) = v.borrow_mut().as_mut() {
            v.trace(PublicationStage::BeforeVerifier);
        }
    });
    publication_ci::before_verifier(program);
}
#[cfg(test)]
#[derive(Default)]
struct Observations {
    trace: Vec<PublicationEvent>,
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
                v.trace(PublicationStage::Committed);
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
                    v.trace(PublicationStage::RolledBack);
                }
            });
        }
    }
}
#[cfg(test)]
pub(crate) fn observe<T>(lower: impl FnOnce() -> T) -> (T, Vec<Receipt>) {
    let (result, receipts, _) = observe_publication(lower);
    (result, receipts)
}
#[cfg(test)]
pub(crate) fn observe_publication<T>(
    lower: impl FnOnce() -> T,
) -> (T, Vec<Receipt>, Vec<PublicationEvent>) {
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
    let values = RECEIPTS.with(|v| v.borrow_mut().take().unwrap());
    (result, values.published, values.trace)
}
pub(super) fn record(receipt: Receipt) {
    #[cfg(test)]
    RECEIPTS.with(|v| {
        if let Some(v) = v.borrow_mut().as_mut() {
            v.pending.push(receipt);
            v.trace(PublicationStage::Recorded);
        }
    });
    #[cfg(not(test))]
    let _ = receipt;
}
