//! Newly rebuilt ordinary Borrow Map exits; not a restoration of the unavailable cloud collector.
//! MemoryDocument/rooted Unit pipelines and verifiers must succeed before separate native targets run.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod identity {
    //! Read-only original source references and caller delivery, independent of production qualification.
    use super::super::Report;
    use lang_frontend::{
        name_resolution::{SymbolKind, UnitReferenceTarget, ValidatedCompilationUnitNames},
        ownership_checking::{
            CompilationUnitOwnership, LoanKind, UnitCallArgumentOwnershipKind, UnitLoanTarget,
        },
        parser::{Expression, ParsedFile},
        type_checking::{CompilationUnitTypes, ParameterMode, UnitCallTarget, UnitExpressionId},
    };

    pub(super) fn unit(
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        expected_calls: usize,
        report: &mut Report,
    ) {
        let [source] = names.names().source_units() else {
            report.check(
                "unit one original source",
                false,
                names.names().source_units().len(),
            );
            return;
        };
        let unit = source.source_unit();
        let scans = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|d| {
                d.name() == "scan" && d.kind() == SymbolKind::Function && d.source_unit() == unit
            })
            .collect::<Vec<_>>();
        let [scan] = scans.as_slice() else {
            report.check("unit original scan declaration", false, scans);
            return;
        };
        let scan_symbols = source
            .resolution()
            .symbols()
            .iter()
            .filter(|s| s.name() == "scan" && s.kind() == SymbolKind::Function && !s.is_synthetic())
            .map(|s| s.id())
            .collect::<Vec<_>>();
        let signatures = typed
            .signatures()
            .declarations()
            .iter()
            .filter(|d| d.symbol().source_unit() == unit && scan_symbols.contains(&d.symbol().symbol()))
            .filter_map(|d| d.callable())
            .collect::<Vec<_>>();
        let parameter = match signatures.as_slice() {
            [s] => match s.parameters() {
                [p] if p.mode() == ParameterMode::Borrow => p.symbol(),
                _ => None,
            },
            _ => None,
        };
        report.check(
            "unit scan actual Borrow parameter",
            parameter.is_some(),
            &signatures,
        );
        for plan in owned.map_iterations() {
            let span = parsed
                .ast()
                .expressions()
                .get(plan.descriptor().source().expression())
                .map(|n| n.span());
            let refs = names
                .names()
                .references()
                .iter()
                .filter(|r| span.as_ref().is_ok_and(|s| r.span() == *s))
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(s) => Some(*s),
                    _ => None,
                })
                .collect::<Vec<_>>();
            report.check("unit original source reference equals exact scan parameter and planned root",
                matches!(refs.as_slice(), [root] if Some(*root) == parameter && root.source_unit() == unit && source.resolution().symbols().iter().any(|s| s.id() == root.symbol() && s.name() == "source" && s.kind() == SymbolKind::ValueParameter && !s.is_synthetic()) && matches!(plan.source(), Some(UnitLoanTarget::Place(p)) if p.is_root() && p.root() == *root)), (refs, parameter, plan.source()));
        }
        let mut calls = 0;
        for (id, node) in parsed.ast().expressions().iter() {
            let qualified = UnitExpressionId::new(unit, id);
            let Some(call) = typed
                .call(qualified)
                .filter(|c| c.target() == UnitCallTarget::Declaration(scan.id()))
            else {
                continue;
            };
            calls += 1;
            let Expression::Call { arguments, .. } = node.payload() else {
                report.check("unit original scan call AST", false, id);
                continue;
            };
            let [argument] = arguments.as_slice() else {
                report.check("unit scan original argument count", false, arguments);
                continue;
            };
            let argument_id = UnitExpressionId::new(unit, argument.value);
            let span = parsed
                .ast()
                .expressions()
                .get(argument.value)
                .map(|n| n.span());
            let refs = names
                .names()
                .references()
                .iter()
                .filter(|r| span.as_ref().is_ok_and(|s| r.span() == *s))
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(s) => Some(*s),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let contracts = owned
                .call_argument_contracts_for(qualified)
                .collect::<Vec<_>>();
            let loans = owned
                .loans()
                .iter()
                .filter(|l| l.call() == qualified)
                .collect::<Vec<_>>();
            let deliveries = owned
                .value_deliveries()
                .iter()
                .filter(|d| d.call() == qualified)
                .collect::<Vec<_>>();
            report.check("unit original caller Shared contract and loan, with no Move delivery",
                call.receiver().is_none() && matches!(call.arguments(), [a] if a.argument_index() == 0 && a.parameter_index() == 0 && a.mode() == ParameterMode::Borrow && Some(a.parameter_type()) == typed.expression_type(argument_id))
                && matches!(contracts.as_slice(), [c] if c.argument() == argument_id && c.parameter_index() == 0 && c.kind() == UnitCallArgumentOwnershipKind::SharedLoan)
                && matches!(refs.as_slice(), [root] if root.source_unit() == unit && matches!(loans.as_slice(), [l] if l.argument() == argument_id && l.kind() == LoanKind::Shared && matches!(l.target(), UnitLoanTarget::Place(p) if p.is_root() && p.root() == *root)))
                && deliveries.is_empty(), (qualified, call, refs, contracts, loans, deliveries));
        }
        report.check(
            "unit exact original caller count",
            calls == expected_calls,
            (calls, expected_calls),
        );
    }
}

