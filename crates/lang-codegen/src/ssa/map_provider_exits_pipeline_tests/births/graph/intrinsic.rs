//! MapNew→optional Put→optional Consume supplies the actual current provider owner.
use super::*;
pub(super) fn check<'ir>(
    case: &Case,
    parsed: &ParsedFile,
    input: &ast::Input,
    module: &Module,
    g: &Graph<'ir>,
    uses: &mut Uses,
) -> Result<(EntityId, BTreeSet<EntityId>), Failure> {
    let construct = at(g, ast::span(parsed, input.construction.unwrap()), |o| {
        matches!(o, Operation::MapConstruct { .. })
    })?;
    let initial = result(construct)?;
    let Operation::MapConstruct { map_type: mutable } = construct.operation else {
        unreachable!()
    };
    let (kind, key, val) = module
        .map_container(mutable)
        .ok_or_else(|| failure("birth actual construction map storage", construct))?;
    need(
        kind == if case.mutable || case.kind == Kind::Consume {
            MapContainerKind::MutableMap
        } else {
            MapContainerKind::Map
        } && [key, val]
            .iter()
            .all(|ty| module.type_kind(*ty) == Some(&SsaTypeKind::StringOwner))
            && g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapConstruct { .. }))
                .count()
                == 1,
        "birth exact actual MapNew kind/StringKV and once",
        construct,
    )?;
    let mut current = initial;
    let mut previous = construct;
    let mut prior = BTreeSet::new();
    uses.protect(current);
    if let Some(put) = input.put {
        let put = at(g, ast::span(parsed, put), |o| {
            matches!(o, Operation::MapPut { .. })
        })?;
        let Operation::MapPut { owner, .. } = put.operation else {
            unreachable!()
        };
        need(
            EntityId::Value(owner) == current
                && put.block == previous.block
                && g.position(previous)? < g.position(put)?
                && g.function.entity(result(put)?).unwrap().ty
                    == g.function.entity(initial).unwrap().ty
                && g.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::MapPut { .. }))
                    .count()
                    == 1,
            "birth actual Put consumes current constructed owner and returns next owner",
            put,
        )?;
        uses.permit(current, put);
        prior.insert(current);
        current = result(put)?;
        uses.protect(current);
        previous = put;
    } else {
        need(
            !g.instructions
                .iter()
                .any(|i| matches!(i.operation, Operation::MapPut { .. })),
            "birth empty map has no fabricated put",
            construct,
        )?;
    }
    if case.kind == Kind::Consume {
        let consume = at(g, ast::span(parsed, input.birth), |o| {
            matches!(o, Operation::MapConsume { .. })
        })?;
        let Operation::MapConsume { owner, map_type } = consume.operation else {
            unreachable!()
        };
        need(
            EntityId::Value(owner) == current
                && module.map_container(map_type) == Some((MapContainerKind::Map, key, val))
                && consume.block == previous.block
                && g.position(previous)? < g.position(consume)?
                && g.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::MapConsume { .. }))
                    .count()
                    == 1,
            "birth actual consume transfers latest populated mutable owner",
            consume,
        )?;
        uses.permit(current, consume);
        prior.insert(current);
        current = result(consume)?;
        uses.protect(current);
    } else {
        need(
            !g.instructions
                .iter()
                .any(|i| matches!(i.operation, Operation::MapConsume { .. })),
            "birth mutable/new output has no substitute consume",
            input,
        )?;
    }
    need(
        g.instructions.iter().all(|i| {
            !matches!(
                i.operation,
                Operation::DirectCall { .. }
                    | Operation::CallableInvoke { .. }
                    | Operation::BorrowCall { .. }
            ) || i.origin
                == Origin::Source(
                    input
                        .site
                        .as_ref()
                        .map_or(input.after, |s| ast::span(parsed, s.read)),
                )
        }),
        "birth intrinsic owner is never supplied by an alternate factory",
        input.birth,
    )?;
    Ok((current, prior))
}
