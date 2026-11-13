//! Real Map storage, original Resource deinit and actual populated/empty factory values.
mod factory {
    //! Original factory syntax and the complete straight-line owned Map construction chain.
    use super::*;
    use lang_frontend::{
        ast::{ExpressionId, StatementId},
        parser::{Expression, FunctionBody, FunctionForm, Statement},
    };

    struct Input {
        constructor: Span,
        kind: MapContainerKind,
        puts: Vec<Span>,
        consume: Option<Span>,
        returned: Span,
        operand: Span,
    }

    fn local(parsed: &ParsedFile, statement: StatementId) -> Option<(Span, ExpressionId)> {
        let Statement::LocalVariable { declaration } =
            parsed.ast().statements().get(statement).ok()?.payload()
        else {
            return None;
        };
        let Item::Variable {
            name: NameMarker::Present(name),
            initializer,
            ..
        } = parsed.ast().items().get(*declaration).ok()?.payload()
        else {
            return None;
        };
        Some((*name, *initializer))
    }

    fn source_call(
        parsed: &ParsedFile,
        id: ExpressionId,
    ) -> Result<(&Expression, &[lang_frontend::parser::CallArgument]), Failure> {
        let node = parsed.ast().expressions().get(id).unwrap();
        let Expression::Call {
            callee, arguments, ..
        } = node.payload()
        else {
            return Err(failure("temporary factory original Call", node));
        };
        Ok((
            parsed.ast().expressions().get(*callee).unwrap().payload(),
            arguments,
        ))
    }

