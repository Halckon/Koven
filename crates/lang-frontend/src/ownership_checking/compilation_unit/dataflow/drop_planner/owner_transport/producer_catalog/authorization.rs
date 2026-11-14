//! Closed experimental payload, not an ordinary lowering permission or loop proof.
use super::*;
use crate::type_checking::{CompilationUnitTypes, UnitItemId};

pub(in super::super::super) struct Authorized<'a> {
    catalog: &'a Catalog,
}
impl Catalog {
    pub(in super::super::super) fn authorize(
        &self,
        typed: &CompilationUnitTypes,
        callable: UnitItemId,
    ) -> Option<Authorized<'_>> {
        self.close()?;
        if !Arc::ptr_eq(&self.analysis, typed.analysis_owner())
            || self.coverage.as_ref()?.callable != callable
        {
            return None;
        }
        Some(Authorized { catalog: self })
    }
}
impl Authorized<'_> {
    /// Only the actual producer token from this observation can select its cleanup.
    pub(in super::super::super) fn producer(
        &self,
        token: &Token,
    ) -> Option<(UnitExpressionId, crate::ownership_checking::UnitDropFact)> {
        let (expression, _) = self.catalog.definitions.iter().find(|(_, t)| t == token)?;
        let (_, fact) = self.catalog.retirements.iter().find(|(t, _)| t == token)?;
        Some((*expression, *fact))
    }
}
