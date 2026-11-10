//! Incremental actual planner actions. No complete lifecycle certificate is issued yet.
use super::{UnitDropPoint, UnitIterationCleanupAction, UnitLoanFact};
use crate::name_resolution::SourceUnitId;

/// Coverage of the declared source observation, not a callable-completeness claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitLoanActionCoverage {
    /// Reserved for a separately validated closed region; this observer never issues it.
    Complete,
    /// Actual actions are present, but unregistered lifecycle families remain.
    Partial,
    /// No supported action was observed. Empty does not prove absence.
    Unsupported,
}

/// A record appended at the actual acquisition or cleanup publication site.
/// Record order is planner traversal order, not a cross-path execution timeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitLoanAction {
    /// The evaluated argument registered this exact pending call token.
    Begin(UnitLoanFact),
    /// An action from the single existing cleanup source, without sorting old tables.
    Cleanup {
        /// Source-qualified cleanup point; separate records are never coalesced by span.
        point: UnitDropPoint,
        /// Existing planner action. This observation does not execute it again.
        action: UnitIterationCleanupAction,
    },
}

/// Source-scoped partial evidence, not an absence or whole-callable oracle.
/// Result handoff, receiver activation and permission restoration remain uncovered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitLoanActionObservation {
    pub(super) source: SourceUnitId,
    pub(super) actions: Vec<UnitLoanAction>,
}

impl UnitLoanActionObservation {
    /// Same-analysis source identity of every record.
    #[must_use]
    pub const fn source(&self) -> SourceUnitId {
        self.source
    }

    /// Actual source records, including repeated distinct same-point actions.
    #[must_use]
    pub fn actions(&self) -> &[UnitLoanAction] {
        &self.actions
    }

    /// This incremental producer cannot certify Complete, including Complete-empty.
    #[must_use]
    pub fn coverage(&self) -> UnitLoanActionCoverage {
        if self.actions.is_empty() {
            UnitLoanActionCoverage::Unsupported
        } else {
            UnitLoanActionCoverage::Partial
        }
    }
}

impl super::CompilationUnitOwnership {
    /// Actual acquisition/cleanup source observation. All entries remain Partial/Unsupported.
    /// Errors and deferred analysis revoke both raw and public observations for the whole unit.
    #[must_use]
    pub fn loan_action_observations(&self) -> &[UnitLoanActionObservation] {
        &self.loan_action_observations
    }
}
