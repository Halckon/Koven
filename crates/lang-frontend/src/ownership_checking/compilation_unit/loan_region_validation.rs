//! Pure independent validation of one actual normal-return record interval.
use super::{UnitDirectCallLoanRegion, UnitLoanAction, UnitLoanActionCoverage, UnitLoanFact};
use crate::type_checking::{UnitExpressionId, UnitItemId};
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct Expected {
    pub(super) loan: UnitLoanFact,
    pub(super) origin: crate::source::Span,
}

pub(super) struct ExpectedReceiver {
    pub(super) receiver: super::UnitReceiverOwnershipFact,
    pub(super) root: crate::name_resolution::UnitSymbolId,
    pub(super) receiver_origin: crate::source::Span,
    pub(super) owner_origin: crate::source::Span,
}

pub(super) struct ExpectedNamed {
    pub(super) loan: UnitLoanFact,
    pub(super) root: crate::name_resolution::UnitSymbolId,
    pub(super) argument_origin: crate::source::Span,
    pub(super) call_origin: crate::source::Span,
    pub(super) owner_origin: crate::source::Span,
}

pub(super) struct Candidate {
    pub(super) call: UnitExpressionId,
    pub(super) callable: Option<UnitItemId>,
    pub(super) expected: Option<Vec<Expected>>,
    pub(super) protocol: u32,
    pub(super) named: Option<ExpectedNamed>,
    pub(super) receiver: Option<ExpectedReceiver>,
    pub(super) start: usize,
    pub(super) end: Option<usize>,
}
impl Candidate {
    pub(super) fn finish(
        self,
        identity: Arc<()>,
        actions: &[UnitLoanAction],
    ) -> UnitDirectCallLoanRegion {
        let records = self
            .end
            .map_or(self.start..self.start, |end| self.start..end);
        let coverage = if self.protocol == 3 {
            named_coverage(&self, actions)
        } else if self.protocol == 2 {
            receiver_coverage(&self, actions)
        } else if self.protocol == 1 {
            match (
                &self.expected,
                self.end.and_then(|end| actions.get(self.start..end)),
            ) {
                (None, _) => UnitLoanActionCoverage::Unsupported,
                (Some(expected), Some(actions))
                    if self.callable.is_some_and(|callable| {
                        callable.source_unit() == self.call.source_unit()
                    }) && validates(self.call, expected, actions) =>
                {
                    UnitLoanActionCoverage::Complete
                }
                (Some(_), _) => UnitLoanActionCoverage::Partial,
            }
        } else {
            UnitLoanActionCoverage::Unsupported
        };
        UnitDirectCallLoanRegion {
            identity,
            call: self.call,
            callable: self.callable,
            coverage,
            records,
            protocol: self.protocol,
        }
    }
}

fn named_coverage(candidate: &Candidate, actions: &[UnitLoanAction]) -> UnitLoanActionCoverage {
    use super::{UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction, UnitLoanTarget};
    use crate::ownership_checking::LoanKind;
    let Some(expected) = &candidate.named else {
        return UnitLoanActionCoverage::Unsupported;
    };
    let Some(actions) = candidate
        .end
        .and_then(|end| actions.get(candidate.start..end))
    else {
        return UnitLoanActionCoverage::Partial;
    };
    let loan = &expected.loan;
    let point = UnitDropPoint::CallReturn(candidate.call);
    let valid = candidate
        .callable
        .is_some_and(|item| item.source_unit() == candidate.call.source_unit())
        && loan.call() == candidate.call
        && loan.argument().source_unit() == candidate.call.source_unit()
        && expected.root.source_unit() == candidate.call.source_unit()
        && loan.kind() == LoanKind::Shared
        && loan.begin_span() == expected.argument_origin
        && loan.end_span() == expected.call_origin
        && matches!(loan.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == expected.root)
        && matches!(actions, [UnitLoanAction::Begin(begin), UnitLoanAction::Cleanup { point: end_point, action: UnitIterationCleanupAction::EndCallLoan(end) }, UnitLoanAction::Cleanup { point: drop_point, action: UnitIterationCleanupAction::Drop(drop) }]
            if begin == loan && end == loan && *end_point == point && *drop_point == point
                && drop.point() == point && drop.target() == UnitDropTarget::Named(expected.root)
                && drop.value_origin() == expected.owner_origin);
    if valid {
        UnitLoanActionCoverage::Complete
    } else {
        UnitLoanActionCoverage::Partial
    }
}

