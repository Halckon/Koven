//! Real Map storage, original Resource deinit and actual populated/empty factory values.
mod factory;
use super::*;
use lang_frontend::parser::{Item, NameMarker};

pub(super) fn check(
    case: &Case,
    module: &Module,
    sources: &SourceMap,
    parsed: &ParsedFile,
    map: SsaTypeId,
    factory: &Function,
    empty: bool,
) -> Result<(SsaTypeId, SsaTypeId), Failure> {
    let (kind, key, val) = module
        .map_container(map)
        .ok_or_else(|| failure("temporary form actual Map storage", map))?;
    need(
        kind == if case.mutable {
            MapContainerKind::MutableMap
        } else {
            MapContainerKind::Map
        } && if matches!(case.representation, 2 | 4) {
            module.type_kind(key)
                == Some(&SsaTypeKind::Integer {
                    bits: 32,
                    signed: true,
                })
        } else {
            module.type_kind(key) == Some(&SsaTypeKind::StringOwner)
        },
        "temporary form exact container kind and represented Int/String key",
        (kind, key),
    )?;
    let resource = match (case.representation, module.type_kind(val)) {
        (4, Some(SsaTypeKind::StringOwner)) => None,
        (0 | 2, Some(SsaTypeKind::HeapOwner { .. })) => Some(val),
        (
            1,
            Some(SsaTypeKind::Aggregate {
                fields,
                ownership: Ownership::MoveOnly,
                ..
            }),
        ) if fields.len() == 1 => Some(fields[0]),
        (3, Some(SsaTypeKind::NullableHandle { inner })) => Some(*inner),
        _ => {
            return Err(failure(
                "temporary form original represented value storage",
                (case.representation, module.type_kind(val)),
            ));
        }
    };
    if let Some(resource) = resource {
        let Some(SsaTypeKind::HeapOwner { payload, .. }) = module.type_kind(resource) else {
            return Err(failure(
                "temporary form underlying actual Resource heap owner",
                resource,
            ));
        };
        if matches!(case.representation, 0 | 3) {
            let payload = payload
                .ok_or_else(|| failure("temporary form Resource owns real tag field", resource))?;
            need(
                strings(module, payload) == 1,
                "temporary form original owned String payload representation",
                module.type_kind(payload),
            )?;
        }
        let (_, marker) = ast::nominal(sources, parsed, "Resource")?;
        let class = one(
            parsed.roots().iter().filter_map(|id| {
                let node = parsed.ast().items().get(*id).ok()?;
                let Item::Classifier(class) = node.payload() else {
                    return None;
                };
                matches!(class.name, NameMarker::Present(name) if name == marker).then_some(class)
            }),
            "temporary form original Resource class",
        )?;
        let deinit = one(
            class
                .body
                .as_ref()
                .unwrap()
                .members
                .iter()
                .filter_map(|id| {
                    let node = parsed.ast().items().get(*id).ok()?;
                    matches!(node.payload(), Item::Deinit { .. }).then_some(node.span())
                }),
            "temporary form actual original Resource deinit",
        )?;
        let actual = module
            .deinits
            .get(&resource)
            .and_then(|id| module.function(*id))
            .ok_or_else(|| failure("temporary form actual Resource deinit binding", resource))?;
        need(
            actual.origin == Origin::Source(deinit),
            "temporary form storage uses exact source deinit",
            actual.id,
        )?;
    }
    let g = Graph::new(factory)?;
    factory::check(case, module, sources, parsed, map, &g, empty)?;
    let puts = g
        .instructions
        .iter()
        .copied()
        .filter(|i| matches!(i.operation, Operation::MapPut { .. }))
        .collect::<Vec<_>>();
    need(
        puts.len()
            == if empty {
                0
            } else if case.representation == 3 {
                3
            } else {
                2
            },
        "temporary form populated and empty factory real insertion counts",
        &puts,
    )?;
    let constructs = g
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::MapConstruct { .. }))
        .count();
    need(
        constructs == 1,
        "temporary form each actual factory creates one fresh empty storage",
        factory.id,
    )?;
    let allocations = g.instructions.iter().filter(|i| matches!(i.operation, Operation::HeapAllocate {owner, ..} if Some(owner) == resource)).count();
    need(
        allocations == if empty || resource.is_none() { 0 } else { 2 },
        "temporary form actual Resource population count",
        allocations,
    )?;
    let mut some = 0;
    let mut none = 0;
    for put in puts {
        let Operation::MapPut {
            key: actual_key,
            value: actual_val,
            ..
        } = put.operation
        else {
            unreachable!()
        };
        need(
            factory.entity(EntityId::Value(actual_key)).unwrap().ty == EntityType::Value(key)
                && factory.entity(EntityId::Value(actual_val)).unwrap().ty
                    == EntityType::Value(val),
            "temporary form real insertion owns exact represented key/value",
            put,
        )?;
        if case.representation == 3 {
            let value = g.definition(EntityId::Value(actual_val))?;
            match value.operation {
                Operation::NullableWrap { nullable, owner } => {
                    need(
                        nullable == val
                            && factory.entity(EntityId::Value(owner)).unwrap().ty
                                == EntityType::Value(resource.unwrap())
                            && matches!(g.definition(EntityId::Value(owner))?.operation, Operation::HeapAllocate {owner:ty, ..} if Some(ty) == resource),
                        "temporary form Some owns actual represented Resource allocation",
                        value,
                    )?;
                    some += 1;
                }
                Operation::NullableNull { nullable } if nullable == val => {
                    none += 1;
                }
                _ => return Err(failure("temporary form actual Some/None insertion", value)),
            }
        }
        if case.representation == 1 {
            let value = g.definition(EntityId::Value(actual_val))?;
            need(
                matches!(&value.operation, Operation::AggregateConstruct {aggregate, fields}
                if *aggregate == val && fields.len() == 1
                    && factory.entity(EntityId::Value(fields[0])).unwrap().ty == EntityType::Value(resource.unwrap())
                    && matches!(g.definition(EntityId::Value(fields[0])).unwrap().operation, Operation::HeapAllocate {owner, ..} if Some(owner) == resource)),
                "temporary form actual structural payload owns its Resource",
                value,
            )?;
        }
    }
    if case.representation == 3 {
        need(
            (some, none) == if empty { (0, 0) } else { (2, 1) },
            "temporary form real mixed nullable population",
            (some, none),
        )?;
        let reader = function(module, ast::function(sources, parsed, "read")?.span)?;
        let parameters = &reader
            .block(reader.entry_block().unwrap())
            .unwrap()
            .parameters;
        let checks = reader
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::NullableLoanIsNull { .. }))
            .collect::<Vec<_>>();
        need(
            matches!(checks.as_slice(), [check] if matches!(check.operation, Operation::NullableLoanIsNull {source} if EntityId::Loan(source) == parameters[1]))
                && !reader.instructions.iter().any(|i| {
                    matches!(
                        i.operation,
                        Operation::NullableTake { .. } | Operation::NullableWrap { .. }
                    )
                }),
            "temporary form reader tests original nullable loan without extracting an owner",
            checks,
        )?;
    }
    Ok((key, val))
}

fn strings(module: &Module, ty: SsaTypeId) -> usize {
    match module.type_kind(ty) {
        Some(SsaTypeKind::StringOwner) => 1,
        Some(SsaTypeKind::Aggregate { fields, .. }) => {
            fields.iter().map(|&field| strings(module, field)).sum()
        }
        _ => 0,
    }
}
