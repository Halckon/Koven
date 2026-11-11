//! Independent fixed three-leaf receipt observation, never inferred from final CFG alone.
use crate::ssa::model::{BlockId, Edge, FunctionId, InstructionId, LoanId, ValueId};
use lang_frontend::{
    ownership_checking::{
        UnitBorrowNestedForwardFlagLayout, UnitBorrowNestedForwardTopology, UnitLoanActionId,
    },
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
    pub(crate) topology: UnitBorrowNestedForwardTopology,
    pub(crate) flag_layout: UnitBorrowNestedForwardFlagLayout,
    pub(super) fold: Option<(BlockRef, BlockRef)>,
    pub(super) cascade_folds: Vec<(BlockRef, BlockRef)>,
    pub(crate) callable: UnitItemId,
    pub(crate) caller: FunctionId,
    pub(crate) entry: BlockId,
    pub(crate) source: LoanId,
    pub(crate) splits: Vec<SplitReceipt>,
    pub(crate) paths: Vec<PathReceipt>,
}

mod route_contract;
use route_contract::RouteContract;
mod admission;
mod builder;
mod emitted;
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
    pub(super) plan: Option<Plan>,
    pub(super) folds: Vec<(BlockRef, BlockRef)>,
}
#[derive(Clone, Debug)]
pub(super) struct Plan {
    pub(super) topology: UnitBorrowNestedForwardTopology,
    pub(super) flag_layout: UnitBorrowNestedForwardFlagLayout,
    pub(super) entry_flags: Vec<UnitSymbolId>,
    pub(super) callable: UnitItemId,
    pub(super) caller: FunctionId,
    pub(super) root: UnitSymbolId,
    pub(super) flags: Vec<UnitSymbolId>,
    pub(super) splits: Vec<SplitPlan>,
    pub(super) paths: Vec<PathPlan>,
    pub(super) source: Option<Slot>,
    pub(super) goto_done: bool,
    pub(super) cascade_gotos: [bool; 2],
}
#[derive(Clone, Debug)]
pub(super) struct SplitPlan {
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

impl Plan {
    fn flag_indices(&self) -> Option<Vec<usize>> {
        RouteContract::new(self.topology).flag_indices(self.flag_layout)
    }
    fn cascade(&self) -> bool {
        self.topology == UnitBorrowNestedForwardTopology::NestedElseCascadeTail
    }
    fn cascade_fold_pairs(&self) -> Option<[(BlockRef, BlockRef); 2]> {
        Some([
            (self.splits.get(1)?.arms?[1], self.splits[1].join?),
            (self.splits[1].arms?[1], self.splits.first()?.join?),
        ])
    }
    fn tail(&self) -> bool {
        self.topology == UnitBorrowNestedForwardTopology::NestedThenTail
    }
    fn valid_shape(&self) -> bool {
        let (splits, leaves) = self.counts();
        let Some(indices) = self.flag_indices() else {
            return false;
        };
        let count = match self.flag_layout {
            UnitBorrowNestedForwardFlagLayout::Distinct => splits,
            UnitBorrowNestedForwardFlagLayout::SharedInner => 2,
        };
        self.splits.len() == splits
            && self.paths.len() == leaves
            && self.flags.len() == splits
            && self.entry_flags.len() == count
            && !self.entry_flags.contains(&self.root)
            && self
                .entry_flags
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == count
            && indices
                .iter()
                .enumerate()
                .all(|(i, index)| self.entry_flags.get(*index) == self.flags.get(i))
    }
    fn counts(&self) -> (usize, usize) {
        RouteContract::new(self.topology).counts()
    }
    fn parent_arm(&self, split: usize) -> Option<usize> {
        RouteContract::new(self.topology).parent_arm(split)
    }
    fn leaf_route(&self, index: usize) -> Vec<(usize, usize)> {
        RouteContract::new(self.topology)
            .route(index)
            .unwrap_or_default()
            .to_vec()
    }
}

impl Receipt {
    pub(crate) fn folded_distinct_blocks(&self) -> bool {
        self.fold.is_some_and(|(from, to)| from != to)
    }
}

impl Receipt {
    pub(crate) fn cascade_folded_distinct_blocks(&self) -> bool {
        matches!(self.cascade_folds.as_slice(), [(from,inner),(same,outer)] if from==same && from!=inner && from!=outer && inner!=outer)
    }
}