use super::{
    Report,
    fixture::{Failure, clean, failure, verify_failure},
    pipeline,
};
use lang_frontend::{
    lexer::lex,
    name_resolution::ValidatedCompilationUnitNames,
    ownership_checking::{
        CompilationUnitOwnership, OwnershipBindingKind, UnitDropTarget, UnitIterationSourceAccess,
        UnitLoanTarget,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{CompilationUnitTypes, MapIterationKind},
};

pub(crate) fn sources() -> [(&'static str, &'static str, &'static str); 4] {
    [
        (
            "map-borrow-continue",
            include_str!(
                "../../../tests/fixtures/map-provider-borrow-exits-0331/map-borrow-continue.ko"
            ),
            include_str!(
                "../../../tests/fixtures/map-provider-borrow-exits-0331/map-borrow-continue.stdout"
            ),
        ),
        (
            "map-borrow-break",
            include_str!(
                "../../../tests/fixtures/map-provider-borrow-exits-0331/map-borrow-break.ko"
            ),
            include_str!(
                "../../../tests/fixtures/map-provider-borrow-exits-0331/map-borrow-break.stdout"
            ),
        ),
        (
            "map-borrow-return",
            include_str!(
                "../../../tests/fixtures/map-provider-borrow-exits-0331/map-borrow-return.ko"
            ),
            include_str!(
                "../../../tests/fixtures/map-provider-borrow-exits-0331/map-borrow-return.stdout"
            ),
        ),
        (
            "map-borrow-conditional",
            include_str!(
                "../../../tests/fixtures/map-provider-borrow-exits-0331/map-borrow-conditional.ko"
            ),
            include_str!(
                "../../../tests/fixtures/map-provider-borrow-exits-0331/map-borrow-conditional.stdout"
            ),
        ),
    ]
}

fn observe(label: &str, text: &str, rooted: bool, report: &mut Report) -> Result<(), Failure> {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("provider.ko", text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, source).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex diagnostics", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse diagnostics", parsed.diagnostics())?;
    let expected_calls = if label == "map-borrow-conditional" {
        5
    } else {
        2
    };
    let observe = |names: &ValidatedCompilationUnitNames,
                   typed: &CompilationUnitTypes,
                   owned: &CompilationUnitOwnership,
                   report: &mut Report| {
        identity::unit(&parsed, names, typed, owned, expected_calls, report);
        let plans = owned.map_iterations();
        report.check(
            "unit one real borrowed Map provider",
            plans.len() == 1,
            plans,
        );
        for plan in plans {
            let descriptor = plan.descriptor();
            let origin = descriptor.source();
            let span = parsed
                .ast()
                .expressions()
                .get(origin.expression())
                .map(|n| n.span());
            let root = match plan.source() {
                Some(UnitLoanTarget::Place(place)) if place.is_root() => Some(place.root()),
                _ => None,
            };
            report.check(
                "unit exact borrowed source identity and no owned source drop",
                descriptor.kind() == MapIterationKind::Map
                    && typed.map_iteration(descriptor.statement()) == Some(descriptor)
                    && plan.source_access() == UnitIterationSourceAccess::Shared
                    && root.is_some_and(|root| {
                        owned.binding_kind(root) == Some(OwnershipBindingKind::Shared)
                            && !owned
                                .drops()
                                .iter()
                                .any(|d| d.target() == UnitDropTarget::Named(root))
                    })
                    && span.is_ok_and(|span| sources.slice(span) == Ok("source"))
                    && descriptor.key_binding() != descriptor.value_binding()
                    && plan.loans()[0] != plan.loans()[1],
                (descriptor, root, span),
            );
        }
    };
    let (program, entry) = if rooted {
        pipeline::unit_observed(&sources, &parsed, "provider.ko", report, observe)?
    } else {
        pipeline::memory_document_observed(&sources, &parsed, report, observe)?
    };
    super::super::verify::verify_program(&program).map_err(|e| verify_failure(&sources, e))?;
    report.check("SSA verifier", true, entry);
    let llvm = crate::llvm::render_verified_program(&program)
        .map_err(|e| failure("LLVM render/verify", e))?;
    report.check("LLVM render/verify", true, llvm.len());
    Ok(())
}

#[test]
fn borrowed_map_direct_exits_reach_verified_llvm_in_both_entries() {
    let mut results = Vec::new();
    for (label, text, _) in sources() {
        for rooted in [false, true] {
            let input = super::input_label(rooted);
            let mut report = Report::default();
            let outcome = observe(label, text, rooted, &mut report);
            diagnostic!(
                "SPEC0331 BORROW EXIT case={label} input={input} outcome={outcome:?} report={report:#?}"
            );
            results.push((label, input, report.passed() && outcome.is_ok()));
        }
    }
    assert_diagnostic!(
        results.len() == 8 && results.iter().all(|(_, _, passed)| *passed),
        "all eight original real pipelines must pass: {results:?}"
    );
}

pub(in crate::ssa) mod query_owner {
//! Unit 六源的实际 owner/caller Borrow 与 provider lease 定义库存。
//! 角色已由 query_ssa 的有向列证明；同类型只用于排除额外实体，不用于认定同根。
use super::super::{
    fixture::{Exit, Failure, failure, need},
    query_graph::{Graph, Uses},
    query_inputs::{Inputs, one},
};
use crate::ssa::model::*;
use lang_frontend::source::Span;
use std::collections::BTreeSet;

pub(in crate::ssa) struct SourceRoles<'a> {
    pub blocks: &'a [BlockId],
    pub seeds: &'a [EntityId],
    pub leases: [EntityId; 4],
    pub root: Option<EntityId>,
}

fn definitions(graph: &Graph<'_>) -> Result<BTreeSet<EntityId>, Failure> {
    let f = graph.function;
    need(
        graph.blocks.len() == f.blocks.len()
            && f.blocks
                .iter()
                .enumerate()
                .all(|(index, block)| block.id.function() == f.id && block.id.index() == index)
            && graph.instructions.len() == f.instructions.len()
            && f.instructions
                .iter()
                .enumerate()
                .all(|(index, instruction)| {
                    instruction.id.function() == f.id && instruction.id.index() == index
                }),
        "identity SSA owner inventory covers actual block/instruction arenas",
        (&graph.blocks, &graph.instructions),
    )?;
    let mut entities = BTreeSet::new();
    for block in &f.blocks {
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
                "identity SSA owner inventory exact parameter definition/origin",
                (block.id, index, entity),
            )?;
        }
    }
    for instruction in &graph.instructions {
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
                "identity SSA owner inventory exact result definition/origin",
                (instruction.id, index, entity),
            )?;
        }
    }
    need(
        entities.len() == f.values.len() + f.loans.len() + f.places.len(),
        "identity SSA owner inventory excludes every orphan entity",
        &entities,
    )?;
    Ok(entities)
}

