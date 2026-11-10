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
    /// Shared explicit owned-parameter receiver acquisition and normal retirement.
    SharedReceiver,
    /// Actual destruction of the independently established named owner.
    NamedSourceDrop,
}

/// One actually entered caller-side normal-return region. Protocol 1 covers literal arguments;
/// protocol 2 covers a Shared explicit receiver backed by an owned class parameter.
/// Complete covers only the listed local families: it does not prove the callee returns,
/// callee-internal/Abort paths, outer loans, result handoffs or the callable.
/// Admission requires independent calls in a flat ordinary function, with Int/Unit results
/// and, for protocol 1, plain String literal Borrow arguments (including zero arguments).
/// Protocol 2 requires zero arguments and an explicit owned non-Resource class parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitDirectCallLoanRegion {
    pub(super) identity: Arc<()>,
    pub(super) call: UnitExpressionId,
    pub(super) callable: Option<UnitItemId>,
    pub(super) coverage: UnitLoanActionCoverage,
    pub(super) protocol: u32,
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
        self.protocol
    }
    /// Exact local families covered when coverage is Complete.
    #[must_use]
    pub const fn families(&self) -> &'static [UnitLoanRegionFamily] {
        if self.protocol == 2 {
            return &[
                UnitLoanRegionFamily::SharedReceiver,
                UnitLoanRegionFamily::NamedSourceDrop,
            ];
        }
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
