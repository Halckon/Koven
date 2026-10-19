//! A source-qualified Return or local Move is an ordinary terminal, never an invented Call.
use super::*;
use crate::ssa::map_source::handoff::{self, Handoff};

#[derive(Clone, Copy)]
pub(in super::super) struct Proof {
    pub(in super::super) terminal: Handoff<UnitSymbolId>,
    drop_index: Option<usize>,
}
impl Proof {
    pub(in super::super) fn drop(
        self,
        owned: &CompilationUnitOwnership,
        span: Span,
    ) -> Result<Option<UnitDropFact>, LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let Handoff::Binding { root, marker, .. } = self.terminal else {
            return Ok(None);
        };
        let fact = *owned
            .drops()
            .get(self.drop_index.ok_or_else(bad)?)
            .ok_or_else(bad)?;
        if fact.target() != UnitDropTarget::Named(root) || fact.value_origin() != marker {
            return Err(bad());
        }
        Ok(Some(fact))
    }
}

pub(super) fn qualify(
    frontend: (
        &ParsedFile,
        &ValidatedCompilationUnitNames,
        &CompilationUnitTypes,
        &CompilationUnitOwnership,
    ),
    plan: &UnitMapIterationOwnershipPlan,
    source: SourceIdentity,
    span: Span,
) -> Result<Option<Proof>, LoweringError> {
    let (parsed, names, typed, owned) = frontend;
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let root = match source {
        SourceIdentity::Parameter { root, .. } | SourceIdentity::Formed { root, .. } => root,
        _ => return Ok(None),
    };
    if owned
        .drops()
        .iter()
        .any(|d| d.target() == UnitDropTarget::Named(root))
    {
        return Ok(None);
    }
    let source_unit = root.source_unit();
    let reference = |at| {
        let references = names
            .names()
            .references()
            .iter()
            .filter(|r| r.source_unit() == source_unit && r.span() == at)
            .collect::<Vec<_>>();
        match references.as_slice() {
            [r] => match r.target() {
                UnitReferenceTarget::Symbol(s) => Ok(*s),
                _ => Err(bad()),
            },
            _ => Err(bad()),
        }
    };
    let declaration = |at| {
        let symbols = names
            .names()
            .source_units()
            .get(source_unit.index())
            .ok_or_else(bad)?
            .resolution()
            .symbols()
            .iter()
            .filter(|s| s.span() == at && !s.is_synthetic())
            .collect::<Vec<_>>();
        let [symbol] = symbols.as_slice() else {
            return Err(bad());
        };
        let roots = owned
            .formed_owned_locals()
            .iter()
            .copied()
            .filter(|root| root.source_unit() == source_unit && root.symbol() == symbol.id())
            .collect::<Vec<_>>();
        match roots.as_slice() {
            [root] => Ok(*root),
            _ => Err(bad()),
        }
    };
    let storage = plan.descriptor().source_type();
    let Some(terminal) =
        handoff::find(parsed, span, root, storage, reference, declaration, |id| {
            let id = UnitExpressionId::new(source_unit, id);
            typed.expression_type(id).zip(typed.expression_category(id))
        })?
    else {
        return Ok(None);
    };
    let drop_index = match terminal {
        Handoff::Return { .. } => {
            let function = parsed
                .ast()
                .items()
                .iter()
                .filter(|(_, n)| {
                    matches!(n.payload(), Item::Function { .. }) && contains(n.span(), span)
                })
                .min_by_key(|(_, n)| n.span().end() - n.span().start())
                .ok_or_else(bad)?;
            let Item::Function {
                name: NameMarker::Present(marker),
                ..
            } = function.1.payload()
            else {
                return Err(bad());
            };
            let callables = typed
                .signatures()
                .declarations()
                .iter()
                .flat_map(|d| {
                    d.callable().into_iter().chain(
                        d.nominal()
                            .into_iter()
                            .flat_map(|n| n.members().iter().chain(n.companion_members())),
                    )
                })
                .filter(|c| c.name_span() == *marker)
                .collect::<Vec<_>>();
            let [callable] = callables.as_slice() else {
                return Err(bad());
            };
            if callable.return_type() != storage
                || callable.result_source() != CallableResultSource::Owned
            {
                return Err(bad());
            }
            None
        }
        Handoff::Binding {
            root: successor,
            marker,
            ..
        } => {
            if successor == root
                || typed.symbol_type(successor) != Some(storage)
                || !owned.formed_owned_locals().contains(&successor)
            {
                return Err(bad());
            }
            let drops = owned
                .drops()
                .iter()
                .enumerate()
                .filter(|(_, d)| d.target() == UnitDropTarget::Named(successor))
                .collect::<Vec<_>>();
            let [(index, fact)] = drops.as_slice() else {
                return Err(bad());
            };
            if fact.value_origin() != marker
                || !matches!(fact.point(), UnitDropPoint::CallReturn(call) if call.source_unit() == source_unit)
            {
                return Err(bad());
            }
            Some(*index)
        }
    };
    Ok(Some(Proof {
        terminal,
        drop_index,
    }))
}
