//! Pure independent validation of one actual normal-return record interval.
use super::{UnitDirectCallLoanRegion, UnitLoanAction, UnitLoanActionCoverage, UnitLoanFact};
use crate::type_checking::{UnitExpressionId, UnitItemId};
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct Expected {
    pub(super) loan: UnitLoanFact,
    pub(super) origin: crate::source::Span,
}

pub(super) struct Candidate {
    pub(super) call: UnitExpressionId,
    pub(super) callable: Option<UnitItemId>,
    pub(super) expected: Option<Vec<Expected>>,
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
        let coverage = match (
            &self.expected,
            self.end.and_then(|end| actions.get(self.start..end)),
        ) {
            (None, _) => UnitLoanActionCoverage::Unsupported,
            (Some(expected), Some(actions))
                if self
                    .callable
                    .is_some_and(|callable| callable.source_unit() == self.call.source_unit())
                    && validates(self.call, expected, actions) =>
            {
                UnitLoanActionCoverage::Complete
            }
            (Some(_), _) => UnitLoanActionCoverage::Partial,
        };
        UnitDirectCallLoanRegion {
            identity,
            call: self.call,
            callable: self.callable,
            coverage,
            records,
        }
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