fn parameter(
    graph: &Graph<'_>,
    block: BlockId,
    entity: EntityId,
    origin: Span,
) -> Result<(), Failure> {
    let index = one(
        graph
            .block(block)?
            .parameters
            .iter()
            .enumerate()
            .filter_map(|(index, &actual)| (actual == entity).then_some(index)),
        "identity SSA source role has one actual parameter column",
    )?;
    need(
        graph.function.entity(entity).is_some_and(|data| {
            data.definition == Definition::BlockParameter { block, index }
                && data.origin == Origin::Source(origin)
        }),
        "identity SSA source role exact parameter definition and source origin",
        (block, index, entity, origin),
    )
}

pub(in crate::ssa) fn check(
    module: &Module,
    graph: &Graph<'_>,
    input: &Inputs,
    roles: SourceRoles<'_>,
    uses: &mut Uses,
) -> Result<(), Failure> {
    let SourceRoles {
        blocks,
        seeds,
        leases,
        root,
    } = roles;
    let entities = definitions(graph)?;
    let borrowed_merge = !input.owned && input.exit == Exit::Break;
    // Refinement SSA keeps the caller Borrow as a dominating definition when the
    // Break merge tail never reads it again, so five seeds are legal but not required.
    let seed_count = seeds.len();
    need(
        blocks.len() == 4 + usize::from(input.exit == Exit::Break)
            && (seed_count == 4 || (borrowed_merge && seed_count == 5)),
        "identity SSA all source roles including only the live Borrow Break merge",
        (blocks, seeds),
    )?;
    let seed_type = graph
        .function
        .entity(seeds[0])
        .ok_or_else(|| failure("identity SSA owner inventory original parameter", seeds[0]))?
        .ty;
    let map_type = seed_type.semantic_type();
    need(
        matches!(
            module.type_kind(map_type),
            Some(SsaTypeKind::MapContainer { .. })
        ) && module.type_ownership(map_type) == Some(Ownership::MoveOnly)
            && seed_type
                == if input.owned {
                    EntityType::Value(map_type)
                } else {
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: map_type,
                    }
                }
            && graph.function.borrow_return.is_none()
            && graph.function.carrier_return.is_none(),
        "identity SSA nonnullable MoveOnly source and no borrowed/carrier return ABI",
        (seed_type, &graph.function.return_types),
    )?;
    let mut expected = BTreeSet::new();
    for (index, &seed) in seeds.iter().enumerate() {
        // Unit orchestrate gives entry parameters the callable instance span;
        // their FE Drop origin remains the original parameter span. All carried
        // blocks, including the borrowed Break merge, are created at the For.
        parameter(
            graph,
            blocks[index],
            seed,
            if index == 0 {
                input.scan
            } else {
                input.site.span
            },
        )?;
        need(
            expected.insert(seed)
                && matches!(
                    (seed, input.owned),
                    (EntityId::Value(_), true) | (EntityId::Loan(_), false)
                )
                && graph
                    .function
                    .entity(seed)
                    .is_some_and(|data| data.ty == seed_type),
            "identity SSA independent current source identities retain exact ownership type",
            seed,
        )?;
    }
    for (index, source) in leases.into_iter().enumerate() {
        need(
            expected.insert(source)
                && matches!(source, EntityId::Loan(_))
                && graph.function.entity(source).is_some_and(|data| {
                    data.ty
                        == EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: map_type,
                        }
                        && data.origin == Origin::Source(input.site.span)
                }),
            "identity SSA provider leases distinct from every owner/caller Borrow",
            source,
        )?;
        if index != 0 {
            parameter(graph, blocks[index], source, input.site.span)?;
        }
    }
    need(
        root.is_some() == input.owned,
        "identity SSA only owned source has an actual RootPlace",
        root,
    )?;
    if let Some(root) = root {
        need(
            expected.insert(root)
                && matches!(root, EntityId::Place(_))
                && graph.function.entity(root).is_some_and(|data| {
                    data.ty == EntityType::Place(map_type)
                        && data.origin == Origin::Source(input.site.span)
                }),
            "identity SSA exact owned source storage is separate from every lease",
            root,
        )?;
    }
    let actual = entities
        .into_iter()
        .filter(|&entity| {
            graph
                .function
                .entity(entity)
                .is_some_and(|data| module.map_container(data.ty.semantic_type()).is_some())
        })
        .collect::<BTreeSet<_>>();
    need(
        actual == expected,
        "identity SSA complete Map owner/loan/place inventory has no replacement or extra capability",
        (&actual, &expected),
    )?;
    // Every map entity, not just the initially discovered operands, is now in
    // the existing exact legal-use check. This adds no permitted operation.
    for entity in actual {
        uses.protect(entity);
    }
    Ok(())
}
}

