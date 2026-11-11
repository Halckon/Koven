//! Shared expression algorithms; entry adapters retain identity and stateful submission.
pub(super) mod checked;

use super::model::ValueId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LoweredValue {
    Unit,
    Value(ValueId),
    Diverged,
}
