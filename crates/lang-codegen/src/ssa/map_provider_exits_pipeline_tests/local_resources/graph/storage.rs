//! Representation is queried from the actual factory result, not from source type spelling.
use super::*;

pub(super) fn check(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    module: &Module,
    map: SsaTypeId,
) -> Result<(SsaTypeId, SsaTypeId), Failure> {
    let (kind, key, value) = module
        .map_container(map)
        .ok_or_else(|| failure("actual Resource Map storage", map))?;
    need(
        kind == if case.mutable {
            MapContainerKind::MutableMap
        } else {
            MapContainerKind::Map
        } && module.type_kind(key) == Some(&SsaTypeKind::StringOwner),
        "actual Map kind and String key",
        (kind, key),
    )?;
    let resource = match (case.nullable, module.type_kind(value)) {
        (true, Some(SsaTypeKind::NullableHandle { inner })) => *inner,
        (false, Some(SsaTypeKind::HeapOwner { .. })) => value,
        _ => {
            return Err(failure(
                "nullable keeps existing handle representation",
                module.type_kind(value),
            ));
        }
    };
    let Some(SsaTypeKind::HeapOwner {
        payload: Some(payload),
        ..
    }) = module.type_kind(resource)
    else {
        return Err(failure("actual Resource owns payload", resource));
    };
    fn strings(m: &Module, t: SsaTypeId) -> usize {
        match m.type_kind(t) {
            Some(SsaTypeKind::StringOwner) => 1,
            Some(SsaTypeKind::Aggregate { fields, .. }) => {
                fields.iter().map(|&t| strings(m, t)).sum()
            }
            _ => 0,
        }
    }
    need(
        strings(module, *payload) == 1,
        "Resource retains its owned String field",
        module.type_kind(*payload),
    )?;
    let class = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            let Item::Classifier(c) = n.payload() else {
                return None;
            };
            matches!(c.name,NameMarker::Present(name) if sources.slice(name)==Ok("Resource"))
                .then_some(c)
        }),
        "original Resource class",
    )?;
    let deinit = one(
        class
            .body
            .as_ref()
            .unwrap()
            .members
            .iter()
            .filter_map(|id| {
                let n = parsed.ast().items().get(*id).ok()?;
                matches!(n.payload(), Item::Deinit { .. }).then_some(n.span())
            }),
        "original Resource deinit",
    )?;
    need(
        module
            .deinits
            .get(&resource)
            .and_then(|id| module.function(*id))
            .is_some_and(|f| f.origin == Origin::Source(deinit)),
        "actual nullable/direct Resource storage maps to original deinit",
        resource,
    )?;
    Ok((key, value))
}