fn receiver_coverage(candidate: &Candidate, actions: &[UnitLoanAction]) -> UnitLoanActionCoverage {
    use super::{
        UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction, UnitReceiverOwnershipKind,
        UnitReceiverOwnershipTarget,
    };
    let Some(expected) = &candidate.receiver else {
        return UnitLoanActionCoverage::Unsupported;
    };
    let Some(actions) = candidate
        .end
        .and_then(|end| actions.get(candidate.start..end))
    else {
        return UnitLoanActionCoverage::Partial;
    };
    let receiver = &expected.receiver;
    let point = UnitDropPoint::CallReturn(candidate.call);
    let valid = candidate
        .callable
        .is_some_and(|item| item.source_unit() == candidate.call.source_unit())
        && expected.root.source_unit() == candidate.call.source_unit()
        && receiver.call() == candidate.call
        && receiver.kind() == UnitReceiverOwnershipKind::SharedLoan
        && !receiver.is_receiver_reservation()
        && receiver.activation_point().is_none()
        && receiver.begin_span() == expected.receiver_origin
        && matches!(receiver.target(), UnitReceiverOwnershipTarget::Place(place) if place.is_root() && place.root() == expected.root)
        && matches!(actions, [UnitLoanAction::ReceiverBegin(begin), UnitLoanAction::ReceiverEnd(end), UnitLoanAction::Cleanup { point: actual_point, action: UnitIterationCleanupAction::Drop(drop) }]
            if begin == receiver && end.receiver() == receiver && end.point() == point
                && *actual_point == point && drop.point() == point
                && drop.target() == UnitDropTarget::Named(expected.root) && drop.value_origin() == expected.owner_origin);
    if valid {
        UnitLoanActionCoverage::Complete
    } else {
        UnitLoanActionCoverage::Partial
    }
}

// Validate the entire unsorted interval, never a filtered selection of matching actions.
// Literal arguments have no nested evaluation or continuation: all acquisitions precede
// every retirement, followed by the exact reverse-evaluation temporary destruction.
fn validates(call: UnitExpressionId, expected: &[Expected], actions: &[UnitLoanAction]) -> bool {
    use super::{UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction, UnitLoanTarget};
    use crate::ownership_checking::LoanKind;
    let count = expected.len();
    if count.checked_mul(3) != Some(actions.len()) {
        return false;
    }
    let mut arguments = std::collections::BTreeSet::new();
    for (index, expected) in expected.iter().enumerate() {
        let loan = &expected.loan;
        if loan.begin_span() != expected.origin
            || loan.call() != call
            || loan.kind() != LoanKind::Shared
            || loan.argument().source_unit() != call.source_unit()
            || loan.target() != &UnitLoanTarget::Temporary(loan.argument())
            || !arguments.insert(loan.argument())
            || !matches!(&actions[index], UnitLoanAction::Begin(actual) if actual == loan)
        {
            return false;
        }
    }
    for (index, expected) in expected.iter().rev().enumerate() {
        let loan = &expected.loan;
        if !matches!(&actions[count + index], UnitLoanAction::Cleanup {
            point, action: UnitIterationCleanupAction::EndCallLoan(actual)
        } if *point == UnitDropPoint::CallReturn(call) && actual == loan)
        {
            return false;
        }
        if !matches!(&actions[2 * count + index], UnitLoanAction::Cleanup {
            point, action: UnitIterationCleanupAction::Drop(drop)
        } if *point == UnitDropPoint::CallReturn(call)
            && drop.point() == *point
            && drop.target() == UnitDropTarget::Temporary(loan.argument())
            && drop.value_origin() == expected.origin)
        {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests;
