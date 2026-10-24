//! 同轮 Unit facts 与真实 SSA 承接旧 owner phi 的可用性、回边和退役合同。
//! 不构造旧 FE 表，不调用生产 qualification，不以同类型实体推定同根。
use super::super::fixture::{Fixture, loop_sites};
use super::{Report, ast};
use crate::ssa::{
    map_provider_exits_pipeline_tests::OrdinaryGraph as Graph,
    model::{
        Definition, Edge, EntityId, EntityType, FunctionId, Instruction, LoanKind,
        MapContainerKind, Module, Operation, Origin, Ownership, Program, SsaTypeKind,
        TerminatorKind,
    },
};
use lang_frontend::{
    name_resolution::{DeclarationId, SymbolKind, ValidatedCompilationUnitNames},
    ownership_checking::{CompilationUnitOwnership, UnitDropPoint, UnitDropTarget, UnitLoanTarget},
    parser::Expression,
    source::Span,
    type_checking::{CompilationUnitTypes, MapIterationKind, ParameterMode, UnitStatementId},
};
use std::collections::BTreeSet;

pub(super) struct Frontend<'a> {
    pub fixture: &'a Fixture,
    pub names: &'a ValidatedCompilationUnitNames,
    pub typed: &'a CompilationUnitTypes,
    pub owned: &'a CompilationUnitOwnership,
}

pub(super) fn collect(
    frontend: Frontend<'_>,
    declaration: DeclarationId,
    program: &Program,
    entry: FunctionId,
    report: &mut Report,
) {
    let result = inspect_root(&frontend, declaration, program, entry);
    report.check(
        "Group same-round real-root owner/caller Borrow SSA contract",
        result.is_ok(),
        (declaration, entry, result),
    );
}

fn debug(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}

fn need(ok: bool, label: &str, detail: impl std::fmt::Debug) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(format!("{label}: {detail:?}"))
    }
}

fn one<T>(mut items: impl Iterator<Item = T>, label: &str) -> Result<T, String> {
    let item = items.next().ok_or_else(|| format!("{label}: missing"))?;
    need(items.next().is_none(), label, "not unique")?;
    Ok(item)
}

