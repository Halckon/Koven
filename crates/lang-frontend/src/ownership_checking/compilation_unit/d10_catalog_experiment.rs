//! Opt-in no-loop catalog handoff. Not a production primitive lowering capability.
use super::*;
use crate::type_checking::UnitItemId;

/// Opaque identity from the original planner observation, not an AST-derived token.
#[derive(Clone, Debug)]
pub struct ProducerIdentity {
    nonce: Arc<()>,
    index: usize,
}
impl PartialEq for ProducerIdentity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.nonce, &other.nonce) && self.index == other.index
    }
}
impl Eq for ProducerIdentity {}
impl ProducerIdentity {
    pub(crate) fn new(nonce: Arc<()>, index: usize) -> Self {
        Self { nonce, index }
    }
}
/// Immutable producer and its actual cleanup.
#[derive(Debug, PartialEq, Eq)]
pub struct Producer {
    pub(crate) identity: ProducerIdentity,
    pub(crate) expression: UnitExpressionId,
    pub(crate) cleanup: UnitDropFact,
}
impl Producer {
    /// Original observation identity.
    #[must_use]
    pub const fn identity(&self) -> &ProducerIdentity {
        &self.identity
    }
    /// Actual literal expression that produced this owner.
    #[must_use]
    pub const fn expression(&self) -> UnitExpressionId {
        self.expression
    }
    /// Exact cleanup published by this analysis.
    #[must_use]
    pub const fn cleanup(&self) -> UnitDropFact {
        self.cleanup
    }
}
/// Actual closed one-root Replace relation; not a loop permission.
#[derive(Debug, PartialEq, Eq)]
pub struct Replacement {
    pub(crate) root: UnitSymbolId,
    pub(crate) call: UnitExpressionId,
    pub(crate) previous: ProducerIdentity,
    pub(crate) incoming: ProducerIdentity,
}
impl Replacement {
    /// Actual updated root.
    #[must_use]
    pub const fn root(&self) -> UnitSymbolId {
        self.root
    }
    /// Actual checked Replace call.
    #[must_use]
    pub const fn call(&self) -> UnitExpressionId {
        self.call
    }
    /// Producer of the old result.
    #[must_use]
    pub const fn previous(&self) -> &ProducerIdentity {
        &self.previous
    }
    /// Producer installed in the root.
    #[must_use]
    pub const fn incoming(&self) -> &ProducerIdentity {
        &self.incoming
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Pending {
    pub(crate) typed: Arc<()>,
    pub(crate) callable: UnitItemId,
    pub(crate) producers: Vec<Producer>,
    pub(crate) replacement: Replacement,
}

/// Final validated ownership and its original analysis's closed catalog.
/// No public constructor or mutable payload; not accepted by ordinary codegen gates.
pub struct BoundCatalog {
    owned: ValidatedCompilationUnitOwnership,
    pending: Pending,
}
impl BoundCatalog {
    /// Exact final ownership instance (cloning preserves its analysis identity).
    #[must_use]
    pub const fn ownership(&self) -> &ValidatedCompilationUnitOwnership {
        &self.owned
    }
    /// Source-qualified callable covered by this no-loop experiment.
    #[must_use]
    pub const fn callable(&self) -> UnitItemId {
        self.pending.callable
    }
    /// Actual planner producers paired with their exact cleanup facts.
    #[must_use]
    pub fn producers(&self) -> &[Producer] {
        &self.pending.producers
    }
    /// Relation observed and closed by the original planner.
    #[must_use]
    pub const fn replacement(&self) -> &Replacement {
        &self.pending.replacement
    }
    /// Checks the final ownership identity, not just compatible typed provenance.
    #[must_use]
    pub fn matches(&self, unit: &super::super::OwnedCompilationUnitView<'_, '_>) -> bool {
        self.owned.ownership().is_same_analysis(unit.ownership())
    }
}
impl Pending {
    pub(super) fn bind(self, owned: CompilationUnitOwnership) -> Option<BoundCatalog> {
        if !Arc::ptr_eq(&self.typed, &owned.provenance.typed_analysis_owner)
            || !owned.diagnostics().is_empty()
            || !owned.deferred().is_empty()
            || owned.drops().len() != self.producers.len()
            || self.producers.iter().any(|producer| {
                owned
                    .drops()
                    .iter()
                    .filter(|f| **f == producer.cleanup)
                    .count()
                    != 1
            })
        {
            return None;
        }
        let owned = owned.validate().ok()?;
        Some(BoundCatalog {
            owned,
            pending: self,
        })
    }
}

/// Runs a fresh full ownership analysis. Failed or uncovered candidates yield no handoff.
/// This does not retrofit a catalog onto an existing ownership product.
pub fn analyze(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
) -> Result<Option<BoundCatalog>, OwnershipCheckingError> {
    let (owned, mut pending) = analysis::analyze_with_catalog(
        sources,
        inputs,
        names.names(),
        environment,
        typed.types(),
        false,
    )?;
    if inputs.len() != 1 || pending.len() != 1 {
        return Ok(None);
    }
    Ok(pending.pop().and_then(|p| p.bind(owned)))
}
