//! Explicit bounded two-source preparation. Never accepts an experimental package or ordinary view.
use super::{
    e2_joint_plan::{JointPlan, PlanRejection},
    model::{FunctionId, Program},
};
use lang_frontend::{
    name_resolution::{DeclarationId, SourceUnitInput, ValidatedCompilationUnitNames},
    ownership_checking::{
        OwnedCompilationUnitViewError, OwnershipCheckingError,
        two_source_experiment::analyze_two_source_experiment,
    },
    source::SourceMap,
    type_checking::{TypeEnvironment, ValidatedCompilationUnitTypes},
};

/// An immutable, completely lowered bounded unit. No ordinary ownership or mutable IR conversion.
/// Construction always performs fresh ownership checking and complete plan/body/replay validation.
/// ```compile_fail,E0308
/// use lang_codegen::emit_native_two_source_unit_object;
/// use lang_frontend::ownership_checking::two_source_experiment::ExperimentalUnitHandoff;
/// fn reject(experimental: &ExperimentalUnitHandoff<'_, '_>) {
///     emit_native_two_source_unit_object(experimental, std::path::Path::new("out.o"));
/// }
/// ```
/// ```compile_fail
/// use lang_codegen::ValidatedTwoSourceUnit;
/// fn reject(unit: &ValidatedTwoSourceUnit<'_>) {
///     let _ = unit.sources; // A public field would make this compile.
/// }
/// ```
pub struct ValidatedTwoSourceUnit<'a> {
    sources: &'a SourceMap,
    _typed: &'a ValidatedCompilationUnitTypes,
    program: Program,
    entry: DeclarationId,
    function: FunctionId,
}
impl ValidatedTwoSourceUnit<'_> {
    pub(crate) fn parts(&self) -> (&SourceMap, &Program, DeclarationId, FunctionId) {
        (self.sources, &self.program, self.entry, self.function)
    }
}

/// Preparation failures never yield a partial capability or create an output artifact.
#[derive(Debug)]
pub enum TwoSourceUnitError {
    /// The real ownership checker failed.
    Ownership(OwnershipCheckingError),
    /// The source/analysis handoff was inconsistent.
    Identity(OwnedCompilationUnitViewError),
    /// The existing native source-entry gate rejected the requested entry.
    Entry(crate::NativeObjectError),
    /// No complete supported fact/entry closure; not a language-invalidity verdict.
    UnsupportedSelection,
    /// The original source mapping error.
    SourceMapping(crate::NativeObjectError),
    /// The unchanged full Unit preflight rejected a construct.
    Preflight(crate::NativeObjectError),
    /// Closed plan/body/replay could not be proven; no fallback is performed.
    UnprovenCoverage,
}

/// Opt-in, single original entry route. Ordinary and constant unit entry points are unchanged.
/// No disjointness is inferred between Shared String parameters. The current source-entry
/// descriptor still accepts only literal String arguments, not named-owner alias expressions.
#[allow(clippy::too_many_arguments)]
pub fn prepare_two_source_unit<'a>(
    sources: &'a SourceMap,
    inputs: &'a [SourceUnitInput<'a>],
    names: &'a ValidatedCompilationUnitNames,
    environment: &'a TypeEnvironment,
    typed: &'a ValidatedCompilationUnitTypes,
    entry: DeclarationId,
) -> Result<ValidatedTwoSourceUnit<'a>, TwoSourceUnitError> {
    let experiment = analyze_two_source_experiment(sources, inputs, names, environment, typed)
        .map_err(TwoSourceUnitError::Ownership)?
        .ok_or(TwoSourceUnitError::UnsupportedSelection)?;
    let handoff = experiment
        .handoff(sources, inputs, names, environment, typed)
        .map_err(TwoSourceUnitError::Identity)?;
    let entries = experiment
        .entries(typed)
        .ok_or(TwoSourceUnitError::UnsupportedSelection)?;
    let mut selected = entries
        .iter()
        .filter(|candidate| candidate.entry() == entry);
    let descriptor = selected
        .next()
        .ok_or(TwoSourceUnitError::UnsupportedSelection)?;
    if selected.next().is_some() {
        return Err(TwoSourceUnitError::UnsupportedSelection);
    }
    let mut selected = experiment.facts().iter().filter(|fact| {
        experiment
            .callable(fact)
            .map(lang_frontend::type_checking::UnitCallTarget::Declaration)
            == Some(descriptor.instance().target())
    });
    let fact = selected
        .next()
        .ok_or(TwoSourceUnitError::UnsupportedSelection)?;
    if selected.next().is_some() {
        return Err(TwoSourceUnitError::UnsupportedSelection);
    }
    crate::native::validate_unit_entry(
        names,
        typed.types(),
        crate::NativeUnitEntry::NoArguments(entry),
    )
    .map_err(TwoSourceUnitError::Entry)?;
    let plan = JointPlan::build_checked(&handoff, &experiment, &[fact], &[descriptor]).map_err(
        |error| match error {
            PlanRejection::AnalysisIdentityMismatch => {
                TwoSourceUnitError::Identity(OwnedCompilationUnitViewError::MismatchedAnalysis)
            }
            PlanRejection::SelectionRejected => TwoSourceUnitError::UnsupportedSelection,
            PlanRejection::SourceMappingRejected(error) => {
                TwoSourceUnitError::SourceMapping(crate::native::map_lowering_error(error))
            }
            PlanRejection::UnitPreflightRejected(error) => {
                TwoSourceUnitError::Preflight(crate::native::map_lowering_error(error))
            }
            PlanRejection::ClosedPlanRejected => TwoSourceUnitError::UnprovenCoverage,
        },
    )?;
    let package = plan
        .publish_common(&handoff)
        .ok_or(TwoSourceUnitError::UnprovenCoverage)?;
    if !package.matches_handoff(&handoff) {
        return Err(TwoSourceUnitError::UnprovenCoverage);
    }
    super::verify::verify_program(package.program())
        .map_err(|_| TwoSourceUnitError::UnprovenCoverage)?;
    let function = package
        .entry(descriptor, typed)
        .ok_or(TwoSourceUnitError::UnprovenCoverage)?;
    let program = package.into_program();
    Ok(ValidatedTwoSourceUnit {
        sources,
        _typed: typed,
        program,
        entry,
        function,
    })
}

#[cfg(test)]
mod tests;
