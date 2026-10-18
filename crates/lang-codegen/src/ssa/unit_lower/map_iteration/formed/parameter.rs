//! A moved parameter keeps its actual callable/ABI identity, distinct from its new local root.
use super::*;

pub(in super::super::super) fn entry_parameter(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    root: UnitSymbolId,
    storage: UnitTypeId,
    span: Span,
) -> Result<(usize, Span), LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let symbol = names
        .names()
        .source_units()
        .get(root.source_unit().index())
        .and_then(|u| u.resolution().symbols().get(root.symbol().index()))
        .ok_or_else(bad)?;
    if root.source_unit().index() >= names.names().source_units().len()
        || symbol.kind() != SymbolKind::ValueParameter
        || symbol.is_synthetic()
        || symbol.span().source_id() != parsed.source_id()
        || typed.symbol_type(root) != Some(storage)
        || owned.binding_kind(root) != Some(OwnershipBindingKind::Owned)
    {
        return Err(bad());
    }
    let mut candidates = Vec::new();
    for declaration in typed.signatures().declarations() {
        for callable in declaration.callable().into_iter().chain(
            declaration
                .nominal()
                .into_iter()
                .flat_map(|n| n.members().iter().chain(n.companion_members())),
        ) {
            for (index, parameter) in callable.parameters().iter().enumerate() {
                if parameter.symbol() == Some(root) {
                    if parameter.mode() != ParameterMode::Value || parameter.ty() != storage {
                        return Err(bad());
                    }
                    candidates.push(index + usize::from(callable.receiver().is_some()));
                }
            }
        }
    }
    let [slot] = candidates.as_slice() else {
        return Err(bad());
    };
    // The actual parameter marker and provider must belong to the same smallest callable.
    let enclosing = |at| {
        parsed
            .ast()
            .items()
            .iter()
            .filter(|(_, n)| matches!(n.payload(), Item::Function { .. }) && contains(n.span(), at))
            .min_by_key(|(_, n)| n.span().end() - n.span().start())
            .map(|(id, _)| id)
    };
    if enclosing(symbol.span()).is_none() || enclosing(symbol.span()) != enclosing(span) {
        return Err(bad());
    }
    Ok((*slot, symbol.span()))
}
