//! Resolve a named epoch at its original Move reference while preserving its outer Drop identity.
use super::*;
pub(in crate::ssa::unit_lower) struct Epoch {
    pub root: UnitSymbolId,
    pub birth: Birth<UnitSymbolId>,
    pub formation: Option<Formation<UnitSymbolId>>,
    pub at: Span,
}
pub(in crate::ssa::unit_lower) fn resolve(
    frontend: (
        &ParsedFile,
        &ValidatedCompilationUnitNames,
        &CompilationUnitTypes,
        &CompilationUnitOwnership,
    ),
    root: UnitSymbolId,
    storage: UnitTypeId,
    rhs: ExpressionId,
    span: Span,
) -> Result<Epoch, LoweringError> {
    let (parsed, names, typed, owned) = frontend;
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let rhs = crate::ssa::map_source::transparent_expression(parsed, rhs, |id| {
        let id = UnitExpressionId::new(root.source_unit(), id);
        typed.expression_type(id).zip(typed.expression_category(id))
    })?;
    let node = parsed.ast().expressions().get(rhs).map_err(|_| bad())?;
    let expression = UnitExpressionId::new(root.source_unit(), rhs);
    if typed.expression_type(expression) != Some(storage) {
        return Err(bad());
    }
    match node.payload() {
        Expression::Call { .. } => Ok(Epoch {
            root,
            birth: Birth::Call(rhs),
            formation: None,
            at: node.span(),
        }),
        Expression::If { .. } => {
            expression_birth::qualify(parsed, names, typed, owned, expression, storage, span)?;
            Ok(Epoch {
                root,
                birth: Birth::Expression(rhs),
                formation: None,
                at: node.span(),
            })
        }
        Expression::Name => {
            if typed.expression_category(expression) != Some(ExpressionCategory::Place) {
                return Err(bad());
            }
            let references = names
                .names()
                .references()
                .iter()
                .filter(|r| r.source_unit() == root.source_unit() && r.span() == node.span())
                .collect::<Vec<_>>();
            let [reference] = references.as_slice() else {
                return Err(bad());
            };
            let UnitReferenceTarget::Symbol(source) = reference.target() else {
                return Err(bad());
            };
            if names
                .names()
                .source_units()
                .iter()
                .find(|u| u.source_unit() == source.source_unit())
                .and_then(|u| u.resolution().symbols().get(source.symbol().index()))
                .is_some_and(|s| s.kind() == SymbolKind::ValueParameter)
            {
                let (_, origin) =
                    super::entry_parameter(parsed, names, typed, owned, *source, storage, span)?;
                return Ok(Epoch {
                    root: *source,
                    birth: Birth::EntryParameter {
                        root: *source,
                        reference: rhs,
                        origin,
                    },
                    formation: None,
                    at: node.span(),
                });
            }
            let formation =
                super::raw_formation(parsed, names, typed, owned, *source, storage, node.span())?;
            Ok(Epoch {
                root: *source,
                birth: formation.birth,
                formation: Some(formation),
                at: node.span(),
            })
        }
        _ => Err(unsupported()),
    }
}
