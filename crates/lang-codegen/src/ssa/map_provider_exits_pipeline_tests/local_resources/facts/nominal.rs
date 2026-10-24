//! Exact original Resource class, owned String field and nullable component type.
use super::*;
use lang_frontend::{
    ast::{ItemId, StatementId},
    parser::{Item, NameMarker},
    type_checking::{NominalKind, UnitItemId, UnitTypeId},
};
struct Class {
    name: Span,
    field: Span,
    item: ItemId,
    body: StatementId,
}
fn class(sources: &SourceMap, parsed: &ParsedFile) -> Result<Class, Failure> {
    let class = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            let Item::Classifier(c) = n.payload() else {
                return None;
            };
            matches!(c.name,NameMarker::Present(name) if sources.slice(name)==Ok("Resource"))
                .then_some(c)
        }),
        "original represented Resource class",
    )?;
    let NameMarker::Present(name) = class.name else {
        return Err(failure("original Resource marker", class));
    };
    let field = one(
        class
            .primary_constructor
            .as_ref()
            .ok_or_else(|| failure("actual Resource primary fields", class))?
            .fields
            .iter(),
        "actual owned String field",
    )?;
    let NameMarker::Present(field) = field.name else {
        return Err(failure("actual Resource field marker", field));
    };
    need(
        sources.slice(field) == Ok("tag"),
        "actual original tag field",
        field,
    )?;
    let (item, body) = one(
        class
            .body
            .as_ref()
            .ok_or_else(|| failure("actual Resource body", class))?
            .members
            .iter()
            .filter_map(|id| {
                let n = parsed.ast().items().get(*id).ok()?;
                match n.payload() {
                    Item::Deinit { body, .. } => Some((*id, *body)),
                    _ => None,
                }
            }),
        "actual original Resource deinit",
    )?;
    Ok(Class {
        name,
        field,
        item,
        body,
    })
}

pub(in crate::ssa::map_provider_exits_pipeline_tests) fn unit(
    nullable: bool,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    value: UnitTypeId,
) -> Result<(), Failure> {
    let class = class(sources, parsed)?;
    let inner = match (nullable, typed.types().get(value)) {
        (true, Some(UnitTypeKind::Nullable(inner))) => *inner,
        (false, Some(UnitTypeKind::Nominal { .. })) => value,
        other => return Err(failure("qualified exact nullable component", other)),
    };
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments,
    }) = typed.types().get(inner)
    else {
        return Err(failure("actual qualified Resource nominal", inner));
    };
    let nominal = one(
        typed
            .signatures()
            .declarations()
            .iter()
            .filter(|d| d.declaration() == *declaration)
            .filter_map(|d| d.nominal()),
        "actual qualified Resource signature",
    )?;
    let declared = one(
        names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|d| d.id() == *declaration && d.name_span() == class.name),
        "original qualified Resource declaration",
    )?;
    let unit = declared.source_unit();
    let field = one(nominal.fields().iter(), "actual qualified Resource field")?;
    let deinit = nominal
        .deinit()
        .ok_or_else(|| failure("actual qualified Resource deinit", nominal))?;
    need(
        arguments.is_empty()
            && nominal.kind() == NominalKind::Class
            && field.span() == class.field
            && typed.types().get(field.ty()) == Some(&UnitTypeKind::Builtin(BuiltinType::String))
            && deinit.owner() == *declaration
            && deinit.item() == UnitItemId::new(unit, class.item)
            && deinit.body() == UnitStatementId::new(unit, class.body)
            && deinit.receiver_type() == inner
            && deinit.receiver_mode() == ParameterMode::Borrow,
        "qualified original Resource/String/deinit identity",
        (nominal, deinit),
    )?;
    Ok(())
}
