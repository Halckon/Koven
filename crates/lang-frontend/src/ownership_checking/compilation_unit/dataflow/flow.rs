//! Body-local ownership 控制流状态与保守合并。

use std::collections::BTreeMap;

use crate::{
    name_resolution::UnitSymbolId,
    source::Span,
    type_checking::{UnitExpressionId, UnitStatementId},
};

use super::{AccessKind, LoanKind, UnitOwnershipPlace};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ActiveLoan {
    pub(super) owner: ActiveLoanOwner,
    pub(super) target: ActiveLoanTarget,
    pub(super) kind: LoanKind,
    pub(super) reserved: bool,
    pub(super) origin: Span,
}

impl ActiveLoan {
    pub(super) fn conflicts_with(&self, access: AccessKind) -> bool {
        !((self.kind == LoanKind::Shared || self.reserved)
            && matches!(access, AccessKind::Read | AccessKind::SharedLoan))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActiveLoanOwner {
    Call(UnitExpressionId),
    Closure(UnitExpressionId),
    IterationSource(UnitStatementId),
    IterationElement(UnitStatementId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ActiveLoanTarget {
    Place(UnitOwnershipPlace),
    This,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct State {
    pub(super) origins: crate::ownership_checking::callable_provenance::graph::OriginState<
        UnitSymbolId,
        crate::ownership_checking::UnitCallableOrigin,
    >,
    pub(super) moved: BTreeMap<UnitSymbolId, Span>,
    pub(super) this_moved: Option<Span>,
    pub(super) loans: Vec<ActiveLoan>,
    pub(super) closures: BTreeMap<UnitSymbolId, UnitExpressionId>,
    pub(super) non_owning: BTreeMap<UnitSymbolId, Span>,
    pub(super) immutable_captures: BTreeMap<UnitSymbolId, Span>,
}

#[derive(Default)]
pub(super) struct Flows {
    pub(super) next: Option<State>,
    pub(super) breaks: Option<State>,
    pub(super) continues: Option<State>,
}

impl Flows {
    pub(super) fn next(state: State) -> Self {
        Self {
            next: Some(state),
            ..Self::default()
        }
    }

    pub(super) fn merge(&mut self, other: Self) {
        merge_optional_state(&mut self.next, other.next);
        merge_optional_state(&mut self.breaks, other.breaks);
        merge_optional_state(&mut self.continues, other.continues);
    }
}

pub(super) fn merge_optional_state(target: &mut Option<State>, source: Option<State>) {
    let Some(source) = source else {
        return;
    };
    if let Some(target) = target {
        merge_state(target, source);
    } else {
        *target = Some(source);
    }
}

pub(super) fn merge_state(target: &mut State, source: State) {
    target.origins.merge(&source.origins);
    // A loan live on any reachable incoming edge must still constrain later access.
    // In particular, a branch-selected closure may retain a shared loan until activation.
    for loan in source.loans {
        if !target.loans.contains(&loan) {
            target.loans.push(loan);
        }
    }
    target
        .closures
        .retain(|symbol, closure| source.closures.get(symbol) == Some(closure));
    target
        .non_owning
        .retain(|symbol, origin| source.non_owning.get(symbol) == Some(origin));
    target
        .immutable_captures
        .retain(|symbol, origin| source.immutable_captures.get(symbol) == Some(origin));
    for (symbol, origin) in source.moved {
        target
            .moved
            .entry(symbol)
            .and_modify(|current| {
                if origin.start() < current.start() {
                    *current = origin;
                }
            })
            .or_insert(origin);
    }
    target.this_moved = match (target.this_moved, source.this_moved) {
        (Some(left), Some(right)) => Some(if left.start() <= right.start() {
            left
        } else {
            right
        }),
        (Some(origin), None) | (None, Some(origin)) => Some(origin),
        (None, None) => None,
    };
}
