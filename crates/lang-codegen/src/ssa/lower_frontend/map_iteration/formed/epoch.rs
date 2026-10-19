//! A historical or future Move resolves the referenced owner at the original Name, not at the For.
use super::*;
pub(in crate::ssa::lower_frontend) struct Epoch {
    pub root: SymbolId,
    pub birth: Birth<SymbolId>,
    pub formation: Option<Formation<SymbolId>>,
    pub at: Span,
}
pub(in crate::ssa::lower_frontend) fn resolve(
    frontend: (
        &ParsedFile,
        &NameResolution,
        &TypedFile,
        &OwnershipCheckedFile,
    ),
    root: SymbolId,
    storage: TypeId,
    rhs: ExpressionId,
    span: Span,
) -> Result<Epoch, LoweringError> {
    let (parsed, names, typed, owned) = frontend;
    let bad = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let rhs = crate::ssa::map_source::transparent_expression(parsed, rhs, |id| {
        typed.expression_type(id).zip(typed.expression_category(id))
    })?;
    let node = parsed.ast().expressions().get(rhs).map_err(|_| bad())?;
    if typed.expression_type(rhs) != Some(storage) {
        return Err(bad());
    }
    match node.payload() {
        Expression::Call { .. } => Ok(Epoch {
            root,
            birth: Birth::Call(rhs),
            formation: None,
            at: node.span(),
        }),
        Expression::Name => {
            if typed.expression_category(rhs) != Some(ExpressionCategory::Place) {
                return Err(bad());
            }
            let reference = unique(
                names
                    .references()
                    .iter()
                    .filter(|r| r.span() == node.span()),
            )
            .ok_or_else(bad)?;
            let ReferenceTarget::Symbol(source) = reference.target() else {
                return Err(bad());
            };
            if names
                .symbols()
                .get(source.index())
                .is_some_and(|s| s.kind() == SymbolKind::ValueParameter)
            {
                let origin = super::owned_parameter(frontend, *source, storage, span)?;
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