fn within(inner: Span, outer: Span) -> bool {
    inner.source_id() == outer.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

struct Source {
    function: Span,
    loop_span: Span,
    parameter: Span,
    kind: MapIterationKind,
    owned: bool,
    after: Option<Span>,
}

fn inspect_root(
    input: &Frontend<'_>,
    declaration: DeclarationId,
    program: &Program,
    entry: FunctionId,
) -> Result<(), String> {
    let declared = one(
        input
            .names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|d| d.id() == declaration),
        "same-round requested declaration",
    )?;
    need(
        declared.kind() == SymbolKind::Function,
        "real function root",
        declared,
    )?;
    let source = one(
        input
            .names
            .names()
            .index()
            .source_units()
            .iter()
            .filter(|s| s.id() == declared.source_unit()),
        "requested declaration source unit",
    )?;
    let file = one(
        input
            .fixture
            .files
            .iter()
            .filter(|f| f.parsed.source_id() == source.source_id()),
        "original parsed source",
    )?;
    let scope = file
        .parsed
        .ast()
        .items()
        .get(declared.root())
        .map_err(debug)?
        .span();
    let module = program
        .module(entry.module())
        .ok_or("returned entry module missing")?;
    let function = module
        .function(entry)
        .ok_or("returned entry function missing")?;
    need(
        function.origin == Origin::Source(scope)
            && program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .filter(|f| f.origin == Origin::Source(scope))
                .count()
                == 1,
        "returned ID is the exact requested AST function",
        (declaration, entry, scope, &function.origin),
    )?;
    need(
        input.owned.captures().is_empty()
            && input.owned.closures().is_empty()
            && function.instructions.iter().all(|i| {
                !matches!(
                    i.operation,
                    Operation::ClosureConstruct { .. } | Operation::CallableInvoke { .. }
                )
            }),
        "same-round no capture/environment and no hidden SSA closure",
        (input.owned.captures(), input.owned.closures()),
    )?;
    let graph = Graph::new(function).map_err(debug)?;
    let sites = loop_sites(file)
        .map_err(debug)?
        .into_iter()
        .filter(|site| within(site.span, scope))
        .collect::<Vec<_>>();
    if sites.is_empty() {
        need(
            ast::empty_entry(file, declared.name_span())
                && graph.blocks.len() == 1
                && graph.edges.is_empty()
                && graph.instructions.is_empty()
                && function.blocks.len() == 1
                && function.instructions.is_empty()
                && function.values.is_empty()
                && function.loans.is_empty()
                && function.places.is_empty()
                && function.return_types.is_empty()
                && function.receiver().is_none()
                && function.borrow_return.is_none()
                && function.carrier_return.is_none()
                && matches!(&graph.term(graph.blocks[0]).map_err(debug)?.kind,
                    TerminatorKind::Return { values } if values.is_empty()),
            "original empty entry remains empty, not a proxy for the four Map roots",
            entry,
        )?;
        return Ok(());
    }
    let site = one(sites.iter(), "one original For in requested root")?;
    let statement = UnitStatementId::new(source.id(), site.statement);
    let plan = input
        .owned
        .map_iteration(statement)
        .ok_or("same-round Map plan missing")?;
    let descriptor = input
        .typed
        .map_iteration(statement)
        .ok_or("same-round Map descriptor missing")?;
    need(
        plan.descriptor() == descriptor,
        "same-round descriptor identity",
        statement,
    )?;
    let Some(UnitLoanTarget::Place(place)) = plan.source() else {
        return Err("real parameter source place missing".into());
    };
    let root = place.root();
    let signature = input
        .typed
        .signatures()
        .declaration(declaration)
        .and_then(|d| d.callable())
        .ok_or("requested callable signature missing")?;
    let parameter = one(signature.parameters().iter(), "one original parameter")?;
    let resolution = one(
        input
            .names
            .names()
            .source_units()
            .iter()
            .filter(|s| s.source_unit() == source.id()),
        "same source names",
    )?;
    let symbol = one(
        resolution
            .resolution()
            .symbols()
            .iter()
            .filter(|s| s.id() == root.symbol()),
        "exact source parameter symbol",
    )?;
    need(
        place.is_root()
            && root.source_unit() == source.id()
            && signature.receiver().is_none()
            && parameter.symbol() == Some(root)
            && parameter.ty() == descriptor.source_type()
            && symbol.kind() == SymbolKind::ValueParameter
            && !symbol.is_synthetic()
            && within(symbol.span(), scope),
        "source plan is anchored to the selected callable's real parameter slot",
        (root, parameter, symbol),
    )?;
    let owned = parameter.mode() == ParameterMode::Value;
    need(
        owned || parameter.mode() == ParameterMode::Borrow,
        "original parameter mode",
        parameter,
    )?;
    let drops = input
        .owned
        .drops()
        .iter()
        .filter(|d| d.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    need(
        drops.len() == usize::from(owned)
            && drops.iter().all(|d| {
                d.point() == UnitDropPoint::LoopExit(statement) && d.value_origin() == symbol.span()
            }),
        "exact same-round Named owner drop, no Borrow owner fact",
        &drops,
    )?;
    let after = file
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            (within(node.span(), scope)
                && node.span().start() > site.span.end()
                && matches!(node.payload(), Expression::Call { .. }))
            .then_some(node.span())
        })
        .collect::<Vec<_>>();
    need(
        after.len() == usize::from(owned)
            && after
                .iter()
                .all(|span| input.fixture.sources.slice(*span) == Ok("println(\"exhausted\")")),
        "original own source retains its observable post-loop call",
        &after,
    )?;
    let source = Source {
        function: scope,
        loop_span: site.span,
        parameter: symbol.span(),
        kind: descriptor.kind(),
        owned,
        after: after.first().copied(),
    };
    inspect_provider(&graph, module, &source)
}

fn result(instruction: &Instruction) -> Result<EntityId, String> {
    one(instruction.results.iter().copied(), "one provider result")
}

