//! Singleton production adapter and private experimental common-root test entry.
use super::*;

pub(super) fn lower_unit_impl(
    source_inputs: (&SourceMap, &[SourceUnitInput<'_>]),
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    constant_owned: Option<&ConstEnabledOwnedUnit>,
    entry: DeclarationId,
    #[cfg(test)] capture: Option<&mut pr120_verifier_diagnostics::Capture>,
) -> Result<(Program, FunctionId), LoweringError> {
    let (program, roots) = super::lower_unit_roots_impl(
        source_inputs,
        names,
        typed,
        owned,
        constant_owned,
        (&[entry], false),
        #[cfg(test)]
        capture,
    )?;
    let id = roots.get(&entry).copied().ok_or(LoweringError {
        kind: LoweringErrorKind::MissingFact,
        span: None,
    })?;
    Ok((program, id))
}

#[cfg(test)]
fn lower_roots(
    view: &lang_frontend::ownership_checking::OwnedCompilationUnitView<'_, '_>,
    roots: &[DeclarationId],
) -> Result<(Program, BTreeMap<DeclarationId, FunctionId>), LoweringError> {
    super::lower_unit_roots_impl(
        (view.sources(), view.inputs()),
        view.names(),
        view.types(),
        view.ownership(),
        None,
        (roots, true),
        None,
    )
}

#[cfg(test)]
mod tests;
