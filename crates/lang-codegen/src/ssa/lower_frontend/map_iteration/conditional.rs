//! Ordinary body CFG uses a plan-bound source identity and append-only exit evidence.
mod actions;
mod facts;
pub(super) mod local;
mod retained;
mod shape;
mod successor;
pub(super) mod transport;
mod verify;

use super::*;

pub(super) use actions::validate_drop_ledger;
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

#[derive(Clone, Copy)]
pub(super) struct CleanupFrame {
    pub(super) statement: StatementId,
    pub(super) roles: Roles,
    pub(super) owner: Option<(source_owner::Proof, SourceIdentity, ValueId)>,
}

pub(super) struct Exit {
    pub(super) kind: IterationExitKind,
    pub(super) block: BlockId,
    pub(super) roles: Roles,
    pub(super) frames: Vec<CleanupFrame>,
    pub(super) cleanup_start: usize,
    pub(super) cleanup_end: usize,
    pub(super) next: Option<ValueId>,
    pub(super) returned: Option<ValueId>,
}

/// Conditional local drops are admitted only when the complete source-qualified
/// plan has proved their original expression owner and ordering.
pub(super) fn supported_local_drop(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    fact: lang_frontend::ownership_checking::DropFact,
) -> bool {
    typed.map_iterations().iter().any(|descriptor| {
        super::qualify::qualify(parsed, names, typed, owned, *descriptor)
            .ok()
            .is_some_and(|qualified| {
                qualified.conditional.is_some()
                    && qualified
                        .plan
                        .exits()
                        .iter()
                        .any(|exit| exit.actions().contains(&IterationCleanupAction::Drop(fact)))
            })
    })
}