pub(in crate::ssa) mod query_inputs {
//! 同次公开 FE products 的身份桥；不调用生产 qualification，不构造或修改 facts。
use super::super::{
    fixture::{Exit, Failure, Fixture, LoopSite, failure, loop_site, need},
    identity,
};
use lang_frontend::{
    name_resolution::ValidatedCompilationUnitNames,
    ownership_checking::{
        CompilationUnitOwnership, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
        UnitIterationExitKind as UE, UnitLoanTarget,
    },
    parser::{Item, NameMarker},
    source::Span,
    type_checking::{CompilationUnitTypes, UnitCallTarget, UnitExpressionId, UnitStatementId},
};

pub(in crate::ssa) fn one<T>(
    mut values: impl Iterator<Item = T>,
    stage: &'static str,
) -> Result<T, Failure> {
    let value = values
        .next()
        .ok_or_else(|| failure(stage, "missing entity"))?;
    need(values.next().is_none(), stage, "multiple entities")?;
    Ok(value)
}

#[derive(Debug)]
pub(in crate::ssa) struct Inputs {
    pub scan: Span,
    pub consumer: Span,
    pub deinit: Span,
    pub parameter: Span,
    pub parameter_slot: usize,
    pub source_drop: Option<Span>,
    pub site: LoopSite,
    pub owned: bool,
    pub exit: Exit,
}

fn function_span(f: &Fixture, name: Span) -> Result<Span, Failure> {
    one(f.parsed.roots().iter().filter_map(|root| {
        let node = f.parsed.ast().items().get(*root).ok()?;
        matches!(node.payload(), Item::Function { name: NameMarker::Present(n), .. } if *n == name)
            .then_some(node.span())
    }), "identity FE actual function root")
}

fn finish(
    f: &Fixture,
    scan: Span,
    consumer: Span,
    parameter: Span,
    parameter_slot: usize,
    source_drop: Option<Span>,
) -> Result<Inputs, Failure> {
    let functions = identity::source_functions(f)?;
    let scan = function_span(f, scan)?;
    let consumer = function_span(f, consumer)?;
    need(
        functions.contains(&("scan", scan)) && functions.contains(&(f.exit.callee(), consumer)),
        "identity FE source declarations agree with actual AST roots",
        &functions,
    )?;
    let deinit = one(
        functions
            .iter()
            .filter_map(|(n, s)| (*n == "Guard.deinit").then_some(*s)),
        "identity FE Guard deinit root",
    )?;
    Ok(Inputs {
        scan,
        consumer,
        deinit,
        parameter,
        parameter_slot,
        source_drop,
        site: loop_site(f)?,
        owned: f.map,
        exit: f.exit,
    })
}

pub(in crate::ssa) fn unit(
    f: &Fixture,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Inputs, Failure> {
    let s = loop_site(f)?;
    let source_unit = one(
        names.names().source_units().iter(),
        "identity FE original unit",
    )?;
    let unit = source_unit.source_unit();
    let statement = UnitStatementId::new(unit, s.statement);
    let transfer = UnitExpressionId::new(unit, s.transfer);
    let d = typed
        .map_iteration(statement)
        .ok_or_else(|| failure("identity FE unit descriptor", statement))?;
    let p = owned
        .map_iteration(statement)
        .ok_or_else(|| failure("identity FE unit plan", statement))?;
    let Some(UnitLoanTarget::Place(path)) = p.source() else {
        return Err(failure("identity FE unit root", p));
    };
    let source = path.root();
    need(
        path.is_root() && source.source_unit() == unit,
        "identity FE unit source identity",
        source,
    )?;
    let parameter = one(
        source_unit
            .resolution()
            .symbols()
            .iter()
            .filter(|v| v.id() == source.symbol()),
        "identity FE unit parameter",
    )?;
    let (signature, slot) = one(
        typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|v| v.callable())
            .flat_map(|c| {
                c.parameters()
                    .iter()
                    .enumerate()
                    .filter_map(move |(i, p)| (p.symbol() == Some(source)).then_some((c, i)))
            }),
        "identity FE unit parameter slot",
    )?;
    need(
        signature.receiver().is_none(),
        "identity FE unit no receiver",
        signature,
    )?;
    let call = typed
        .call(UnitExpressionId::new(unit, s.call))
        .ok_or_else(|| failure("identity FE unit call", s.call))?;
    let UnitCallTarget::Declaration(callee) = call.target() else {
        return Err(failure("identity FE unit source callee", call));
    };
    let consumer = one(
        names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|v| v.id() == callee && v.source_unit() == unit),
        "identity FE unit consumer",
    )?;
    let local = one(
        source_unit
            .resolution()
            .symbols()
            .iter()
            .filter(|v| v.span() == s.local),
        "identity FE unit Guard local",
    )?;
    let guard = *one(owned.drops().iter().filter(|v| matches!(v.target(), UnitDropTarget::Named(n) if n.source_unit() == unit && n.symbol() == local.id())), "identity FE unit Guard Drop")?;
    need(
        guard.point() == UnitDropPoint::ControlTransfer(transfer)
            && guard.value_origin() == s.local,
        "identity FE unit Guard Drop identity",
        guard,
    )?;
    let drops = owned
        .drops()
        .iter()
        .filter(|v| v.target() == UnitDropTarget::Named(source))
        .collect::<Vec<_>>();
    let expected_count = usize::from(f.map) * if f.exit == Exit::Continue { 1 } else { 2 };
    need(
        drops.len() == expected_count && drops.iter().all(|v| v.value_origin() == parameter.span()),
        "identity FE unit all source Drop origins",
        &drops,
    )?;
    let transfer_kind = match f.exit {
        Exit::Continue => UE::Continue(transfer),
        Exit::Break => UE::Break(transfer),
        Exit::Return => UE::Return(transfer),
    };
    need(
        p.exits().len() == 2,
        "identity FE unit actual two exits",
        p.exits(),
    )?;
    for (kind, point, is_transfer) in [
        (
            transfer_kind,
            UnitDropPoint::ControlTransfer(transfer),
            true,
        ),
        (UE::Exhaustion, UnitDropPoint::LoopExit(statement), false),
    ] {
        let exit = one(
            p.exits()
                .iter()
                .filter(|v| v.kind() == kind && v.point() == point),
            "identity FE unit exact exit",
        )?;
        let mut expected = if is_transfer {
            vec![
                UA::Drop(guard),
                UA::EndBinding {
                    statement,
                    symbol: d.value_binding(),
                },
                UA::EndBinding {
                    statement,
                    symbol: d.key_binding(),
                },
            ]
        } else {
            vec![]
        };
        if !is_transfer || f.exit != Exit::Continue {
            expected.extend([UA::FinishProvider(statement), UA::EndSource(statement)]);
            if f.map {
                expected.push(UA::Drop(*one(
                    drops.iter().copied().filter(|v| v.point() == point),
                    "identity FE unit per-path source Drop",
                )?));
            }
        }
        need(
            exit.actions() == expected
                && owned.iteration_cleanup_at(point) == Some(expected.as_slice()),
            "identity FE unit full cleanup sequence",
            (exit, expected),
        )?;
    }
    finish(
        f,
        signature.name_span(),
        consumer.name_span(),
        parameter.span(),
        slot,
        f.map.then_some(parameter.span()),
    )
}
}
