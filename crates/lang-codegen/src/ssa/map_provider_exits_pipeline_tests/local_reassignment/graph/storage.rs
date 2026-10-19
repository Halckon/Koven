//! Actual returned map and payload ABI; empty producers are proved from SSA construction.
use super::*;
pub(super) fn factory(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    module: &Module,
    f: &Function,
) -> Result<(), Failure> {
    let name = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            matches!(n.payload(),Item::Function{..} if f.origin==Origin::Source(n.span()))
                .then_some(n)
        }),
        "original selected factory item",
    )?;
    let Item::Function {
        name: NameMarker::Present(name),
        ..
    } = name.payload()
    else {
        unreachable!()
    };
    let name = sources
        .slice(*name)
        .map_err(|e| failure("actual factory name", e))?;
    let empty = name.ends_with("Empty");
    let g = Graph::new(f)?;
    let constructs = g
        .instructions
        .iter()
        .copied()
        .filter(|i| matches!(i.operation, Operation::MapConstruct { .. }))
        .collect::<Vec<_>>();
    need(
        constructs.len() == 1,
        "actual factory starts one empty map",
        &constructs,
    )?;
    let puts = g
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::MapPut { .. }))
        .count();
    need(
        puts == if empty {
            0
        } else if case.payload == 2 {
            2
        } else {
            1
        },
        "factory real entry population count",
        (name, puts),
    )?;
    let heaps = g
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::HeapAllocate { .. }))
        .count();
    need(
        heaps == usize::from(!empty && case.payload != 0),
        "empty factory creates no Resource payload",
        (name, heaps),
    )?;
    if empty {
        let mut current = result(constructs[0])?;
        let consumes = g
            .instructions
            .iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::MapConsume { .. }))
            .collect::<Vec<_>>();
        need(
            consumes.len() == usize::from(!case.mutable),
            "empty Map conversion keeps same zero entries",
            &consumes,
        )?;
        if let Some(consume) = consumes.first() {
            need(
                matches!(consume.operation,Operation::MapConsume{owner,..} if EntityId::Value(owner)==current),
                "consume original empty MutableMap",
                consume,
            )?;
            current = result(consume)?;
        }
        let EntityId::Value(returned) = current else {
            return Err(failure("actual empty owner Value", current));
        };
        need(g.blocks.iter().filter(|&&b|matches!(&g.term(b).unwrap().kind,TerminatorKind::Return{values} if values.as_slice()==[returned])).count()==1,"empty factory returns exact zero-entry construction",f.id)?;
    }
    need(
        f.return_types.len() == 1 && module.map_container(f.return_types[0]).is_some(),
        "factory returns actual Map storage",
        f.id,
    )
}
pub(super) fn representation(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    module: &Module,
    map: SsaTypeId,
) -> Result<(), Failure> {
    let (kind, key, val) = module
        .map_container(map)
        .ok_or_else(|| failure("actual epoch map representation", map))?;
    need(
        kind == if case.mutable {
            MapContainerKind::MutableMap
        } else {
            MapContainerKind::Map
        } && module.type_kind(key) == Some(&SsaTypeKind::StringOwner),
        "actual epoch container/String key storage",
        (kind, key),
    )?;
    if case.payload == 0 {
        return need(
            module.type_kind(val) == Some(&SsaTypeKind::StringOwner),
            "Pure payload remains String owner",
            val,
        );
    }
    let resource = if case.payload == 2 {
        let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(val) else {
            return Err(failure("nullable keeps real handle ABI", val));
        };
        *inner
    } else {
        val
    };
    let Some(SsaTypeKind::HeapOwner {
        payload: Some(payload),
        ..
    }) = module.type_kind(resource)
    else {
        return Err(failure("actual Resource heap payload", resource));
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
        "Resource still owns its String tag",
        resource,
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
        "original Resource declaration",
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
        "actual Resource representation selects original deinit",
        resource,
    )
}
