//! Bind ordinary named obligations to this analysis's real local and callable.
//! These facts feed the unchanged SSA construction/parameter and cleanup lineage
//! query; neither a same-table getter nor an invented owner ID proves ownership.
use super::*;

pub(super) fn named(
    fe: &Fe<'_>,
    root: UnitSymbolId,
    initializer: UnitExpressionId,
    callable_span: Span,
    callable_name: Span,
) -> Outcome<()> {
    let parsed = file(fe.files, root.source_unit())?;
    let binding = symbol(fe.names, root)?;
    let (_, local_span, actual_initializer) = declaration(parsed, binding.span())?;
    let actual_callable = super::callable(parsed, local_span)?;
    let declarations: Vec<_> = fe
        .names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|value| {
            value.source_unit() == root.source_unit()
                && value.name_span() == callable_name
                && value.kind() == SymbolKind::Function
        })
        .collect();
    let declaration = *one(
        &declarations,
        "unit named owner's exact callable declaration",
    )?;
    let item = parsed
        .ast()
        .items()
        .get(declaration.root())
        .map_err(|error| missing("unit named owner's real callable item", error))?;
    let signature = fe
        .typed
        .signatures()
        .declaration(declaration.id())
        .ok_or_else(|| missing("unit named owner's callable signature", declaration))?;
    let callable = signature
        .callable()
        .ok_or_else(|| missing("unit named owner's concrete callable", signature))?;
    let function = symbol(fe.names, signature.symbol())?;
    gate(
        "unit named owner has exact source-qualified local, initializer and callable",
        initializer.source_unit() == root.source_unit()
            && actual_initializer == initializer.expression()
            && binding.kind() == SymbolKind::Variable
            && !binding.is_synthetic()
            && binding.span().source_id() == parsed.source_id()
            && contains(local_span, binding.span())
            && contains(local_span, span(parsed, actual_initializer)?)
            && actual_callable == (callable_span, callable_name)
            && item.span() == callable_span
            && matches!(item.payload(), Item::Function {
                name: NameMarker::Present(name), ..
            } if *name == callable_name)
            && signature.declaration() == declaration.id()
            && signature.package() == declaration.package()
            && signature.symbol().source_unit() == root.source_unit()
            && fe.names.names().declaration_symbol(declaration.id()) == Some(signature.symbol())
            && function.kind() == SymbolKind::Function
            && !function.is_synthetic()
            && function.span() == callable_name
            && callable.target() == UnitCallableTarget::Declaration(declaration.id())
            && callable.name_span() == callable_name
            && callable.receiver().is_none()
            && fe.names.names().index().source_units().iter().any(|unit| {
                unit.id() == root.source_unit()
                    && unit.source_id() == parsed.source_id()
                    && unit.package() == declaration.package()
            })
            && fe.owned.formed_owned_locals().contains(&root),
        (root, binding, declaration, signature, actual_callable),
    )
}
