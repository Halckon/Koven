//! Independent bounded continuation-tree receipts, published only after the original verifier.
use crate::ssa::model::{BlockId, Edge, FunctionId, InstructionId, LoanId, ValueId};
use lang_frontend::{
    ownership_checking::UnitLoanActionId,
    type_checking::{UnitExpressionId, UnitItemId},
};
#[derive(Clone, Debug)]
pub(crate) struct SplitReceipt {
    pub(crate) split: UnitExpressionId,
    pub(crate) condition: UnitExpressionId,
    pub(crate) block: BlockId,
    pub(crate) flag: LoanId,
    pub(crate) read: InstructionId,
    pub(crate) value: ValueId,
    pub(crate) edges: [Edge; 2],
}
#[derive(Clone, Debug)]
pub(crate) struct PathReceipt {
    pub(crate) control: UnitExpressionId,
    pub(crate) value: UnitExpressionId,
    pub(crate) ids: Vec<UnitLoanActionId>,
    pub(crate) callee: FunctionId,
    pub(crate) block: BlockId,
    pub(crate) instruction: InstructionId,
    pub(crate) source: LoanId,
    pub(crate) result: LoanId,
    pub(crate) edges: Vec<Edge>,
}
#[derive(Clone, Debug)]
pub(crate) struct Receipt {
    pub(super) original_gotos: Vec<(BlockRef, BlockRef)>,
    pub(super) folds: Vec<(BlockRef, BlockRef)>,
    pub(super) original_tail: BlockRef,
    pub(super) retained_hole: BlockRef,
    pub(super) final_tail: BlockId,
    pub(crate) callable: UnitItemId,
    pub(crate) caller: FunctionId,
    pub(crate) entry: BlockId,
    pub(crate) source: LoanId,
    pub(crate) splits: Vec<SplitReceipt>,
    pub(crate) paths: Vec<PathReceipt>,
}

#[cfg(test)]
#[derive(Default)]
struct Observations {
    pending: Vec<Receipt>,
    published: Vec<Receipt>,
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
    let receipts = RECEIPTS.with(|v| v.borrow_mut().take().unwrap().published);
    (result, receipts)
}

mod admission;
mod builder;
mod emitted;
mod tree;
use super::body::{BlockRef, ROp, RTerm, Slot};
use crate::ssa::lowering_support::{LoweringError, LoweringErrorKind, error};
pub(super) use emitted::Emitted;
use lang_frontend::{
    name_resolution::UnitSymbolId, ownership_checking::BorrowSourceLoan, source::Span,
};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug)]
pub(super) enum Tag {
    Condition(usize),
    Call(usize),
}
#[derive(Clone, Debug, Default)]
pub(super) struct Plans {
    pub(super) folds: Vec<(BlockRef, BlockRef)>,
    pub(super) plan: Option<Plan>,
}
#[derive(Clone, Debug)]
pub(super) struct Plan {
    pub(super) callable: UnitItemId,
    pub(super) caller: FunctionId,
    pub(super) root: UnitSymbolId,
    pub(super) flags: Vec<UnitSymbolId>,
    pub(super) splits: Vec<SplitPlan>,
    pub(super) paths: Vec<PathPlan>,
    pub(super) source: Option<Slot>,
    pub(super) cursor: usize,
    pub(super) hole_route: Vec<(usize, usize)>,
    pub(super) gotos: Vec<(BlockRef, BlockRef)>,
}
#[derive(Clone, Debug)]
pub(super) struct SplitPlan {
    pub(super) parent: Option<(usize, usize)>,
    pub(super) split: UnitExpressionId,
    pub(super) condition: UnitExpressionId,
    pub(super) entry: Option<BlockRef>,
    pub(super) arms: Option<[BlockRef; 2]>,
    pub(super) flag: Option<Slot>,
    pub(super) value: Option<Slot>,
    pub(super) done: bool,
    pub(super) join: Option<BlockRef>,
}
#[derive(Clone, Debug)]
pub(super) struct PathPlan {
    pub(super) route: Vec<(usize, usize)>,
    pub(super) control: UnitExpressionId,
    pub(super) value: UnitExpressionId,
    pub(super) ids: Vec<UnitLoanActionId>,
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

impl Receipt {
    #[cfg(test)]
    pub(crate) fn continuation_goto_count(&self) -> usize {
        self.original_gotos.len()
    }
    #[cfg(test)]
    pub(crate) fn continuation_fold_count(&self) -> usize {
        self.folds.len()
    }
    pub(crate) fn folded_continuation_chain(&self) -> bool {
        let Some(first) = self.original_gotos.first() else {
            return false;
        };
        let mut chain = vec![first.0];
        for (from, to) in &self.original_gotos {
            if chain.last() != Some(from) || chain.contains(to) {
                return false;
            }
            chain.push(*to);
        }
        if chain.first() != Some(&self.retained_hole)
            || chain.last() != Some(&self.original_tail)
            || self.folds.len() != self.original_gotos.len()
        {
            return false;
        }
        for (from, to) in &self.folds {
            let Some(i) = chain.windows(2).position(|p| p == [*from, *to]) else {
                return false;
            };
            chain.remove(i + 1);
        }
        chain == [self.retained_hole]
    }
    pub(crate) fn tail_mapping_matches(&self, block: BlockId) -> bool {
        self.folded_continuation_chain()
            && self.final_tail == block
            && self.paths.last().is_some_and(|p| p.block == block)
    }
}
