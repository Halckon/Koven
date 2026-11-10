//! Bounded caller-side normal-return certificates, not whole-callable lifecycle claims.
use super::{UnitLoanActionCoverage, UnitLoanActionId};
use crate::type_checking::{UnitExpressionId, UnitItemId};
use std::{ops::Range, sync::Arc};

/// Fixed action families certified by the ordinary literal-argument region protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitLoanRegionFamily {
    /// Shared caller argument acquisition and retirement at normal call return.
    DirectSharedArgument,
    /// Destruction of the corresponding independently owned String temporary.
    ArgumentTemporaryDrop,
}

/// One actually entered caller-side normal-return region, protocol version 1.
/// Complete covers only the listed local families: it does not prove the callee returns,
/// callee-internal/Abort paths, outer loans, receivers, result handoffs or the callable.
/// Admission requires independent calls in a flat ordinary function, with Int/Unit results
/// and only plain String literal Borrow arguments (including zero arguments).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitDirectCallLoanRegion {
    pub(super) identity: Arc<()>,
    pub(super) call: UnitExpressionId,
    pub(super) callable: Option<UnitItemId>,
    pub(super) coverage: UnitLoanActionCoverage,
    pub(super) records: Range<usize>,
}
impl UnitDirectCallLoanRegion {
    /// Actual entered call, qualified by the observation source.
    #[must_use]
    pub const fn call(&self) -> UnitExpressionId {
        self.call
    }
    /// Eligible ordinary function, when the flat callable shape is established.
    #[must_use]
    pub const fn callable(&self) -> Option<UnitItemId> {
        self.callable
    }
    /// Fixed normal-return protocol version, independent of source-wide coverage.
    #[must_use]
    pub const fn protocol_version(&self) -> u32 {
        1
    }
    /// Exact local families covered when coverage is Complete.
    #[must_use]
    pub const fn families(&self) -> &'static [UnitLoanRegionFamily] {
        &[
            UnitLoanRegionFamily::DirectSharedArgument,
            UnitLoanRegionFamily::ArgumentTemporaryDrop,
        ]
    }
    /// Independent action validation result for the declared normal edge.
    #[must_use]
    pub const fn coverage(&self) -> UnitLoanActionCoverage {
        self.coverage
    }
    /// Catalog-bound records of a normally completed region; an empty unsupported range
    /// is not absence. Complete-empty also requires the positive entry/completion witness.
    pub fn action_ids(&self) -> impl ExactSizeIterator<Item = UnitLoanActionId> + '_ {
        self.records.clone().map(|index| UnitLoanActionId {
            identity: self.identity.clone(),
            index,
        })
    }
}
