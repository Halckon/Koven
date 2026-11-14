//! Checked experimental source handoff, NOT a completed backend preflight or ordinary view.
use super::{TwoSourceCounterFact, TwoSourceExperiment};
use crate::{
    name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
    ownership_checking::{OwnedCompilationUnitViewError, handoff::validate_source_analysis},
    source::SourceMap,
    type_checking::{TypeEnvironment, ValidatedCompilationUnitTypes},
};

/// Private construction, no raw ownership getter, Deref, conversion or callback escape.
/// All source borrows remain bound to the exact issuing experimental analysis.
/// Backend declaration/binding/global checks have NOT been run by this handoff.
///
/// ```compile_fail,E0308
/// use lang_frontend::ownership_checking::{OwnedCompilationUnitView, two_source_experiment::ExperimentalUnitHandoff};
/// fn ordinary(_: &OwnedCompilationUnitView<'_, '_>) {}
/// fn reject(experimental: &ExperimentalUnitHandoff<'_, '_>) {
///     ordinary(experimental);
/// }
/// ```
pub struct ExperimentalUnitHandoff<'view, 'parsed: 'view> {
    experiment: &'view TwoSourceExperiment,
    sources: &'view SourceMap,
    inputs: &'view [SourceUnitInput<'parsed>],
    names: &'view ValidatedCompilationUnitNames,
    _environment: &'view TypeEnvironment,
    typed: &'view ValidatedCompilationUnitTypes,
}
impl<'view, 'parsed: 'view> ExperimentalUnitHandoff<'view, 'parsed> {
    /// Original checked source collection.
    pub fn sources(&self) -> &'view SourceMap {
        self.sources
    }
    /// Original parsed input slice; no AST copy.
    pub fn inputs(&self) -> &'view [SourceUnitInput<'parsed>] {
        self.inputs
    }
    /// Original validated names bound by the source gate.
    pub fn names(&self) -> &'view ValidatedCompilationUnitNames {
        self.names
    }
    /// Original typed analysis, not an ownership capability.
    pub fn types(&self) -> &'view ValidatedCompilationUnitTypes {
        self.typed
    }
    /// Controlled query against the actual retained ownership product, not reconstructed facts.
    pub fn owns(&self, fact: &TwoSourceCounterFact) -> bool {
        self.experiment.retained.owns_two_source_counter(fact)
    }
}
impl TwoSourceExperiment {
    /// Reuses the ordinary source-identity gate without creating ordinary owned authority.
    /// Error order remains source identity, then issuing analysis identity.
    pub fn handoff<'view, 'parsed: 'view>(
        &'view self,
        sources: &'view SourceMap,
        inputs: &'view [SourceUnitInput<'parsed>],
        names: &'view ValidatedCompilationUnitNames,
        environment: &'view TypeEnvironment,
        typed: &'view ValidatedCompilationUnitTypes,
    ) -> Result<ExperimentalUnitHandoff<'view, 'parsed>, OwnedCompilationUnitViewError> {
        validate_source_analysis(
            sources,
            inputs,
            names,
            environment,
            typed.types().signatures(),
        )?;
        if !self.matches_types(typed) || !self.retained.is_compatible_with(typed) {
            return Err(OwnedCompilationUnitViewError::MismatchedAnalysis);
        }
        Ok(ExperimentalUnitHandoff {
            experiment: self,
            sources,
            inputs,
            names,
            _environment: environment,
            typed,
        })
    }
}
