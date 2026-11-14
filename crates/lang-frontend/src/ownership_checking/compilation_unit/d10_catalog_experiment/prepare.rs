//! Explicit default-off preparation, not normal driver admission or a fallback.
use super::*;
use crate::{
    diagnostic::Diagnostic,
    type_checking::{FinalTypeAdmissionError, complete_compilation_unit_types},
};

/// Original admission failures are retained separately from experimental scope refusals.
#[derive(Debug)]
pub enum PrepareNamedEntryError {
    /// Final type completion failed; retains diagnostics or the original internal error.
    FinalTypeAdmission(FinalTypeAdmissionError),
    /// Ownership analysis infrastructure failed.
    Ownership(OwnershipCheckingError),
    /// Actual ownership diagnostics and deferred facts, before any scope decision.
    OwnershipAdmission {
        /// Original published diagnostics, without rewriting their severity or span.
        diagnostics: Vec<Diagnostic>,
        /// Original deferred facts, including their source identity and reason.
        deferred: Vec<UnitOwnershipDeferredFact>,
    },
    /// The source-set is outside the closed scope or produced no named candidate.
    Unsupported,
    /// More than one candidate was produced; no first-match dispatch is permitted.
    Ambiguous,
    /// The unique candidate failed its original final-owned binder.
    Unproved,
}

/// Owns the same typed and bound analyses. No mutable access or ownership conversion is offered.
pub struct PreparedNamedEntry {
    typed: ValidatedCompilationUnitTypes,
    bound: BoundNamedEntry,
}
impl PreparedNamedEntry {
    /// The original admitted typed analysis used by the single ownership run.
    #[must_use]
    pub const fn typed(&self) -> &ValidatedCompilationUnitTypes {
        &self.typed
    }
    /// The original sealed selection bound to that run's final ownership.
    #[must_use]
    pub const fn bound(&self) -> &BoundNamedEntry {
        &self.bound
    }
}

/// Complete types, analyze ownership exactly once, and select exactly one original candidate.
/// This opt-in basic path does not perform constant rewriting or ordinary driver admission.
pub fn prepare_named_entry(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: ValidatedCompilationUnitTypes,
) -> Result<PreparedNamedEntry, PrepareNamedEntryError> {
    complete_compilation_unit_types(sources, inputs, names, environment, typed.types())
        .map_err(PrepareNamedEntryError::FinalTypeAdmission)?;
    let (owned, _, _, _, _, _, before, after) = analysis::analyze_with_catalog(
        sources,
        inputs,
        names.names(),
        environment,
        typed.types(),
        false,
    )
    .map_err(PrepareNamedEntryError::Ownership)?;
    if !owned.diagnostics().is_empty() || !owned.deferred().is_empty() {
        return Err(PrepareNamedEntryError::OwnershipAdmission {
            diagnostics: owned.diagnostics().to_vec(),
            deferred: owned.deferred().to_vec(),
        });
    }
    if inputs.len() != 1 {
        return Err(PrepareNamedEntryError::Unsupported);
    }
    let bound = super::named_entry::select(owned, before, after)?;
    Ok(PreparedNamedEntry { typed, bound })
}
