//! Single ownership analysis, exclusive named-source dispatch. Not ordinary driver admission.
use super::*;

/// The unique closed named-source shape selected in this analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamedEntryOrder {
    /// Parent replacement occurs before the nested child loop.
    BeforeChild,
    /// Parent replacement occurs after the nested child loop.
    AfterChild,
}

enum Inner {
    BeforeChild(BoundNamedWrapper),
    AfterChild(BoundNamedAfterChildWrapper),
}

/// Sealed single-run selection. No constructor, ownership extraction, or fallback admission.
pub struct BoundNamedEntry {
    inner: Inner,
}

/// Read-only references to the original sealed capabilities, not a constructor for BoundNamedEntry.
/// A consumer of the unified capability must take BoundNamedEntry and derive this view itself.
pub enum NamedEntryView<'a> {
    /// The original BeforeChild bound from this exact final ownership run.
    BeforeChild(&'a BoundNamedWrapper),
    /// The original AfterChild bound from this exact final ownership run.
    AfterChild(&'a BoundNamedAfterChildWrapper),
}

impl BoundNamedEntry {
    /// Borrow the actual selected original bound. No new analysis or field normalization occurs.
    #[must_use]
    pub const fn view(&self) -> NamedEntryView<'_> {
        match &self.inner {
            Inner::BeforeChild(bound) => NamedEntryView::BeforeChild(bound),
            Inner::AfterChild(bound) => NamedEntryView::AfterChild(bound),
        }
    }
    /// Borrow the original complete final ownership instance without extracting or replacing it.
    #[must_use]
    pub const fn ownership(&self) -> &ValidatedCompilationUnitOwnership {
        match &self.inner {
            Inner::BeforeChild(bound) => bound.ownership(),
            Inner::AfterChild(bound) => bound.ownership(),
        }
    }
    /// Compare the exact final ownership analysis identity.
    #[must_use]
    pub fn matches(&self, unit: &super::super::super::OwnedCompilationUnitView<'_, '_>) -> bool {
        match &self.inner {
            Inner::BeforeChild(bound) => bound.matches(unit),
            Inner::AfterChild(bound) => bound.matches(unit),
        }
    }
    /// Original source-qualified wrapper declaration.
    #[must_use]
    pub const fn entry(&self) -> UnitItemId {
        match &self.inner {
            Inner::BeforeChild(bound) => bound.entry(),
            Inner::AfterChild(bound) => bound.entry(),
        }
    }
    /// Derived from the selected bound, never stored independently from it.
    #[must_use]
    pub const fn order(&self) -> NamedEntryOrder {
        match &self.inner {
            Inner::BeforeChild(_) => NamedEntryOrder::BeforeChild,
            Inner::AfterChild(_) => NamedEntryOrder::AfterChild,
        }
    }
}

/// Analyze once and accept exactly one original named candidate across both order channels.
/// All analysis errors propagate. Zero, multiple or ambiguous candidates return None; a binder
/// refusal never tries the other order or an ordinary lowering fallback. Default-off only.
pub fn analyze_named_entry(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
) -> Result<Option<BoundNamedEntry>, OwnershipCheckingError> {
    let (owned, _, _, _, _, _, before, after, _) = analysis::analyze_with_catalog(
        sources,
        inputs,
        names.names(),
        environment,
        typed.types(),
        false,
    )?;
    if inputs.len() != 1 {
        return Ok(None);
    }
    Ok(select(owned, before, after).ok())
}

pub(super) fn select(
    owned: CompilationUnitOwnership,
    mut before: Vec<WrappedDualPending>,
    mut after: Vec<WrappedAfterChildPending>,
) -> Result<BoundNamedEntry, super::prepare::PrepareNamedEntryError> {
    use super::prepare::PrepareNamedEntryError;
    let inner = match (before.len(), after.len()) {
        (0, 0) => return Err(PrepareNamedEntryError::Unsupported),
        (1, 0) => before
            .pop()
            .and_then(|pending| pending.bind(owned))
            .map(Inner::BeforeChild),
        (0, 1) => after
            .pop()
            .and_then(|pending| pending.bind_after(owned))
            .map(Inner::AfterChild),
        _ => return Err(PrepareNamedEntryError::Ambiguous),
    };
    inner
        .map(|inner| BoundNamedEntry { inner })
        .ok_or(PrepareNamedEntryError::Unproved)
}
