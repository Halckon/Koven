//! Constructed storage consumes the actual owned payload and returns its current owner.
use super::*;
use lang_frontend::{
    ast::ExpressionId,
    parser::{Expression, Item, NameMarker},
    type_checking::ParameterMode,
};

fn original_call(
    sources: &SourceMap,
    parsed: &ParsedFile,
    scope: Span,
    wanted: &str,
    member: bool,
) -> Result<(ExpressionId, Span), Failure> {
    one(parsed.ast().expressions().iter().filter_map(|(id,node)|{
        let Expression::Call {callee,..}=node.payload()else{return None;};
        if !ast::inside(node.span(),scope){return None;}
        let target=parsed.ast().expressions().get(*callee).ok()?;
        let selected=if member{matches!(target.payload(),Expression::Member {name_span,..} if sources.slice(*name_span)==Ok(wanted))}
            else{matches!(target.payload(),Expression::Name)&&sources.slice(target.span())==Ok(wanted)};
        selected.then_some((id,node.span()))
    }),"delivery actual factory body Call")
}

pub(super) fn check(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    module: &Module,
    f: &Function,
    input: &ast::Input,
    map: SsaTypeId,
) -> Result<(), Failure> {
    let (constructor, constructor_span) =
        original_call(sources, parsed, input.factory.span, "mutableMapOf", false)?;
    let (put_ast, put_span) = original_call(sources, parsed, input.factory.span, "put", true)?;
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
                (ast::inside(node.span(), input.factory.span) && *actual == initializer)
                    .then_some(*name)
            }),
            "delivery actual factory local binding",
        )
    };
    let root_marker = local(constructor)?;
    let receiver = |call| -> Result<(), Failure> {
        let Expression::Call { callee, .. } =
            parsed.ast().expressions().get(call).unwrap().payload()
        else {
            unreachable!()
        };
        let Expression::Member { receiver, .. } =
            parsed.ast().expressions().get(*callee).unwrap().payload()
        else {
            return Err(failure("delivery factory original member Call", call));
        };
        need(
            matches!(
                parsed.ast().expressions().get(*receiver).unwrap().payload(),
                Expression::Name
            ) && sources.slice(ast::expression(parsed, *receiver)) == sources.slice(root_marker),
            "delivery put/consume receiver is actual constructed root",
            call,
        )
    };
    receiver(put_ast)?;
    let (consume_span, returned_marker) = if case.mutable {
        (None, root_marker)
    } else {
        let (id, span) = original_call(sources, parsed, input.factory.span, "consume", true)?;
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
            ast::inside(node.span(), input.factory.span).then_some((node.span(), *value))
        }),
        "delivery actual factory sole source Return",
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
        ) && sources.slice(ast::expression(parsed, returned.1)) == sources.slice(returned_marker),
        "delivery Return names actual current put/consume binding",
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
    let Operation::MapPut {
        owner,
        key,
        value: payload,
    } = put.operation
    else {
        unreachable!()
    };
    let (_, key_type, value_type) = module
        .map_container(map)
        .ok_or_else(|| failure("delivery factory result representation", map))?;
    let initial = value(construct)?;
    need(
        module.map_container(mutable) == Some((MapContainerKind::MutableMap, key_type, value_type))
            && owner == initial
            && construct.block == put.block
            && g.position(construct)? < g.position(put)?
            && f.entity(EntityId::Value(key)).unwrap().ty == EntityType::Value(key_type)
            && f.entity(EntityId::Value(payload)).unwrap().ty == EntityType::Value(value_type)
            && f.entity(result(construct)?).unwrap().ty == EntityType::Value(mutable)
            && f.entity(result(put)?).unwrap().ty == EntityType::Value(mutable)
            && g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapConstruct { .. }))
                .count()
                == 1
            && g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapPut { .. }))
                .count()
                == 1,
        "delivery actual MapConstruct to exact current MapPut and represented payload",
        (construct, put),
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
                && consume.block == put.block
                && g.position(put)? < g.position(consume)?
                && g.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::MapConsume { .. }))
                    .count()
                    == 1,
            "delivery readonly conversion consumes actual populated owner",
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
            "delivery mutable result is original current Put result",
            map,
        )?;
    }
    let formals = &g.block(f.entry_block().unwrap())?.parameters;
    let own_slots = input
        .factory
        .parameters
        .iter()
        .enumerate()
        .filter(|(_, p)| p.mode == ParameterMode::Value)
        .collect::<Vec<_>>();
    if let [(slot, parameter)] = own_slots.as_slice() {
        let parameter_entity = formals[*slot];
        anchored(&g, EntityId::Value(payload), &[parameter_entity])?;
        let Expression::Call { arguments, .. } =
            parsed.ast().expressions().get(put_ast).unwrap().payload()
        else {
            unreachable!()
        };
        need(
            arguments.len() == 2
                && matches!(
                    parsed
                        .ast()
                        .expressions()
                        .get(arguments[1].value)
                        .unwrap()
                        .payload(),
                    Expression::Name
                )
                && sources.slice(ast::expression(parsed, arguments[1].value))
                    == sources.slice(parameter.name)
                && f.entity(parameter_entity).unwrap().ty == EntityType::Value(value_type),
            "delivery Put source payload is actual own parameter, not borrowed argument",
            (put, parameter_entity),
        )?;
        // A Value parameter is consumed by this Put. Any separate Drop/read or
        // second consumer fails the complete operand-use check.
        let mut family = BTreeSet::from([parameter_entity]);
        loop {
            let before = family.len();
            for (_, edge) in &g.edges {
                for (&from, &to) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                    if family.contains(&from) {
                        family.insert(to);
                    }
                }
            }
            if before == family.len() {
                break;
            }
        }
        for entity in family {
            anchored(&g, entity, &[parameter_entity])?;
            uses.protect(entity);
        }
        uses.permit(EntityId::Value(payload), put);
    } else {
        need(
            own_slots.is_empty() && matches!(case.delivery, Delivery::SameMapBorrow),
            "delivery only actual Resource payload is Value parameter",
            own_slots,
        )?;
    }
    if matches!(case.delivery, Delivery::SameMapBorrow) {
        let size_ast=one(parsed.ast().expressions().iter().filter_map(|(_,n)|
            (ast::inside(n.span(),input.factory.span)&&matches!(n.payload(),Expression::Member {name_span,..} if sources.slice(*name_span)==Ok("size"))).then_some(n.span())),
            "delivery actual borrowed-input factory size")?;
        let size = at(&g, size_ast, |o| matches!(o, Operation::MapSize { .. }))?;
        let Operation::MapSize { owner: input_loan } = size.operation else {
            unreachable!()
        };
        let input_parameter = one(
            formals.iter().copied(),
            "delivery actual borrowed Map formal",
        )?;
        need(
            matches!(f.entity(input_parameter).unwrap().ty,EntityType::Loan {kind:LoanKind::Shared,target}
            if module.map_container(target).is_some_and(|(_,key,val)|key==key_type&&val==value_type)),
            "delivery same-K/V input remains Shared ABI rather than returned owning storage",
            input_parameter,
        )?;
        anchored(&g, input_loan, &[input_parameter])?;
        let mut family = BTreeSet::from([input_parameter]);
        loop {
            let before = family.len();
            for (_, edge) in &g.edges {
                for (&from, &to) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                    if family.contains(&from) {
                        family.insert(to);
                    }
                }
            }
            if before == family.len() {
                break;
            }
        }
        for entity in family {
            anchored(&g, entity, &[input_parameter])?;
            uses.protect(entity);
        }
        uses.permit(input_loan, size);
    }
    let normal = one(
        g.blocks
            .iter()
            .copied()
            .filter(|&b| matches!(g.term(b).unwrap().kind, TerminatorKind::Return { .. })),
        "delivery factory sole normal Return",
    )?;
    need(
        normal == construct.block
            && g.term(normal)?.origin == Origin::Source(returned.0)
            && matches!(&g.term(normal)?.kind,TerminatorKind::Return {values} if values.as_slice()==[current])
            && f.entity(EntityId::Value(current)).unwrap().ty == EntityType::Value(map),
        "delivery actual Return carries current construct/Put/consume chain",
        g.term(normal)?,
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
            && !g.instructions.iter().any(|i| {
                matches!(
                    i.operation,
                    Operation::DirectCall { .. } | Operation::BorrowCall { .. }
                )
            }),
        "delivery complete output owner chain has no alternative producer",
        (all, owners),
    )?;
    uses.check(&g, Some((normal, current)))
}
