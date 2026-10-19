//! Unit source-qualified last-use obligations, independent of single owner/condition tables.
use super::super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::super::ast::{Anchors, Input};
use lang_frontend::{
    name_resolution::{SymbolKind, UnitReferenceTarget, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, UnitDropPoint as P, UnitDropTarget as T,
        UnitIterationCleanupAction as A, UnitIterationExitKind as E, UnitLoanTarget,
    },
    type_checking::{CompilationUnitTypes, UnitExpressionId, UnitStatementId},
};

pub(in super::super) fn query(
    input: &Input,
    anchors: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let source = one(
        names.names().source_units().iter(),
        "local unit one original source",
    )?;
    let unit = source.source_unit();
    for site in &anchors.sites {
        let original = one(
            source.resolution().symbols().iter().filter(|s| {
                s.span() == site.local.marker
                    && s.kind() == SymbolKind::Variable
                    && !s.is_synthetic()
            }),
            "local unit original declaration",
        )?;
        let reference = one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.source_unit() == unit && r.span() == site.source_span),
            "local unit original source reference",
        )?;
        let UnitReferenceTarget::Symbol(root) = reference.target() else {
            return Err(failure("local unit actual source symbol", reference));
        };
        need(
            root.source_unit() == unit && root.symbol() == original.id(),
            "local unit declaration/reference qualified identity",
            (root, original),
        )?;
        let statement = UnitStatementId::new(unit, site.statement);
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("local unit own plan", site))?;
        need(
            typed.map_iteration(statement) == Some(p.descriptor())
                && matches!(p.source(), Some(UnitLoanTarget::Place(v)) if v.is_root() && v.root() == *root),
            "local unit actual plan uses original root",
            p,
        )?;
        let point = site.size.as_ref().map_or(P::LoopExit(statement), |s| {
            P::CallReturn(UnitExpressionId::new(unit, s.expression))
        });
        let drops = owned
            .drops()
            .iter()
            .filter(|d| d.target() == T::Named(*root))
            .collect::<Vec<_>>();
        need(
            matches!(drops.as_slice(), [d] if d.point() == point && d.value_origin() == site.local.marker),
            "local unit unique exact last-use Drop, no dead predecessor re-drop",
            &drops,
        )?;
        let exhausted = one(
            p.exits().iter().filter(|e| e.kind() == E::Exhaustion),
            "local unit exact exhaustion",
        )?;
        let expected = if input.later {
            vec![A::FinishProvider(statement), A::EndSource(statement)]
        } else {
            vec![
                A::FinishProvider(statement),
                A::EndSource(statement),
                A::Drop(*drops[0]),
            ]
        };
        need(
            exhausted.point() == P::LoopExit(statement) && exhausted.actions() == expected,
            "local unit complete exhaustion actions and source-before-owner order",
            exhausted,
        )?;
    }
    Ok(())
}
