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
    pub(crate) callable: UnitItemId,
    pub(crate) caller: FunctionId,
    pub(crate) entry: BlockId,
    pub(crate) source: LoanId,
    pub(crate) splits: Vec<SplitReceipt>,
    pub(crate) paths: Vec<PathReceipt>,
}

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
        match (self.topology, self.flag_layout) {
            (
                UnitBorrowNestedForwardTopology::BalancedBoth,
                UnitBorrowNestedForwardFlagLayout::SharedInner,
            ) => Some(vec![0, 1, 1]),
            (_, UnitBorrowNestedForwardFlagLayout::Distinct) => {
                Some((0..self.counts().0).collect())
            }
            _ => None,
        }
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
        match self.topology {
            UnitBorrowNestedForwardTopology::BalancedBoth => (3, 4),
            UnitBorrowNestedForwardTopology::NestedThen
            | UnitBorrowNestedForwardTopology::NestedElse => (2, 3),
        }
    }
    fn parent_arm(&self, split: usize) -> Option<usize> {
        match (self.topology, split) {
            (UnitBorrowNestedForwardTopology::BalancedBoth, 1) => Some(0),
            (UnitBorrowNestedForwardTopology::BalancedBoth, 2) => Some(1),
            (_, 1) => self.nested_arm(),
            _ => None,
        }
    }
    fn nested_arm(&self) -> Option<usize> {
        match self.topology {
            UnitBorrowNestedForwardTopology::NestedThen => Some(0),
            UnitBorrowNestedForwardTopology::NestedElse => Some(1),
            UnitBorrowNestedForwardTopology::BalancedBoth => None,
        }
    }
    fn leaf_route(&self, index: usize) -> Vec<(usize, usize)> {
        match (self.topology, index) {
            (UnitBorrowNestedForwardTopology::NestedThen, 0) => vec![(0, 0), (1, 0)],
            (UnitBorrowNestedForwardTopology::NestedThen, 1) => vec![(0, 0), (1, 1)],
            (UnitBorrowNestedForwardTopology::NestedThen, 2) => vec![(0, 1)],
            (UnitBorrowNestedForwardTopology::NestedElse, 0) => vec![(0, 0)],
            (UnitBorrowNestedForwardTopology::NestedElse, 1) => vec![(0, 1), (1, 0)],
            (UnitBorrowNestedForwardTopology::NestedElse, 2) => vec![(0, 1), (1, 1)],
            (UnitBorrowNestedForwardTopology::BalancedBoth, 0) => vec![(0, 0), (1, 0)],
            (UnitBorrowNestedForwardTopology::BalancedBoth, 1) => vec![(0, 0), (1, 1)],
            (UnitBorrowNestedForwardTopology::BalancedBoth, 2) => vec![(0, 1), (2, 0)],
            (UnitBorrowNestedForwardTopology::BalancedBoth, 3) => vec![(0, 1), (2, 1)],
            _ => Vec::new(),
        }
    }
}
