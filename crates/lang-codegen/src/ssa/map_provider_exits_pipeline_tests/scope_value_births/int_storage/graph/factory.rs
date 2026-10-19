//! makeInt really constructs, inserts the two scalar payloads, consumes, and returns that owner.
use super::*;
pub(super) fn check(
    p: &ParsedFile,
    i: &ast::Input,
    module: &Module,
    map: SsaTypeId,
) -> Result<(), Failure> {
    let f = function(module, i.make)?;
    let g = Graph::new(f)?;
    let construct = at(&g, ast::span(p, i.construction), |op| {
        matches!(op, Operation::MapConstruct { .. })
    })?;
    let put = at(&g, ast::span(p, i.put), |op| {
        matches!(op, Operation::MapPut { .. })
    })?;
    let consume = at(&g, ast::span(p, i.consume), |op| {
        matches!(op, Operation::MapConsume { .. })
    })?;
    let Operation::MapConstruct { map_type: mutable } = construct.operation else {
        unreachable!()
    };
    let Operation::MapPut {
        owner: initial,
        key,
        value: payload,
    } = put.operation
    else {
        unreachable!()
    };
    let (_, kt, vt) = module.map_container(map).unwrap();
    need(
        module.map_container(mutable) == Some((MapContainerKind::MutableMap, kt, vt))
            && initial == value(construct)?
            && consume.operation
                == Operation::MapConsume {
                    owner: value(put)?,
                    map_type: map,
                }
            && f.entity(result(construct)?).unwrap().ty == EntityType::Value(mutable)
            && f.entity(result(put)?).unwrap().ty == EntityType::Value(mutable)
            && f.entity(result(consume)?).unwrap().ty == EntityType::Value(map)
            && g.blocks.len() == 1
            && g.edges.is_empty()
            && g.position(construct)? < g.position(put)?
            && g.position(put)? < g.position(consume)?
            && g.term(consume.block)?.origin == Origin::Source(ast::span(p, i.returned))
            && matches!(&g.term(consume.block)?.kind,TerminatorKind::Return{values}if values.as_slice()==[value(consume)?]),
        "Int actual original construction/updated owner/consume/current Return chain",
        (construct, put, consume),
    )?;
    let mut uses = Uses::default();
    for e in [result(construct)?, result(put)?, result(consume)?] {
        uses.protect(e);
    }
    uses.permit(result(construct)?, put);
    uses.permit(result(put)?, consume);
    for ((id, expression), expected) in [(key, i.payload[0]), (payload, i.payload[1])]
        .into_iter()
        .zip([1, 2])
    {
        let literal = g.definition(EntityId::Value(id))?;
        need(
            literal.operation == Operation::Constant(ScalarConstant::Integer(expected))
                && literal.origin == Origin::Source(ast::span(p, expression))
                && f.entity(EntityId::Value(id)).unwrap().ty == EntityType::Value(kt),
            "Int actual exact put scalar bytes/value/type",
            literal,
        )?;
        uses.protect(EntityId::Value(id));
        uses.permit(EntityId::Value(id), put);
    }
    for (index, data) in f.values.iter().enumerate() {
        if module.map_container(data.ty.semantic_type()).is_some() {
            need(
                [result(construct)?, result(put)?, result(consume)?].contains(&EntityId::Value(
                    ValueId {
                        function: f.id,
                        index,
                    },
                )),
                "Int all makeInt Map Values are this actual owner evolution",
                (index, data),
            )?;
        }
    }
    need(g.instructions.iter().filter(|op|matches!(op.operation,Operation::MapConstruct{..})).count()==1&&g.instructions.iter().filter(|op|matches!(op.operation,Operation::MapPut{..})).count()==1&&g.instructions.iter().filter(|op|matches!(op.operation,Operation::MapConsume{..})).count()==1&&!g.instructions.iter().any(|op|matches!(op.operation,Operation::Drop{owner}if module.map_container(f.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())),"Int no stale owner cleanup or substitute factory construction",f.id)?;
    uses.check(&g, Some((consume.block, value(consume)?)))
}
