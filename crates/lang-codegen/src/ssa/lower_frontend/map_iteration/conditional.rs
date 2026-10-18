//! Three no-else exits use ordinary If transport, with a separate path-local Map frame.
mod facts;
mod shape;
pub(super) mod transport;
mod verify;

use super::*;
use lang_frontend::ownership_checking::{CleanupOwnerValueId, DropFact};

pub(super) use facts::qualify;
pub(super) use shape::shape;
pub(super) use verify::validate;

#[derive(Clone, Copy)]
pub(super) struct Arm {
    pub(super) control: ExpressionId,
    pub(super) kind: IterationExitKind,
    pub(super) operand_span: Option<Span>,
    pub(super) span: Span,
}

#[derive(Clone, Copy)]
pub(super) struct Local {
    pub(super) symbol: SymbolId,
    pub(super) initializer: ExpressionId,
    pub(super) initializer_span: Span,
    pub(super) origin: Span,
}

#[derive(Clone, Copy)]
pub(super) struct Shape {
    pub(super) arms: [Arm; 3],
    pub(super) local: Local,
    pub(super) body: StatementId,
}

impl Shape {
    pub(super) fn kinds(self) -> [IterationExitKind; 5] {
        [
            self.arms[0].kind,
            self.arms[1].kind,
            self.arms[2].kind,
            IterationExitKind::Fallthrough,
            IterationExitKind::Exhaustion,
        ]
    }
}

pub(super) struct State {
    pub(super) shape: Shape,
    pub(super) transfers: Vec<Transfer>,
    pub(super) exits: Vec<Exit>,
}

impl State {
    pub(super) fn new(shape: Shape) -> Self {
        Self {
            shape,
            transfers: Vec::new(),
            exits: Vec::new(),
        }
    }
}

/// Only this data is restored for a sibling. Evidence in State is append-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Roles {
    pub(super) source: LoanId,
    pub(super) cursor: ValueId,
    pub(super) key: LoanId,
    pub(super) value: LoanId,
    pub(super) owner: Option<ValueId>,
    pub(super) local: ValueId,
}

pub(super) struct Transfer {
    pub(super) control: ExpressionId,
    pub(super) branch: usize,
    pub(super) from: BlockId,
    pub(super) to: BlockId,
    pub(super) before: Roles,
    pub(super) after: Roles,
}

pub(super) struct Exit {
    pub(super) kind: IterationExitKind,
    pub(super) block: BlockId,
    pub(super) roles: Roles,
    pub(super) cleanup_start: usize,
    pub(super) next: Option<ValueId>,
    pub(super) returned: Option<ValueId>,
}

pub(super) fn guard_drop(
    owned: &OwnershipCheckedFile,
    shape: Shape,
    point: DropPoint,
    owner: CleanupOwnerValueId,
    span: Span,
) -> Result<DropFact, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let fact = unique(owned.drops().iter().copied().filter(|fact| {
        fact.target() == lang_frontend::ownership_checking::DropTarget::Named(shape.local.symbol)
            && fact.point() == point
    }))
    .ok_or_else(missing)?;
    if fact.owner() != Some(owner)
        || fact.value_origin() != shape.local.origin
        || fact.capture_slot().is_some()
        || fact.instance_address().is_some()
    {
        return Err(missing());
    }
    if fact.condition().is_some() {
        return Err(error(LoweringErrorKind::UnsupportedNode, span));
    }
    Ok(fact)
}
