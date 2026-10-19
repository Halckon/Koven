//! Existing String-field Resource and body Guard identities from the same FE products.
use super::*;
use lang_frontend::{
    name_resolution::{NameResolution, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, DropPoint, DropTarget, OwnershipCheckedFile, UnitDropPoint,
        UnitDropTarget,
    },
    type_checking::{
        BuiltinType, CompilationUnitTypes, NominalKind, ParameterMode, TypeId, TypeKind, TypedFile,
        UnitExpressionId, UnitStatementId, UnitTypeId, UnitTypeKind,
    },
};
#[derive(Clone, Debug)]
pub(super) struct Representation {
    pub deinit: Span,
    pub strings: usize,
}
#[derive(Clone, Debug)]
pub(super) struct Guard {
    pub initializer: Span,
    pub origin: Span,
    pub deinit: Span,
}
fn class(parsed: &ParsedFile, marker: Span, deinit: Span) -> Result<(), Failure> {
    let original = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            matches!(n.payload(),Item::Classifier(c) if c.name==NameMarker::Present(marker))
                .then_some(n)
        }),
        "formation actual original Resource class",
    )?;
    let member = one(
        parsed.ast().items().iter().filter_map(|(_, n)| {
            (n.span() == deinit
                && inside(n.span(), original.span())
                && matches!(n.payload(), Item::Deinit { .. }))
            .then_some(n)
        }),
        "formation actual original deinit member",
    )?;
    need(
        inside(member.span(), original.span()),
        "formation original class owns actual deinit",
        member.span(),
    )
}
pub(super) fn single(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    ty: TypeId,
) -> Result<Option<Representation>, Failure> {
    if typed.types().get(ty) == Some(&TypeKind::Builtin(BuiltinType::String)) {
        return Ok(None);
    }
    let Some(TypeKind::Nominal { nominal, arguments }) = typed.types().get(ty) else {
        return Err(failure("formation existing component representation", ty));
    };
    let d = one(
        typed.nominals().iter().filter(|d| d.id() == *nominal),
        "formation actual Resource descriptor",
    )?;
    let deinit = d
        .deinit()
        .ok_or_else(|| failure("formation actual Resource deinit", d))?;
    let original = parsed.ast().items().get(deinit.item()).unwrap();
    let marker = names.symbols()[nominal.symbol().index()].span();
    class(parsed, marker, original.span())?;
    need(
        arguments.is_empty()
            && d.kind() == NominalKind::Class
            && d.type_parameters().is_empty()
            && d.interfaces().is_empty()
            && deinit.owner() == *nominal
            && deinit.receiver_type() == ty
            && deinit.receiver_mode() == ParameterMode::Borrow
            && d.fields().iter().all(|field| {
                typed.symbol_type(*field).is_some_and(|t| {
                    typed.types().get(t) == Some(&TypeKind::Builtin(BuiltinType::String))
                })
            })
            && typed.is_resource_type(ty) == Some(true),
        "formation exact existing Resource nominal and owned String fields",
        d,
    )?;
    Ok(Some(Representation {
        deinit: original.span(),
        strings: d.fields().len(),
    }))
}
pub(super) fn unit(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
) -> Result<Option<Representation>, Failure> {
    if typed.types().get(ty) == Some(&UnitTypeKind::Builtin(BuiltinType::String)) {
        return Ok(None);
    }
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments,
    }) = typed.types().get(ty)
    else {
        return Err(failure("formation existing qualified representation", ty));
    };
    let d = one(
        typed
            .signatures()
            .declarations()
            .iter()
            .filter(|d| d.declaration() == *declaration)
            .filter_map(|d| d.nominal()),
        "formation actual qualified Resource descriptor",
    )?;
    let deinit = d
        .deinit()
        .ok_or_else(|| failure("formation qualified actual Resource deinit", d))?;
    let original = parsed.ast().items().get(deinit.item().item()).unwrap();
    let symbol = d.symbol();
    let marker = one(
        names
            .names()
            .source_units()
            .iter()
            .filter(|u| u.source_unit() == symbol.source_unit())
            .filter_map(|u| {
                u.resolution()
                    .symbols()
                    .get(symbol.symbol().index())
                    .map(|s| s.span())
            }),
        "formation qualified original class marker",
    )?;
    class(parsed, marker, original.span())?;
    need(
        arguments.is_empty()
            && d.kind() == NominalKind::Class
            && d.type_parameters().is_empty()
            && d.interfaces().is_empty()
            && deinit.receiver_type() == ty
            && deinit.receiver_mode() == ParameterMode::Borrow
            && d.fields().iter().all(|field| {
                typed.types().get(field.ty()) == Some(&UnitTypeKind::Builtin(BuiltinType::String))
            })
            && typed.is_resource_type(ty) == Some(true),
        "formation exact qualified Resource nominal and String fields",
        d,
    )?;
    Ok(Some(Representation {
        deinit: original.span(),
        strings: d.fields().len(),
    }))
}
fn guard_site(parsed: &ParsedFile, site: &Site) -> Result<Option<(Span, ExpressionId)>, Failure> {
    let declarations = parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, n)| match n.payload() {
            Item::Variable {
                name: NameMarker::Present(name),
                initializer,
                ..
            } if inside(n.span(), site.for_span) => Some((*name, *initializer)),
            _ => None,
        })
        .collect::<Vec<_>>();
    need(
        declarations.len() <= 1,
        "formation original body-local Guard set",
        &declarations,
    )?;
    Ok(declarations.first().copied())
}
pub(super) fn single_guard(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    site: &Site,
) -> Result<Option<Guard>, Failure> {
    let Some((marker, initializer)) = guard_site(parsed, site)? else {
        return Ok(None);
    };
    let local = one(
        names.symbols().iter().filter(|s| s.span() == marker),
        "formation actual Guard root",
    )?;
    let construction = typed
        .construction(initializer)
        .ok_or_else(|| failure("formation same-analysis Guard construction", initializer))?;
    let repr = single(parsed, names, typed, construction.result_type())?
        .ok_or_else(|| failure("formation actual Resource Guard", initializer))?;
    let drop = one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == DropTarget::Named(local.id())),
        "formation sole original Guard Drop fact",
    )?;
    need(
        repr.strings == 0
            && construction.arguments().is_empty()
            && typed.symbol_type(local.id()) == Some(construction.result_type())
            && drop.point() == DropPoint::AfterStatement(site.body)
            && drop.value_origin() == marker,
        "formation original Guard formation and actual body-local Drop point",
        (construction, drop),
    )?;
    Ok(Some(Guard {
        initializer: span(parsed, initializer),
        origin: drop.value_origin(),
        deinit: repr.deinit,
    }))
}
pub(super) fn unit_guard(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    site: &Site,
) -> Result<Option<Guard>, Failure> {
    let Some((marker, initializer)) = guard_site(parsed, site)? else {
        return Ok(None);
    };
    let source = one(
        names.names().source_units().iter(),
        "formation Guard original unit",
    )?;
    let symbol = one(
        source
            .resolution()
            .symbols()
            .iter()
            .filter(|s| s.span() == marker),
        "formation qualified Guard root",
    )?;
    let root = one(
        owned.drops().iter().filter_map(|drop| match drop.target() {
            UnitDropTarget::Named(root)
                if root.source_unit() == source.source_unit()
                    && root.symbol() == symbol.id()
                    && source
                        .resolution()
                        .symbols()
                        .get(root.symbol().index())
                        .is_some_and(|original| original.span() == marker) =>
            {
                Some(root)
            }
            _ => None,
        }),
        "formation actual qualified Guard root from original Drop fact",
    )?;
    let expression = UnitExpressionId::new(source.source_unit(), initializer);
    let construction = typed
        .construction(expression)
        .ok_or_else(|| failure("formation qualified Guard construction", expression))?;
    let repr = unit(parsed, names, typed, construction.result_type())?
        .ok_or_else(|| failure("formation qualified Resource Guard", expression))?;
    let drop = one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(root)),
        "formation qualified original Guard Drop",
    )?;
    need(
        repr.strings == 0
            && construction.arguments().is_empty()
            && typed.symbol_type(root) == Some(construction.result_type())
            && drop.point()
                == UnitDropPoint::AfterStatement(UnitStatementId::new(
                    source.source_unit(),
                    site.body,
                ))
            && drop.value_origin() == marker,
        "formation qualified Guard body-local cleanup identity",
        (construction, drop),
    )?;
    Ok(Some(Guard {
        initializer: span(parsed, initializer),
        origin: drop.value_origin(),
        deinit: repr.deinit,
    }))
}

