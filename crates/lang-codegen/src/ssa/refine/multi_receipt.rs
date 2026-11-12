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
    pub(super) blocks: BTreeMap<BlockRef, BlockId>,
    pub(crate) continuation_edges: Vec<(BlockId, Edge)>,
    pub(crate) origins: Vec<Vec<Edge>>,
    pub(super) original_gotos: Vec<(BlockRef, BlockRef)>,
    pub(super) folds: Vec<(BlockRef, BlockRef)>,
    pub(super) original_tail: BlockRef,
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
    pub(super) hole_routes: Vec<Vec<(usize, usize)>>,
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
    pub(crate) fn continuation_goto_count(&self) -> usize {
        self.original_gotos.len()
    }
    pub(crate) fn continuation_fold_count(&self) -> usize {
        self.folds.len()
    }
    pub(crate) fn tail_mapping_matches(&self, block: BlockId) -> bool {
        let mut original = self.original_tail;
        for (from, to) in &self.folds {
            if original == *to {
                original = *from;
            }
        }
        self.blocks.get(&original) == Some(&block)
            && self.final_tail == block
            && self.paths.last().is_some_and(|p| p.block == block)
    }
}

#[cfg(test)]
impl Receipt {
    /// Observe a real fold whose surviving predecessor was already a multi-source join.
    pub(crate) fn folded_multi_predecessor_successor(&self) -> bool {
        let mut edges = self.original_gotos.clone();
        let mut observed = false;
        for (from, to) in &self.folds {
            let Some(index) = edges.iter().position(|edge| edge == &(*from, *to)) else {
                return false;
            };
            if edges.iter().filter(|(_, target)| target == to).count() != 1 {
                return false;
            }
            observed |= edges.iter().filter(|(_, target)| target == from).count() >= 2;
            edges.remove(index);
            for (source, target) in &mut edges {
                if source == to {
                    *source = *from;
                }
                if target == to {
                    *target = *from;
                }
            }
        }
        observed
    }
}
