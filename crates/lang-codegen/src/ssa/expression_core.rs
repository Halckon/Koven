//! Shared expression algorithms; entry adapters retain identity and stateful submission.
pub(in crate::ssa) mod checked;

use super::model::ValueId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ssa) enum LoweredValue {
    Unit,
    Value(ValueId),
    Diverged,
}
