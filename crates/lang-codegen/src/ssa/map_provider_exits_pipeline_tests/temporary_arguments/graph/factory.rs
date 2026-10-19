//! The selected Int factory's successful path returns its actual populated owner chain.
use super::*;
use lang_frontend::parser::{Expression, Item, NameMarker};

fn original_call(
    sources: &SourceMap,
    parsed: &ParsedFile,
    scope: Span,
    wanted: &str,
    member: bool,
) -> Result<(lang_frontend::ast::ExpressionId, Span), Failure> {
    one(parsed.ast().expressions().iter().filter_map(|(id,node)|{
        let Expression::Call {callee,..}=node.payload() else{return None;};
        if !ast::inside(node.span(),scope){return None;}
        let callee_node=parsed.ast().expressions().get(*callee).ok()?;
        let matches=if member {
            matches!(callee_node.payload(),Expression::Member {name_span,..} if sources.slice(*name_span)==Ok(wanted))
        }else{matches!(callee_node.payload(),Expression::Name)&&sources.slice(callee_node.span())==Ok(wanted)};
        matches.then_some((id,node.span()))
    }),"argument factory original selected body Call")
}

pub(in crate::ssa::map_provider_exits_pipeline_tests) fn check(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    module: &Module,
    f: &Function,
    map: SsaTypeId,
) -> Result<(), Failure> {
    let scope = f.origin.span();
    let (constructor_id, constructor_span) =
        original_call(sources, parsed, scope, "mutableMapOf", false)?;
    let (put_id, put_span) = original_call(sources, parsed, scope, "put", true)?;
    let local = |initializer| {
        one(
            parsed.ast().items().iter().filter_map(|(_, node)| {
                let Item::Variable {
                    name: NameMarker::Present(name),
                    initializer: actual,
                    ..
                } = node.payload()
                else {
                    return None;
                };
                (ast::inside(node.span(), scope) && *actual == initializer).then_some(*name)
            }),
            "argument factory actual local binding",
        )
    };
    let root_marker = local(constructor_id)?;
    let root = sources
        .slice(root_marker)
        .map_err(|e| failure("argument factory actual root", e))?;
    let receiver = |id| -> Result<(), Failure> {
        let Expression::Call { callee, .. } = parsed.ast().expressions().get(id).unwrap().payload()
        else {
            return Err(failure("argument factory source member Call", id));
        };
        let Expression::Member { receiver, .. } =
            parsed.ast().expressions().get(*callee).unwrap().payload()
        else {
            return Err(failure("argument factory source receiver", id));
        };
        need(
            matches!(
                parsed.ast().expressions().get(*receiver).unwrap().payload(),
                Expression::Name
            ) && sources.slice(ast::span(parsed, *receiver)) == Ok(root),
            "argument factory put/consume use original constructed root",
            id,
        )
    };
    receiver(put_id)?;
    let (consume_span, returned_marker) = if case.mutable {
        (None, root_marker)
    } else {
        let (id, span) = original_call(sources, parsed, scope, "consume", true)?;
        receiver(id)?;
        (Some(span), local(id)?)
    };
    let returned = one(
        parsed.ast().expressions().iter().filter_map(|(_, node)| {
            let Expression::Return {
                value: Some(value), ..
            } = node.payload()
            else {
                return None;
            };
            ast::inside(node.span(), scope).then_some((node.span(), *value))
        }),
        "argument selected factory original Return",
    )?;
    need(
        matches!(
            parsed
                .ast()
                .expressions()
                .get(returned.1)
                .unwrap()
                .payload(),
            Expression::Name
        ) && sources.slice(ast::span(parsed, returned.1)) == sources.slice(returned_marker),
        "argument factory actual Return operand names current map result",
        returned,
    )?;
    let g = Graph::with_abort(f)?;
    let construct = at(&g, constructor_span, |o| {
        matches!(o, Operation::MapConstruct { .. })
    })?;
    let put = at(&g, put_span, |o| matches!(o, Operation::MapPut { .. }))?;
    let Operation::MapConstruct { map_type: mutable } = construct.operation else {
        unreachable!()
    };
    let (_, key, val) = module
        .map_container(map)
        .ok_or_else(|| failure("argument factory returned Map representation", map))?;
    need(
        module.map_container(mutable) == Some((MapContainerKind::MutableMap, key, val))
            && g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapConstruct { .. }))
                .count()
                == 1
            && g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapPut { .. }))
                .count()
                == 1
            && construct.block == put.block
            && g.position(construct)? < g.position(put)?,
        "argument selected factory creates and mutates only original storage",
        (construct, put),
    )?;
    let initial = value(construct)?;
    let Operation::MapPut { owner, .. } = put.operation else {
        unreachable!()
    };
    need(
        owner == initial
            && f.entity(result(construct)?).unwrap().ty == EntityType::Value(mutable)
            && f.entity(result(put)?).unwrap().ty == EntityType::Value(mutable),
        "argument actual put consumes constructed owner and returns next owner",
        put,
    )?;
    let mut uses = Uses::default();
    uses.protect(EntityId::Value(initial));
    uses.permit(EntityId::Value(initial), put);
    let mut current = value(put)?;
    uses.protect(EntityId::Value(current));
    let mut owners = BTreeSet::from([initial.index(), current.index()]);
    if let Some(span) = consume_span {
        let consume = at(&g, span, |o| matches!(o, Operation::MapConsume { .. }))?;
        need(
            consume.operation
                == Operation::MapConsume {
                    owner: current,
                    map_type: map,
                }
                && consume.block == construct.block
                && g.position(put)? < g.position(consume)?
                && g.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::MapConsume { .. }))
                    .count()
                    == 1,
            "argument actual readonly result consumes current populated owner",
            consume,
        )?;
        uses.permit(EntityId::Value(current), consume);
        current = value(consume)?;
        owners.insert(current.index());
        uses.protect(EntityId::Value(current));
    } else {
        need(
            mutable == map
                && !g
                    .instructions
                    .iter()
                    .any(|i| matches!(i.operation, Operation::MapConsume { .. })),
            "argument mutable factory returns its actual current put owner",
            f.id,
        )?;
    }
    let block = construct.block;
    need(
        matches!(&g.term(block)?.kind,TerminatorKind::Return {values} if values.as_slice()==[current])
            && g.term(block)?.origin == Origin::Source(returned.0)
            && f.entity(EntityId::Value(current)).unwrap().ty == EntityType::Value(map)
            && g.blocks
                .iter()
                .filter(|&&block| {
                    matches!(g.term(block).unwrap().kind, TerminatorKind::Return { .. })
                })
                .count()
                == 1,
        "argument actual sole normal Return carries current put/consume owner",
        (g.term(block)?, returned),
    )?;
    let all = f
        .values
        .iter()
        .enumerate()
        .filter_map(|(index, data)| {
            module
                .map_container(data.ty.semantic_type())
                .is_some()
                .then_some(index)
        })
        .collect::<BTreeSet<_>>();
    need(
        all == owners
            && f.places
                .iter()
                .chain(&f.loans)
                .all(|data| module.map_container(data.ty.semantic_type()).is_none())
            && !g.instructions.iter().any(|i| {
                matches!(
                    i.operation,
                    Operation::DirectCall { .. } | Operation::BorrowCall { .. }
                )
            }),
        "argument factory complete owned map chain and no alternative map producer/place/loan",
        (all, owners),
    )?;
    uses.check(&g, Some((block, current)))
}
