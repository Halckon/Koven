//! General ordinary local formation uses original Expression ownership, not a lexical carrier.
use super::*;
use crate::ssa::map_source::{Formation, named_formation};
use lang_frontend::{
    name_resolution::ReferenceTarget,
    ownership_checking::{IterationSource, MapIterationComponent, OwnershipBindingKind},
    parser::{NameMarker, VariableKind},
    type_checking::{ExpressionCategory, IntrinsicTypeConstructor, MapIterationKind},
};

pub(super) fn formation(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    root: SymbolId,
    storage: TypeId,
    span: Span,
) -> Result<Formation<SymbolId>, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    named_formation(
        parsed,
        span,
        root,
        |root| {
            let symbol = names.symbols().get(root.index()).ok_or_else(missing)?;
            if symbol.kind() != SymbolKind::Variable {
                return Err(unsupported());
            }
            if symbol.is_synthetic()
                || typed.symbol_type(root) != Some(storage)
                || !owned.formed_owned_locals().contains(&root)
            {
                return Err(missing());
            }
            let item =
                unique(
                    parsed
                        .ast()
                        .items()
                        .iter()
                        .filter_map(|(_, node)| match node.payload() {
                            Item::Variable {
                                kind,
                                name: NameMarker::Present(marker),
                                initializer,
                                ..
                            } if *marker == symbol.span() => Some((*kind, *marker, *initializer)),
                            _ => None,
                        }),
                )
                .ok_or_else(missing)?;
            if !matches!(item.0, VariableKind::Val | VariableKind::Var) {
                return Err(unsupported());
            }
            if typed.expression_type(item.2) != Some(storage) {
                return Err(missing());
            }
            Ok((item.1, item.2))
        },
        |span| {
            let reference = unique(names.references().iter().filter(|r| r.span() == span))
                .ok_or_else(missing)?;
            match reference.target() {
                ReferenceTarget::Symbol(root) => Ok(*root),
                _ => Err(missing()),
            }
        },
        |id| typed.expression_type(id).zip(typed.expression_category(id)),
        typed
            .map_puts()
            .iter()
            .map(|d| d.receiver())
            .chain(typed.map_removes().iter().map(|d| d.receiver())),
    )
}

pub(super) fn qualify<'a>(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &'a OwnershipCheckedFile,
    plan: &'a MapIterationOwnershipPlan,
    body: StatementId,
    span: Span,
) -> Result<qualify::Qualified<'a>, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let d = plan.descriptor();
    let Some(LoanTarget::Place(place)) = plan.source() else {
        return Err(unsupported());
    };
    let root = place.root();
    if !place.is_root()
        || plan.source_kind() != &IterationSource::BorrowedSource(LoanTarget::Place(place.clone()))
        || plan.source_access() != MapIterationSourceAccess::Owned
        || typed.copyability(d.source_type()) != Some(Copyability::MoveOnly)
        || typed.expression_category(d.source()) != Some(ExpressionCategory::Place)
        || typed.expression_type(d.source()) != Some(d.source_type())
        || d.key_binding() == d.value_binding()
        || d.bindings().contains(&root)
    {
        return Err(missing());
    }
    let resource = typed
        .is_resource_type(d.source_type())
        .ok_or_else(missing)?;
    if qualify::has_enclosing_control(parsed, span)
        || (!resource
            && [d.key_type(), d.value_type()]
                .iter()
                .any(|t| builtin_type(typed, *t) != Some(BuiltinType::String)))
    {
        return Err(unsupported());
    }
    let constructor = match d.kind() {
        MapIterationKind::Map => IntrinsicTypeConstructor::Map,
        MapIterationKind::MutableMap => IntrinsicTypeConstructor::MutableMap,
    };
    if !matches!(typed.types().get(d.source_type()),Some(TypeKind::Intrinsic{constructor:actual,arguments}) if *actual==constructor && arguments.as_slice()==[d.key_type(),d.value_type()])
    {
        return Err(missing());
    }
    for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let binding = plan.bindings()[index];
        let loan = &plan.loans()[index];
        if binding.symbol() != d.bindings()[index]
            || binding.kind() != OwnershipBindingKind::Shared
            || loan.binding() != binding.symbol()
            || loan.component() != component
            || loan.statement() != d.statement()
            || loan.source() != d.source()
            || Some(loan.origin()) != plan.source()
            || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
            || typed.symbol_type(binding.symbol()) != Some([d.key_type(), d.value_type()][index])
        {
            return Err(missing());
        }
    }
    let formation = formation(parsed, names, typed, owned, root, d.source_type(), span)?;
    local::producer(
        parsed,
        names,
        typed,
        owned,
        formation.producer,
        d.source_type(),
        span,
    )?;
    let body_exit = exit_shape::body_exit(parsed, names, typed, *d, body, span)?;
    exit_shape::cleanup(owned, plan, body, body_exit, span)?;
    let owner = if resource {
        resource::qualify(
            parsed,
            names,
            typed,
            owned,
            plan,
            (root, Some(formation.producer)),
            body_exit,
        )?
    } else {
        source_owner::qualify_formed(
            parsed,
            names,
            owned,
            plan,
            (root, formation.producer),
            body,
            body_exit,
        )?
    };
    Ok(qualify::Qualified {
        plan,
        source: SourceIdentity::Formed {
            symbol: root,
            producer: formation.producer,
        },
        body,
        origin: names
            .symbols()
            .get(root.index())
            .ok_or_else(missing)?
            .span(),
        owner: Some(owner),
        body_exit,
        conditional: None,
        nested: None,
        local: None,
    })
}