pub(super) fn graph(
    module: &crate::ssa::model::Module,
    g: &crate::ssa::map_provider_exits_pipeline_tests::query_graph::Graph<'_>,
    input: &Inputs,
    seed: crate::ssa::model::EntityId,
    next: &crate::ssa::model::Instruction,
) -> Result<(), Failure> {
    use crate::ssa::{
        map_provider_exits_pipeline_tests::query_graph::{result, value},
        model::*,
    };
    let (kind, key, value_type) = module
        .map_container(g.function.entity(seed).unwrap().ty.semantic_type())
        .ok_or_else(|| failure("formation actual represented source", seed))?;
    need(
        kind == match input.kind {
            lang_frontend::type_checking::MapIterationKind::Map => MapContainerKind::Map,
            lang_frontend::type_checking::MapIterationKind::MutableMap => {
                MapContainerKind::MutableMap
            }
        },
        "formation represented container kind equals same-analysis descriptor",
        (kind, input.kind),
    )?;
    need(
        module.type_kind(key) == Some(&SsaTypeKind::StringOwner),
        "formation original String key represented storage",
        key,
    )?;
    if let Some(repr) = &input.representation {
        let Some(SsaTypeKind::HeapOwner { payload, .. }) = module.type_kind(value_type) else {
            return Err(failure(
                "formation Resource actual heap representation",
                value_type,
            ));
        };
        let deinit = module
            .deinits
            .get(&value_type)
            .and_then(|id| module.function(*id))
            .ok_or_else(|| failure("formation actual Resource deinit function", value_type))?;
        need(
            deinit.origin == Origin::Source(repr.deinit)
                && payload.map(|t| strings(module, t)).unwrap_or(0) == repr.strings,
            "formation represented Resource uses same FE nominal deinit/String fields",
            deinit.id,
        )?;
        need(!g.instructions.iter().any(|i|matches!(i.operation,Operation::Drop{owner} if g.function.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()==value_type)),"formation provider never drops borrowed Resource component owner",g.function.id)?;
    } else {
        need(
            module.type_kind(value_type) == Some(&SsaTypeKind::StringOwner),
            "formation original String value storage",
            value_type,
        )?;
    }
    if let Some(guard) = &input.guard {
        let allocation = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(guard.initializer)
                    && matches!(i.operation, Operation::HeapAllocate { .. })
            }),
            "formation actual original Guard allocation",
        )?;
        let ty = g
            .function
            .entity(result(allocation)?)
            .unwrap()
            .ty
            .semantic_type();
        let deinit = module
            .deinits
            .get(&ty)
            .and_then(|id| module.function(*id))
            .ok_or_else(|| failure("formation actual Guard deinit", ty))?;
        let drop=one(g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::Drop{owner} if EntityId::Value(owner)==result(allocation).unwrap())),"formation Guard exact unique current allocation Drop")?;
        let vend=one(g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::BorrowEnd{loan} if g.definition(EntityId::Loan(loan)).is_ok_and(|v|matches!(v.operation,Operation::MapIterationValueLoan{..})))),"formation actual unique value component End")?;
        let read=one(g.instructions.iter().copied().filter(|i|matches!(&i.operation,Operation::DirectCall{arguments,..} if arguments.len()==2 && arguments.iter().all(|a|matches!(a,EntityId::Loan(_))))),"formation body actual ordered component reader")?;
        need(
            deinit.origin == Origin::Source(guard.deinit)
                && drop.origin == Origin::Source(guard.origin)
                && drop.block == next.block
                && read.block == drop.block
                && allocation.block == read.block
                && vend.block == drop.block
                && g.position(allocation)? < g.position(read)?
                && g.position(read)? < g.position(drop)?
                && g.position(drop)? < g.position(vend)?
                && g.position(vend)? < g.position(next)?,
            "formation original Guard lives through read then drops before V/K End and Next",
            (allocation, read, drop, vend),
        )?;
        need(
            value(allocation).is_ok(),
            "formation actual Guard owner Value",
            allocation,
        )?;
    }
    Ok(())
}
fn strings(module: &crate::ssa::model::Module, ty: crate::ssa::model::SsaTypeId) -> usize {
    match module.type_kind(ty) {
        Some(crate::ssa::model::SsaTypeKind::StringOwner) => 1,
        Some(crate::ssa::model::SsaTypeKind::Aggregate { fields, .. }) => {
            fields.iter().map(|&t| strings(module, t)).sum()
        }
        _ => 0,
    }
}