fn columns(
    graph: &Graph<'_>,
    edge: &Edge,
    expected: &[(EntityId, EntityId); 3],
) -> Result<(), String> {
    let target = graph.block(edge.target).map_err(debug)?;
    need(
        edge.arguments.len() == 3
            && target.parameters.len() == 3
            && expected
                .iter()
                .map(|(a, _)| a)
                .collect::<BTreeSet<_>>()
                .len()
                == 3
            && expected
                .iter()
                .map(|(_, b)| b)
                .collect::<BTreeSet<_>>()
                .len()
                == 3
            && edge
                .arguments
                .iter()
                .zip(&target.parameters)
                .all(|(&a, &b)| a != b && expected.contains(&(a, b))),
        "every edge has exactly the proven owner/source/cursor columns",
        (edge, expected),
    )
}

fn inspect_provider(graph: &Graph<'_>, module: &Module, source: &Source) -> Result<(), String> {
    let f = graph.function;
    let op = |accepts: fn(&Operation) -> bool| {
        one(
            graph
                .instructions
                .iter()
                .copied()
                .filter(|i| accepts(&i.operation)),
            "one actual provider operation",
        )
    };
    let first = op(|o| matches!(o, Operation::MapIterationFirst { .. }))?;
    let guard = op(|o| matches!(o, Operation::MapIterationHasNext { .. }))?;
    let key = op(|o| matches!(o, Operation::MapIterationKeyLoan { .. }))?;
    let value = op(|o| matches!(o, Operation::MapIterationValueLoan { .. }))?;
    let next = op(|o| matches!(o, Operation::MapIterationNext { .. }))?;
    let Operation::MapIterationFirst { source: lease } = first.operation else {
        return Err("First operation changed".into());
    };
    let Operation::MapIterationHasNext {
        source: hs,
        cursor: hc,
    } = guard.operation
    else {
        return Err("HasNext operation changed".into());
    };
    let Operation::MapIterationKeyLoan {
        source: bs,
        cursor: bc,
    } = key.operation
    else {
        return Err("KeyLoan operation changed".into());
    };
    need(
        value.operation
            == Operation::MapIterationValueLoan {
                source: bs,
                cursor: bc,
            }
            && next.operation
                == Operation::MapIterationNext {
                    source: bs,
                    cursor: bc,
                }
            && [first, guard, key, value, next]
                .iter()
                .all(|i| i.origin == Origin::Source(source.loop_span)),
        "provider uses the original For and exact current source/cursor",
        [first, guard, key, value, next],
    )?;
    let (pre, header, body) = (first.block, guard.block, key.block);
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &graph.term(header).map_err(debug)?.kind
    else {
        return Err("provider header is not its real conditional".into());
    };
    let exit = when_false.target;
    let roles = [pre, header, body, exit];
    need(
        roles.into_iter().collect::<BTreeSet<_>>().len() == 4
            && graph.blocks.iter().copied().collect::<BTreeSet<_>>()
                == roles.into_iter().collect::<BTreeSet<_>>()
            && graph.edges.len() == 4
            && f.entry_block() == Some(pre)
            && result(guard)? == EntityId::Value(*condition)
            && when_true.target == body
            && value.block == body
            && next.block == body,
        "closed normal CFG: entry, header, body, exhaustion",
        (&graph.blocks, &graph.edges),
    )?;
    for (target, predecessors) in [
        (pre, vec![]),
        (header, vec![pre, body]),
        (body, vec![header]),
        (exit, vec![header]),
    ] {
        graph.incoming_from(target, &predecessors).map_err(debug)?;
    }
    let entry = graph.branch(pre).map_err(debug)?;
    let back = graph.branch(body).map_err(debug)?;
    need(
        entry.target == header && back.target == header,
        "entry and natural backedge",
        (entry, back),
    )?;
    let seed = one(
        graph.block(pre).map_err(debug)?.parameters.iter().copied(),
        "actual source parameter",
    )?;
    let header_seed = graph.transported(entry, seed).map_err(debug)?;
    let body_seed = graph.transported(when_true, header_seed).map_err(debug)?;
    let exit_seed = graph.transported(when_false, header_seed).map_err(debug)?;
    let exit_source = graph
        .transported(when_false, EntityId::Loan(hs))
        .map_err(debug)?;
    let exit_cursor = graph
        .transported(when_false, EntityId::Value(hc))
        .map_err(debug)?;
    let seeds = [seed, header_seed, body_seed, exit_seed];
    let leases = [
        EntityId::Loan(lease),
        EntityId::Loan(hs),
        EntityId::Loan(bs),
        exit_source,
    ];
    let cursors = [
        result(first)?,
        EntityId::Value(hc),
        EntityId::Value(bc),
        result(next)?,
        exit_cursor,
    ];
    columns(
        graph,
        entry,
        &[
            (seed, header_seed),
            (leases[0], leases[1]),
            (cursors[0], cursors[1]),
        ],
    )?;
    columns(
        graph,
        when_true,
        &[
            (header_seed, body_seed),
            (leases[1], leases[2]),
            (cursors[1], cursors[2]),
        ],
    )?;
    columns(
        graph,
        back,
        &[
            (body_seed, header_seed),
            (leases[2], leases[1]),
            (cursors[3], cursors[1]),
        ],
    )?;
    columns(
        graph,
        when_false,
        &[
            (header_seed, exit_seed),
            (leases[1], leases[3]),
            (cursors[1], cursors[4]),
        ],
    )?;
    for entity in &seeds {
        graph.anchored(*entity, &[seed]).map_err(debug)?;
    }
    let seed_type = f.entity(seed).ok_or("source parameter entity missing")?.ty;
    let map_type = seed_type.semantic_type();
    let (kind, kt, vt) = module
        .map_container(map_type)
        .ok_or("source is not represented Map")?;
    need(
        kind == if source.kind == MapIterationKind::Map {
            MapContainerKind::Map
        } else {
            MapContainerKind::MutableMap
        } && module.type_kind(kt)
            == Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            })
            && module.type_kind(vt) == Some(&SsaTypeKind::StringOwner)
            && module.type_ownership(map_type) == Some(Ownership::MoveOnly)
            && seed_type
                == if source.owned {
                    EntityType::Value(map_type)
                } else {
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: map_type,
                    }
                }
            && seeds.iter().enumerate().all(|(index, &entity)| {
                matches!(
                    (entity, source.owned),
                    (EntityId::Value(_), true) | (EntityId::Loan(_), false)
                ) && f.entity(entity).is_some_and(|d| {
                    d.ty == seed_type
                        && d.origin
                            == Origin::Source(if index == 0 {
                                source.function
                            } else {
                                source.loop_span
                            })
                })
            }),
        "nonoptional exact source identities on every real incoming (Always)",
        (seed_type, seeds),
    )?;
    need(
        leases.iter().all(|&e| {
            f.entity(e).is_some_and(|d| {
                d.ty == EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: map_type,
                } && d.origin == Origin::Source(source.loop_span)
            })
        }) && cursors.iter().all(|&e| {
            f.entity(e).is_some_and(|d| {
                matches!(d.ty,
                EntityType::Value(ty) if module.type_kind(ty)
                    == Some(&SsaTypeKind::Integer { bits: 64, signed: false }))
            })
        }) && f.entity(result(guard)?).is_some_and(|d| {
            matches!(d.ty,
                EntityType::Value(ty) if module.type_kind(ty) == Some(&SsaTypeKind::Boolean))
        }),
        "provider Shared leases, u64 cursors and Boolean guard",
        (leases, cursors),
    )?;
    let birth = graph.definition(leases[0]).map_err(debug)?;
    need(
        birth.block == pre
            && graph.position(birth).map_err(debug)? < graph.position(first).map_err(debug)?,
        "source lease precedes First in actual entry",
        birth,
    )?;
    let root = match (source.owned, seed, &birth.operation) {
        (
            true,
            EntityId::Value(owner),
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
        ) => {
            let root = graph.definition(EntityId::Place(*place)).map_err(debug)?;
            need(
                root.operation == Operation::RootPlace { owner }
                    && root.block == pre
                    && root.origin == Origin::Source(source.loop_span)
                    && graph.position(root).map_err(debug)?
                        < graph.position(birth).map_err(debug)?,
                "owned provider lease roots exact original parameter",
                root,
            )?;
            Some(root)
        }
        (false, EntityId::Loan(caller), Operation::SharedReborrow { source: actual }) => {
            need(
                *actual == caller && lease != caller,
                "provider does not consume caller Borrow",
                birth,
            )?;
            None
        }
        _ => return Err(format!("wrong original source lease birth: {birth:?}")),
    };
    need(
        birth.origin == Origin::Source(source.loop_span),
        "actual source lease origin",
        birth,
    )?;
    components(graph, [key, value, next], [kt, vt])?;
    let end = one(graph.ops(exit).map_err(debug)?.into_iter().filter(|i|
        matches!(i.operation, Operation::BorrowEnd { loan } if EntityId::Loan(loan) == exit_source)),
        "one exhaustion EndSource")?;
    need(
        end.origin == Origin::Source(source.loop_span) && graph.position(end).map_err(debug)? == 0,
        "zero-round exhaustion begins EndSource, without component projection",
        end,
    )?;
    let map_drops = graph
        .instructions
        .iter()
        .copied()
        .filter(|i| {
            matches!(i.operation,
        Operation::Drop { owner } if seeds.contains(&EntityId::Value(owner)))
        })
        .collect::<Vec<_>>();
    need(
        map_drops.len() == usize::from(source.owned),
        "exact source drop cardinality",
        &map_drops,
    )?;
    let drop = map_drops.first().copied();
    if let Some(drop) = drop {
        need(
            matches!(drop.operation, Operation::Drop { owner } if EntityId::Value(owner) == exit_seed)
                && drop.block == exit
                && drop.origin == Origin::Source(source.parameter)
                && graph.position(drop).map_err(debug)? == graph.position(end).map_err(debug)? + 1,
            "same-round Named Drop consumes current exhausted owner immediately after EndSource",
            drop,
        )?;
        let after_span = source.after.ok_or("original exhaustion print missing")?;
        let after = one(
            graph.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(after_span)
                    && matches!(i.operation, Operation::PrintString { .. })
            }),
            "original observable exhaustion print",
        )?;
        need(
            after.block == exit
                && graph.position(drop).map_err(debug)? < graph.position(after).map_err(debug)?,
            "owner retirement precedes original post-loop println",
            (drop, after),
        )?;
    }
    inventory(
        graph,
        map_type,
        &seeds,
        &leases,
        root.map(result).transpose()?,
    )?;
    for instruction in &graph.instructions {
        for entity in instruction.operation.entities() {
            if seeds.contains(&entity) {
                let allowed = if source.owned {
                    root.is_some_and(|i| i.id == instruction.id)
                        || drop.is_some_and(|i| i.id == instruction.id)
                } else {
                    instruction.id == birth.id
                };
                need(
                    allowed,
                    "complete source-owner/caller-Borrow uses, no copy/capture/revival",
                    instruction,
                )?;
            }
            if leases.contains(&entity) {
                need(
                    [first.id, guard.id, key.id, value.id, next.id, end.id]
                        .contains(&instruction.id),
                    "complete provider lease uses",
                    instruction,
                )?;
            }
            if cursors.contains(&entity) {
                need(
                    entity != exit_cursor
                        && [guard.id, key.id, value.id, next.id].contains(&instruction.id),
                    "complete cursor uses, no false-edge dereference",
                    instruction,
                )?;
            }
        }
    }
    need(
        matches!(&graph.term(exit).map_err(debug)?.kind, TerminatorKind::Return { values } if values.is_empty())
            && graph.edges.iter().all(|(from, _)| *from != exit)
            && f.return_types.is_empty()
            && f.receiver().is_none()
            && f.borrow_return.is_none()
            && f.carrier_return.is_none(),
        "exhausted owner has no output, outgoing edge or revived availability (Never)",
        graph.term(exit).map_err(debug)?,
    )
}

