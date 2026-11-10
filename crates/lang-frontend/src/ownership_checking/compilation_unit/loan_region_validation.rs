//! Pure independent validation of one actual normal-return record interval.
use super::{UnitDirectCallLoanRegion, UnitLoanAction, UnitLoanActionCoverage, UnitLoanFact};
use crate::type_checking::{UnitExpressionId, UnitItemId};
use std::sync::Arc;

pub(super) struct Candidate {
    pub(super) call: UnitExpressionId,
    pub(super) callable: Option<UnitItemId>,
    pub(super) expected: Option<Vec<UnitLoanFact>>,
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
            // The independent closure proof is deliberately not installed in the capability red test.
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