    fn input(
        case: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
        f: &Function,
        empty: bool,
    ) -> Result<Input, Failure> {
        let node = one(
            parsed
                .roots()
                .iter()
                .filter_map(|id| parsed.ast().items().get(*id).ok())
                .filter(|node| f.origin == Origin::Source(node.span())),
            "temporary factory original AST identity",
        )?;
        let Item::Function {
            parameters, form, ..
        } = node.payload()
        else {
            return Err(failure("temporary factory actual original function", node));
        };
        need(
            parameters.is_empty(),
            "temporary factory original parameters are empty",
            parameters,
        )?;
        let body = match form {
            FunctionForm::ImplicitUnitBlock(body)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(body),
                ..
            } => *body,
            other => return Err(failure("temporary factory actual block", other)),
        };
        let Statement::Block { elements } = parsed.ast().statements().get(body).unwrap().payload()
        else {
            return Err(failure("temporary factory original statement list", body));
        };
        need(
            elements.iter().all(|id| {
                matches!(
                    parsed.ast().statements().get(*id).unwrap().payload(),
                    Statement::LocalVariable { .. } | Statement::Expression { .. }
                )
            }),
            "temporary factory source is explicitly straight-line",
            elements,
        )?;
        let (binding, constructor, kind) = one(
            elements.iter().filter_map(|&statement| {
                let (binding, initializer) = local(parsed, statement)?;
                let node = parsed.ast().expressions().get(initializer).ok()?;
                let Expression::Call {
                    callee, arguments, ..
                } = node.payload()
                else {
                    return None;
                };
                if !arguments.is_empty()
                    || !matches!(
                        parsed.ast().expressions().get(*callee).ok()?.payload(),
                        Expression::Name
                    )
                {
                    return None;
                }
                let kind = match sources.slice(ast::expression(parsed, *callee)).ok()? {
                    "mapOf" => MapContainerKind::Map,
                    "mutableMapOf" => MapContainerKind::MutableMap,
                    _ => return None,
                };
                Some((binding, initializer, kind))
            }),
            "temporary factory original local empty constructor",
        )?;
        need(
            kind == if empty && !case.mutable {
                MapContainerKind::Map
            } else {
                MapContainerKind::MutableMap
            },
            "temporary factory original constructor representation",
            kind,
        )?;
        let root = sources
            .slice(binding)
            .map_err(|e| failure("temporary factory actual root spelling", e))?;
        let mut puts = Vec::new();
        let mut consume = None;
        let mut return_binding = binding;
        let mut returned = None;
        for &statement in elements {
            let expression = match parsed.ast().statements().get(statement).unwrap().payload() {
                Statement::Expression { expression } => *expression,
                Statement::LocalVariable { .. } => {
                    let (name, initializer) = local(parsed, statement).unwrap();
                    if initializer == constructor {
                        continue;
                    }
                    let (callee, arguments) = source_call(parsed, initializer)?;
                    let Expression::Member {
                        receiver,
                        name_span,
                        safe: false,
                        ..
                    } = callee
                    else {
                        return Err(failure(
                            "temporary factory only original consume result binding",
                            callee,
                        ));
                    };
                    need(
                        sources.slice(*name_span) == Ok("consume")
                            && arguments.is_empty()
                            && sources.slice(ast::expression(parsed, *receiver)) == Ok(root)
                            && matches!(
                                parsed.ast().expressions().get(*receiver).unwrap().payload(),
                                Expression::Name
                            )
                            && consume
                                .replace(ast::expression(parsed, initializer))
                                .is_none(),
                        "temporary factory actual consume receives original mutable root",
                        initializer,
                    )?;
                    return_binding = name;
                    continue;
                }
                other => {
                    return Err(failure(
                        "temporary factory source remains straight-line",
                        other,
                    ));
                }
            };
            let node = parsed.ast().expressions().get(expression).unwrap();
            if let Expression::Return {
                value: Some(value), ..
            } = node.payload()
            {
                need(
                    returned
                        .replace((node.span(), ast::expression(parsed, *value)))
                        .is_none()
                        && matches!(
                            parsed.ast().expressions().get(*value).unwrap().payload(),
                            Expression::Name
                        )
                        && sources.slice(ast::expression(parsed, *value))
                            == sources.slice(return_binding)
                        && elements.last() == Some(&statement),
                    "temporary factory actual final Return operand names its current result",
                    node,
                )?;
                continue;
            }
            let (callee, arguments) = source_call(parsed, expression)?;
            match callee {
                Expression::Member {
                    receiver,
                    name_span,
                    safe: false,
                    ..
                } => {
                    need(
                        sources.slice(*name_span) == Ok("put")
                            && arguments.len() == 2
                            && matches!(
                                parsed.ast().expressions().get(*receiver).unwrap().payload(),
                                Expression::Name
                            )
                            && sources.slice(ast::expression(parsed, *receiver)) == Ok(root),
                        "temporary factory every put targets original local root",
                        node,
                    )?;
                    puts.push(node.span());
                }
                Expression::Name => {
                    let Expression::Call { callee, .. } = node.payload() else {
                        unreachable!()
                    };
                    need(
                        sources.slice(ast::expression(parsed, *callee)) == Ok("println")
                            && arguments.len() == 1,
                        "temporary factory no alternate source-level factory call",
                        node,
                    )?;
                }
                other => {
                    return Err(failure(
                        "temporary factory unexpected top-level source Call",
                        other,
                    ));
                }
            }
        }
        let (returned, operand) =
            returned.ok_or_else(|| failure("temporary factory actual Return expression", body))?;
        need(
            puts.len()
                == if empty {
                    0
                } else if case.representation == 3 {
                    3
                } else {
                    2
                }
                && consume.is_some() == (!empty && !case.mutable),
            "temporary factory actual AST population and optional conversion",
            (&puts, consume),
        )?;
        Ok(Input {
            constructor: ast::expression(parsed, constructor),
            kind,
            puts,
            consume,
            returned,
            operand,
        })
    }

    pub(super) fn check(
        case: &Case,
        module: &Module,
        sources: &SourceMap,
        parsed: &ParsedFile,
        returned_map: SsaTypeId,
        g: &Graph<'_>,
        empty: bool,
    ) -> Result<(), Failure> {
        let f = g.function;
        let original = input(case, sources, parsed, f, empty)?;
        let block = f.entry_block().unwrap();
        need(
            g.blocks.as_slice() == [block]
                && g.edges.is_empty()
                && g.block(block)?.parameters.is_empty()
                && f.instructions.len() == g.instructions.len(),
            "temporary factory entire reachable SSA is one actual straight-line block",
            f.id,
        )?;
        let construct = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| matches!(i.operation, Operation::MapConstruct { .. })),
            "temporary factory unique original MapConstruct",
        )?;
        let Operation::MapConstruct { map_type } = construct.operation else {
            unreachable!()
        };
        let (_, key, val) = module.map_container(returned_map).unwrap();
        need(
            construct.origin == Origin::Source(original.constructor)
                && module.map_container(map_type) == Some((original.kind, key, val))
                && f.entity(result(construct)?).unwrap().ty == EntityType::Value(map_type),
            "temporary factory actual source constructor supplies original represented storage",
            construct,
        )?;
        let mut current = value(construct)?;
        let mut previous = construct;
        let mut owners = BTreeSet::from([current.index()]);
        let mut uses = Uses::default();
        uses.protect(EntityId::Value(current));
        for origin in &original.puts {
            let put = one(
                g.instructions.iter().copied().filter(|i| {
                    i.origin == Origin::Source(*origin)
                        && matches!(i.operation, Operation::MapPut { .. })
                }),
                "temporary factory original put operation",
            )?;
            need(
                matches!(put.operation, Operation::MapPut {owner, ..} if owner == current)
                    && g.position(previous)? < g.position(put)?
                    && f.entity(result(put)?).unwrap().ty == EntityType::Value(map_type),
                "temporary factory put consumes exactly previous current owner and returns next owner",
                (previous, put),
            )?;
            uses.permit(EntityId::Value(current), put);
            current = value(put)?;
            need(
                owners.insert(current.index()),
                "temporary factory mutation creates distinct next SSA owner",
                put,
            )?;
            uses.protect(EntityId::Value(current));
            previous = put;
        }
        need(
            g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapPut { .. }))
                .count()
                == original.puts.len(),
            "temporary factory no unaccounted mutation on another owner",
            f.id,
        )?;
        if let Some(origin) = original.consume {
            let consume = one(
                g.instructions
                    .iter()
                    .copied()
                    .filter(|i| matches!(i.operation, Operation::MapConsume { .. })),
                "temporary factory original optional conversion",
            )?;
            need(
                consume.origin == Origin::Source(origin)
                    && consume.operation
                        == Operation::MapConsume {
                            owner: current,
                            map_type: returned_map,
                        }
                    && g.position(previous)? < g.position(consume)?,
                "temporary factory readonly result consumes exact last put owner",
                (previous, consume),
            )?;
            uses.permit(EntityId::Value(current), consume);
            current = value(consume)?;
            need(
                owners.insert(current.index())
                    && f.entity(EntityId::Value(current)).unwrap().ty
                        == EntityType::Value(returned_map),
                "temporary factory actual MapConsume output owns returned representation",
                consume,
            )?;
            uses.protect(EntityId::Value(current));
        } else {
            need(
                !g.instructions
                    .iter()
                    .any(|i| matches!(i.operation, Operation::MapConsume { .. }))
                    && map_type == returned_map,
                "temporary factory direct return retains actual empty/populated storage",
                f.id,
            )?;
        }
        let term = g.term(block)?;
        need(
            term.origin == Origin::Source(original.returned)
                && matches!(&term.kind, TerminatorKind::Return {values} if values.as_slice() == [current])
                && f.return_types.as_slice() == [returned_map],
            "temporary factory original Return span delivers exact current or consume result",
            (term, original.operand),
        )?;
        let actual = f
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
            actual == owners
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
            "temporary factory exact Map owner set, no hidden Map loan/place or alternate factory Call",
            (actual, owners),
        )?;
        uses.check(g, Some((block, current)))
    }
}
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
