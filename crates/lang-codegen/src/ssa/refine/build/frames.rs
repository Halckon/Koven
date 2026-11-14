//! Shared control-flow frames and outcomes used by structured lowering.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum Lowered {
    Value(Slot),
    Unit,
    Diverged,
}

/// Jump targets of one loop; views above `views` belong to its iteration.
#[derive(Clone, Copy, Debug)]
pub(super) struct LoopFrame {
    pub(super) statement: UnitStatementId,
    pub(super) continue_target: BlockRef,
    pub(super) exit: BlockRef,
    pub(super) views: usize,
    pub(super) loans: usize,
    pub(super) broken: bool,
    pub(super) seq_for: Option<SeqForFrame>,
    pub(super) map_for: Option<MapForFrame>,
}

/// Active sequential-container or `RangeView` `for` provider loans and cursor slots.
#[derive(Clone, Copy, Debug)]
pub(super) struct SeqForFrame {
    pub(super) source_loan: Slot,
    pub(super) elem_loan: Slot,
    pub(super) cursor: Slot,
    pub(super) step: Slot,
    pub(super) int_ty: SsaTypeId,
    pub(super) range_source: Option<ExpressionId>,
}

/// Active `Map` / `MutableMap` `for` provider loans and cursor slots.
#[derive(Clone, Copy, Debug)]
pub(super) struct MapForFrame {
    pub(super) source_loan: Slot,
    pub(super) key_binding: UnitSymbolId,
    pub(super) value_binding: UnitSymbolId,
    pub(super) key_loan: Slot,
    pub(super) value_loan: Slot,
    pub(super) cursor: Slot,
    pub(super) cursor_ty: SsaTypeId,
}

/// A shared proof view of a MoveOnly nullable binding's inner value.
#[derive(Clone, Copy, Debug)]
pub(super) struct View {
    pub(super) symbol: UnitSymbolId,
    pub(super) slot: Slot,
    pub(super) target: BlockRef,
}
