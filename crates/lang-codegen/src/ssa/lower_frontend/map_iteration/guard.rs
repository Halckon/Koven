//! Original lexical Resource sentinel proof, shared by temporary direct exits.
use super::*;
use lang_frontend::{
    name_resolution::ReferenceTarget,
    parser::{NameMarker, VariableKind},
    type_checking::{ConstructionTarget, NominalKind},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Local {
    pub(super) symbol: SymbolId,
    pub(super) initializer: ExpressionId,
    pub(super) initializer_span: Span,
    pub(super) origin: Span,
    pub(super) ty: TypeId,
    pub(super) deinit: lang_frontend::ast::ItemId,
}
pub(super) fn local(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    statement: StatementId,
    span: Span,
) -> Result<Local, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let node = parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|_| missing())?;
    let Statement::LocalVariable { declaration } = node.payload() else {
        return Err(unsupported());
    };
    let item = parsed
        .ast()
        .items()
        .get(*declaration)
        .map_err(|_| missing())?;
    let Item::Variable {
        kind: VariableKind::Val,
        name: NameMarker::Present(origin),
        initializer,
        ..
    } = item.payload()
    else {
        return Err(unsupported());
    };
    let symbol = unique(
        names
            .symbols()
            .iter()
            .filter(|symbol| symbol.span() == *origin && symbol.kind() == SymbolKind::Variable),
    )
    .ok_or_else(missing)?;
    let ty = typed.symbol_type(symbol.id()).ok_or_else(missing)?;
    let TypeKind::Nominal { nominal, arguments } = typed.types().get(ty).ok_or_else(missing)?
    else {
        return Err(unsupported());
    };
    let nominal =
        unique(typed.nominals().iter().filter(|item| item.id() == *nominal)).ok_or_else(missing)?;
    if nominal.kind() != NominalKind::Class
        || !arguments.is_empty()
        || !nominal.type_parameters().is_empty()
        || !nominal.fields().is_empty()
        || !nominal.variants().is_empty()
        || !nominal.interfaces().is_empty()
    {
        return Err(unsupported());
    }
    match typed.is_resource_type(ty) {
        Some(true) => {}
        Some(false) => return Err(unsupported()),
        None => return Err(missing()),
    }
    let deinit = nominal.deinit().ok_or_else(missing)?;
    let init = parsed
        .ast()
        .expressions()
        .get(*initializer)
        .map_err(|_| missing())?;
    let Expression::Call {
        callee,
        arguments,
        type_arguments,
        ..
    } = init.payload()
    else {
        return Err(unsupported());
    };
    if !arguments.is_empty() || !type_arguments.is_empty() {
        return Err(unsupported());
    }
    let callee = parsed
        .ast()
        .expressions()
        .get(*callee)
        .map_err(|_| missing())?;
    if callee.payload() != &Expression::Name {
        return Err(unsupported());
    }
    let reference = unique(
        names
            .references()
            .iter()
            .filter(|r| r.span() == callee.span()),
    )
    .ok_or_else(missing)?;
    if reference.target() != &ReferenceTarget::Symbol(nominal.id().symbol()) {
        return Err(missing());
    }
    let construction = typed.construction(*initializer).ok_or_else(missing)?;
    if construction.expression() != *initializer
        || construction.target() != ConstructionTarget::Nominal(nominal.id())
        || construction.result_type() != ty
        || typed.expression_type(*initializer) != Some(ty)
        || !construction.instance().type_arguments().is_empty()
        || !construction.arguments().is_empty()
        || symbol.is_synthetic()
        || !owned.formed_owned_locals().contains(&symbol.id())
        || deinit.owner() != nominal.id()
        || deinit.receiver_type() != ty
        || deinit.receiver_mode() != ParameterMode::Borrow
    {
        return Err(missing());
    }
    let deinit_node = parsed
        .ast()
        .items()
        .get(deinit.item())
        .map_err(|_| missing())?;
    if !matches!(deinit_node.payload(), Item::Deinit { body, .. } if *body == deinit.body())
        || !matches!(
            parsed
                .ast()
                .statements()
                .get(deinit.body())
                .map_err(|_| missing())?
                .payload(),
            Statement::Block { .. }
        )
    {
        return Err(missing());
    }
    let nominal_symbol = names
        .symbols()
        .get(nominal.id().symbol().index())
        .ok_or_else(missing)?;
    let declaration = unique(parsed.ast().items().iter().filter_map(
        |(_, item)| match item.payload() {
            Item::Classifier(declaration)
                if declaration.name == NameMarker::Present(nominal_symbol.span()) =>
            {
                Some(declaration)
            }
            _ => None,
        },
    ))
    .ok_or_else(missing)?;
    if !declaration
        .body
        .as_ref()
        .is_some_and(|body| body.members.contains(&deinit.item()))
    {
        return Err(missing());
    }
    Ok(Local {
        symbol: symbol.id(),
        initializer: *initializer,
        initializer_span: init.span(),
        origin: *origin,
        ty,
        deinit: deinit.item(),
    })
}
