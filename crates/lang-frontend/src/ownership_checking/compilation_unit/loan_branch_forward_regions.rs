//! Branch coverage API shell: no producer or certificate is implemented yet.
use super::UnitLoanActionCoverage;

/// Independently scoped branch coverage, never a linear sequence across both arms.
/// The initial API shell publishes no instances. Source/callable coverage and backend
/// receipt eligibility are unaffected by this separate product.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBorrowBranchForwardRegion {
    coverage: UnitLoanActionCoverage,
}

impl UnitBorrowBranchForwardRegion {
    /// Coverage of the explicitly declared region, not the whole source or callable.
    #[must_use]
    pub const fn coverage(&self) -> UnitLoanActionCoverage {
        self.coverage
    }
}
