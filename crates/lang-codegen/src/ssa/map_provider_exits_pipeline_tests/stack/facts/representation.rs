//! Original component and Guard nominal identities from the observed typed products.
use super::*;
use lang_frontend::{
    name_resolution::{NameResolution, SymbolKind, ValidatedCompilationUnitNames},
    ownership_checking::{CompilationUnitOwnership, OwnershipCheckedFile},
    type_checking::{
        BuiltinType, CompilationUnitTypes, ConstructionTarget, NominalKind, ParameterMode, TypeId,
        TypeKind, TypedFile, UnitConstructionTarget, UnitExpressionId, UnitTypeId, UnitTypeKind,
    },
};

pub(super) fn single(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    ty: TypeId,
) -> Result<Representation, Failure> {
    let (inner, nullable) = match typed.types().get(ty) {
        Some(TypeKind::Nullable(inner)) => (*inner, true),
        _ => (ty, false),
    };
    if typed.types().get(inner) == Some(&TypeKind::Builtin(BuiltinType::String)) {
        need(
            !nullable,
            "stack original String key/value is nonnullable",
            ty,
        )?;
        return Ok(Representation {
            nullable: false,
            deinit: None,
            strings: 0,
        });
    }
    let Some(TypeKind::Nominal { nominal, arguments }) = typed.types().get(inner) else {
        return Err(failure("stack actual component type", ty));
    };
    let descriptor = one(
        typed.nominals().iter().filter(|n| n.id() == *nominal),
        "stack actual component nominal",
    )?;
    let symbol = one(
        names
            .symbols()
            .iter()
            .filter(|s| s.id() == nominal.symbol() && s.kind() == SymbolKind::Classifier),
        "stack original component classifier marker",
    )?;
    let class = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            matches!(n.payload(),Item::Classifier(c) if c.name==NameMarker::Present(symbol.span()))
                .then_some(n)
        }),
        "stack component actual AST classifier",
    )?;
    let deinit = descriptor
        .deinit()
        .ok_or_else(|| failure("stack Resource actual deinit descriptor", descriptor))?;
    let original = parsed.ast().items().get(deinit.item()).unwrap();
    need(
        arguments.is_empty()
            && descriptor.kind() == NominalKind::Class
            && descriptor.type_parameters().is_empty()
            && descriptor.interfaces().is_empty()
            && deinit.owner() == *nominal
            && deinit.receiver_type() == inner
            && deinit.receiver_mode() == ParameterMode::Borrow
            && matches!(original.payload(),Item::Deinit{body,..} if *body==deinit.body())
            && inside(original.span(), class.span())
            && descriptor.fields().iter().all(|f| {
                typed.symbol_type(*f).is_some_and(|t| {
                    typed.types().get(t) == Some(&TypeKind::Builtin(BuiltinType::String))
                })
            }),
        "stack exact component Resource nominal/String fields/original deinit",
        (descriptor, deinit, class),
    )?;
    Ok(Representation {
        nullable,
        deinit: Some(original.span()),
        strings: descriptor.fields().len(),
    })
}
pub(super) fn unit(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
) -> Result<Representation, Failure> {
    let (inner, nullable) = match typed.types().get(ty) {
        Some(UnitTypeKind::Nullable(inner)) => (*inner, true),
        _ => (ty, false),
    };
    if typed.types().get(inner) == Some(&UnitTypeKind::Builtin(BuiltinType::String)) {
        need(!nullable, "stack qualified String is nonnullable", ty)?;
        return Ok(Representation {
            nullable: false,
            deinit: None,
            strings: 0,
        });
    }
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments,
    }) = typed.types().get(inner)
    else {
        return Err(failure("stack qualified actual component type", ty));
    };
    let descriptor = one(
        typed
            .signatures()
            .declarations()
            .iter()
            .filter(|d| d.declaration() == *declaration)
            .filter_map(|d| d.nominal()),
        "stack qualified component nominal",
    )?;
    let declared = one(
        names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|d| d.id() == *declaration),
        "stack original qualified Resource declaration",
    )?;
    let class = parsed.ast().items().get(declared.root()).unwrap();
    let deinit = descriptor
        .deinit()
        .ok_or_else(|| failure("stack qualified Resource deinit", descriptor))?;
    let original = parsed.ast().items().get(deinit.item().item()).unwrap();
    need(
        arguments.is_empty()
            && descriptor.kind() == NominalKind::Class
            && descriptor.type_parameters().is_empty()
            && descriptor.interfaces().is_empty()
            && deinit.owner() == *declaration
            && deinit.receiver_type() == inner
            && deinit.receiver_mode() == ParameterMode::Borrow
            && deinit.item().source_unit() == declared.source_unit()
            && inside(original.span(), class.span())
            && matches!(original.payload(),Item::Deinit{body,..} if *body==deinit.body().statement())
            && descriptor.fields().iter().all(|f| {
                typed.types().get(f.ty()) == Some(&UnitTypeKind::Builtin(BuiltinType::String))
            }),
        "stack qualified exact Resource/String field/original deinit",
        (descriptor, deinit, class),
    )?;
    Ok(Representation {
        nullable,
        deinit: Some(original.span()),
        strings: descriptor.fields().len(),
    })
}
fn locals(parsed: &ParsedFile, sites: &[AstSite]) -> Vec<(Span, lang_frontend::ast::ExpressionId)> {
    parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, n)| {
            let Item::Variable {
                name: NameMarker::Present(marker),
                initializer,
                ..
            } = n.payload()
            else {
                return None;
            };
            sites
                .iter()
                .any(|s| inside(n.span(), s.body_span))
                .then_some((*marker, *initializer))
        })
        .collect()
}
pub(super) fn single_guards(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    sites: &[AstSite],
) -> Result<Vec<GuardInput>, Failure> {
    let mut result = Vec::new();
    for (marker, initializer) in locals(parsed, sites) {
        let local = one(
            names
                .symbols()
                .iter()
                .filter(|s| s.span() == marker && s.kind() == SymbolKind::Variable),
            "stack original Guard local",
        )?;
        let construction = typed
            .construction(initializer)
            .ok_or_else(|| failure("stack Guard actual typed construction", initializer))?;
        let repr = single(parsed, names, typed, construction.result_type())?;
        need(
            repr.deinit.is_some()
                && !repr.nullable
                && repr.strings == 0
                && construction.arguments().is_empty()
                && construction.instance().type_arguments().is_empty()
                && matches!(typed.types().get(construction.result_type()),Some(TypeKind::Nominal{nominal,arguments}) if construction.target()==ConstructionTarget::Nominal(*nominal)&&arguments.is_empty())
                && typed.symbol_type(local.id()) == Some(construction.result_type())
                && typed.expression_type(initializer) == Some(construction.result_type())
                && typed.is_resource_type(construction.result_type()) == Some(true)
                && owned.formed_owned_locals().contains(&local.id()),
            "stack same FE Guard owner/construction/Resource/deinit",
            (local, construction, &repr),
        )?;
        result.push(GuardInput {
            marker,
            initializer: parsed.ast().expressions().get(initializer).unwrap().span(),
            deinit: repr.deinit.unwrap(),
        });
    }
    Ok(result)
}
pub(super) fn unit_guards(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    sites: &[AstSite],
) -> Result<Vec<GuardInput>, Failure> {
    let source = one(
        names.names().source_units().iter(),
        "stack Guard original qualified source",
    )?;
    let unit = source.source_unit();
    let mut result = Vec::new();
    for (marker, initializer) in locals(parsed, sites) {
        let local = one(
            owned.formed_owned_locals().iter().copied().filter(|s| {
                s.source_unit() == unit
                    && source.resolution().symbols().iter().any(|n| {
                        n.id() == s.symbol()
                            && n.span() == marker
                            && n.kind() == SymbolKind::Variable
                    })
            }),
            "stack qualified original formed Guard",
        )?;
        let construction = typed
            .construction(UnitExpressionId::new(unit, initializer))
            .ok_or_else(|| failure("stack qualified Guard construction", initializer))?;
        let repr = self::unit(parsed, names, typed, construction.result_type())?;
        need(
            repr.deinit.is_some()
                && !repr.nullable
                && repr.strings == 0
                && construction.arguments().is_empty()
                && construction.instance().type_arguments().is_empty()
                && matches!(typed.types().get(construction.result_type()),Some(UnitTypeKind::Nominal{declaration,arguments}) if construction.target()==UnitConstructionTarget::Nominal(*declaration)&&arguments.is_empty())
                && typed.symbol_type(local) == Some(construction.result_type())
                && typed.expression_type(UnitExpressionId::new(unit, initializer))
                    == Some(construction.result_type()),
            "stack qualified Guard exact formation and nominal",
            (construction, &repr, local),
        )?;
        result.push(GuardInput {
            marker,
            initializer: parsed.ast().expressions().get(initializer).unwrap().span(),
            deinit: repr.deinit.unwrap(),
        });
    }
    Ok(result)
}