fn components(
    graph: &Graph<'_>,
    instructions: [&Instruction; 3],
    types: [crate::ssa::model::SsaTypeId; 2],
) -> Result<(), String> {
    let [key, value, next] = instructions;
    let roots = [result(key)?, result(value)?];
    need(
        roots[0] != roots[1]
            && roots.iter().zip(types).all(|(&e, target)| {
                matches!(e, EntityId::Loan(_))
                    && graph.function.entity(e).is_some_and(|d| {
                        d.ty == EntityType::Loan {
                            kind: LoanKind::Shared,
                            target,
                        }
                    })
            }),
        "distinct actual Int/String component Shared loans",
        roots,
    )?;
    let ends = roots
        .iter()
        .map(|&root| {
            one(
                graph.instructions.iter().copied().filter(|i| {
                    matches!(i.operation,
            Operation::BorrowEnd { loan } if EntityId::Loan(loan) == root)
                }),
                "one exact component EndBinding",
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    need(
        ends.iter()
            .all(|i| i.block == key.block && i.origin == key.origin)
            && graph.position(key).map_err(debug)? < graph.position(value).map_err(debug)?
            && graph.position(value).map_err(debug)? < graph.position(ends[1]).map_err(debug)?
            && graph.position(ends[1]).map_err(debug)? < graph.position(ends[0]).map_err(debug)?
            && graph.position(ends[0]).map_err(debug)? < graph.position(next).map_err(debug)?
            && graph
                .edges
                .iter()
                .all(|(_, edge)| edge.arguments.iter().all(|e| !roots.contains(e))),
        "normal fallthrough ends value then key before Next, neither escapes",
        ends,
    )
}

fn inventory(
    graph: &Graph<'_>,
    map_type: crate::ssa::model::SsaTypeId,
    seeds: &[EntityId; 4],
    leases: &[EntityId; 4],
    root: Option<EntityId>,
) -> Result<(), String> {
    let f = graph.function;
    need(
        graph.blocks.len() == f.blocks.len() && graph.instructions.len() == f.instructions.len(),
        "all block and instruction arenas are reachable",
        (&graph.blocks, &f.blocks),
    )?;
    let mut entities = BTreeSet::new();
    for (block_index, block) in f.blocks.iter().enumerate() {
        need(
            block.id.function() == f.id && block.id.index() == block_index,
            "actual complete block arena identity",
            block.id,
        )?;
        for (index, &entity) in block.parameters.iter().enumerate() {
            need(
                entities.insert(entity)
                    && f.entity(entity).is_some_and(|data| {
                        data.definition
                            == Definition::BlockParameter {
                                block: block.id,
                                index,
                            }
                            && data.origin == block.origin
                    }),
                "complete actual parameter definition",
                entity,
            )?;
        }
    }
    for (instruction_index, instruction) in f.instructions.iter().enumerate() {
        need(
            instruction.id.function() == f.id && instruction.id.index() == instruction_index,
            "actual complete instruction arena identity",
            instruction.id,
        )?;
        for (index, &entity) in instruction.results.iter().enumerate() {
            need(
                entities.insert(entity)
                    && f.entity(entity).is_some_and(|data| {
                        data.definition
                            == Definition::InstructionResult {
                                instruction: instruction.id,
                                index,
                            }
                            && data.origin == instruction.origin
                    }),
                "complete actual result definition",
                entity,
            )?;
        }
    }
    need(
        entities.len() == f.values.len() + f.loans.len() + f.places.len(),
        "no orphan owner/loan/place entities",
        &entities,
    )?;
    let expected = seeds
        .iter()
        .chain(leases)
        .copied()
        .chain(root)
        .collect::<BTreeSet<_>>();
    let actual = entities
        .into_iter()
        .filter(|&entity| {
            f.entity(entity)
                .is_some_and(|data| data.ty.semantic_type() == map_type)
        })
        .collect::<BTreeSet<_>>();
    need(
        expected.len() == 8 + usize::from(root.is_some()) && actual == expected,
        "complete Map inventory excludes extra owner phi, capture, replacement or availability carrier",
        (actual, expected),
    )
}
