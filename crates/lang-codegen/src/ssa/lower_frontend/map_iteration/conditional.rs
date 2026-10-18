//! Ordinary body CFG uses a plan-bound source identity and append-only exit evidence.
mod facts;
mod shape;
pub(super) mod transport;
mod verify;

use super::*;

pub(super) use facts::{qualify, source_drop};
pub(super) use shape::shape;
pub(super) use verify::validate;

#[derive(Clone, Copy)]
pub(super) struct Shape {
    pub(super) statement: StatementId,
    pub(super) body: StatementId,
    pub(super) source: SourceIdentity,
}

pub(super) struct State {
    pub(super) shape: Shape,
    pub(super) exits: Vec<Exit>,
}

impl State {
    pub(super) fn new(shape: Shape) -> Self {
        Self {
            shape,
            exits: Vec::new(),
        }
    }
}

/// These are the actual live entities immediately before this path's cleanup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Roles {
    pub(super) source: LoanId,
    pub(super) cursor: ValueId,
    pub(super) key: LoanId,
    pub(super) value: LoanId,
    pub(super) owner: Option<ValueId>,
}

pub(super) struct Exit {
    pub(super) kind: IterationExitKind,
    pub(super) block: BlockId,
    pub(super) roles: Roles,
    pub(super) cleanup_start: usize,
    pub(super) next: Option<ValueId>,
    pub(super) returned: Option<ValueId>,
}
