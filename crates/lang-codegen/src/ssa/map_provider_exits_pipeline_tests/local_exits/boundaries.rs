//! Legal alternate formation histories must not be mistaken for missing frontend facts.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
// The input flag selects rooted Unit or MemoryDocument Unit, never Legacy lowering.
mod alias;
pub(in super::super) mod formation;
use super::super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms,
};
use crate::ssa::verify::verify_program;
use lang_frontend::{
    lexer::lex,
    parser::{Statement, parse_file},
    source::SourceMap,
};

pub(crate) fn sources() -> Vec<(&'static str, String)> {
    let common = r#"fun read(key:String,value:String):Unit{println(key);println(value)}
fun readMap(source:Map<String,String>):Unit{if(source.size==0){println("zero")}else{println("nonzero")}}
fun make():Map<String,String>{return mapOf<String,String>()}
fun other():Map<String,String>{return mapOf<String,String>()}
fun makeMutable():MutableMap<String,String>{return mutableMapOf<String,String>()}
"#;
    let histories = [
        (
            "reassigned",
            "",
            "var source=make();source=other()",
            "",
            "scan()",
        ),
        (
            "parameter-move",
            "own incoming:Map<String,String>",
            "val source=incoming",
            "",
            "val input=make();scan(input)",
        ),
        (
            "old-alias-rebound",
            "",
            "var old=make();val source=old;old=other()",
            "readMap(old)",
            "scan()",
        ),
        (
            "prefix-cfg",
            "",
            "val source=make();if(source.size==0){println(\"zero\")}else{println(\"nonzero\")}",
            "",
            "scan()",
        ),
        (
            "prefix-put",
            "",
            "var source=makeMutable();source.put(\"key\".clone(),\"value\".clone())",
            "",
            "scan()",
        ),
    ];
    histories.into_iter().map(|(label,parameters,formation,after,entry)|{
        (label,format!("{common}\nfun scan({parameters}):Unit{{{formation};println(\"gap\");for((key,value) in source){{read(key,value)}};{after}}}\nfun entry():Unit{{{entry}}}\n"))
    }).collect()
}

fn observe(label: &str, source: &str, unit: bool) -> Result<&'static str, Failure> {
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let spans = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(_, n)| matches!(n.payload(), Statement::For { .. }).then_some(n.span()))
        .collect::<Vec<_>>();
    assert_eq!(spans.len(), 1);
    diagnostic!(
        "G2 ALTERNATE ORIGINAL FOR case={label} unit={unit} span={:?}",
        spans[0]
    );
    let mut report = Report::default();
    let mut identity = None;
    let result = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                let observed = formation::unit(&sources, &parsed, names, typed, owned);
                report.query(
                    "alternate original qualified formation identities",
                    &observed,
                );
                identity = Some(observed);
                if label == "old-alias-rebound" {
                    report.query(
                        "rebound alias original Unit identities",
                        &alias::unit_facts(&sources, &parsed, names, typed, owned),
                    );
                }
            },
        )
    } else {
        pipeline::memory_document_observed(
            &sources,
            &parsed,
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                let observed = formation::unit(&sources, &parsed, names, typed, owned);
                report.query(
                    "alternate original qualified formation identities",
                    &observed,
                );
                identity = Some(observed);
                if label == "old-alias-rebound" {
                    report.query(
                        "rebound alias original Unit identities",
                        &alias::unit_facts(&sources, &parsed, names, typed, owned),
                    );
                }
            },
        )
    };
    diagnostic!(
        "G2 ALTERNATE unit={unit} source={source:?} report={report:#?} result={:?}",
        result.as_ref().map(|(_, id)| id)
    );
    if label == "old-alias-rebound"
        && let Ok((program, _)) = &result
    {
        report.query(
            "rebound alias actual SSA owner separation",
            &identity
                .as_ref()
                .ok_or_else(|| failure("alias original input bridge", "absent"))
                .and_then(|v| {
                    v.as_ref()
                        .map_err(|e| failure("alias same-analysis FE bridge", e))
                })
                .and_then(formation::alias_drop_origins)
                .and_then(|origins| alias::graph(&sources, &parsed, program, origins)),
        );
    }
    if let (Some(Ok(input)), Ok((program, _))) = (&identity, &result) {
        report.query(
            "alternate exact current owners and complete graph",
            &formation::check(&sources, &parsed, program, input),
        );
    }
    if let Ok((program, _)) = &result {
        report.query(
            "Unit complete temporary source acquisition",
            &source_forms::graph::temporary_entries(&parsed, program),
        );
        report.query(
            "alternate actual SSA verifier",
            &verify_program(program).map_err(|e| failure("SSA", e)),
        );
        report.query(
            "alternate actual LLVM renderer",
            &crate::llvm::render_verified_program(program)
                .map(|_| ())
                .map_err(|e| failure("LLVM", e)),
        );
    }
    if !report.passed() {
        return Err(failure("alternate original FE", report));
    }
    result?;
    Ok("verified LLVM")
}

#[test]
fn legal_alternate_histories_reach_verified_llvm() {
    let mut outcomes = Vec::new();
    for (label, source) in super::alternate_sources() {
        for unit in [false, true] {
            let result = observe(label, &source, unit);
            diagnostic!("G2 ALTERNATE CASE {label} unit={unit} result={result:?}");
            outcomes.push((label, unit, result));
        }
    }
    assert_diagnostic!(
        outcomes.len() == 10 && outcomes.iter().all(|(_, _, r)| r.is_ok()),
        "{outcomes:#?}"
    );
}

pub(crate) fn alias_rebound_source() -> (&'static str, String, &'static str) {
    let selected = sources()
        .into_iter()
        .filter(|(label, _)| *label == "old-alias-rebound")
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 1);
    let (label, source) = selected.into_iter().next().unwrap();
    (label, source, "gap\nzero\n")
}

pub(in crate::ssa) mod unit_cleanup {
//! Unit 只核公开 qualified drops/plans/同表 getter，不制造 single owner/phi 表。
use crate::ssa::map_provider_exits_pipeline_tests::temporary_exits::ast::{Anchors, Exit, Input};
use crate::ssa::map_provider_exits_pipeline_tests::temporary_exits::query_inputs::query_cleanup::{Failure, failure, need, one};
use lang_frontend::{
    name_resolution::{SourceUnitId, UnitSymbolId},
    ownership_checking::{
        CompilationUnitOwnership, UnitDropFact, UnitDropPoint, UnitDropTarget,
        UnitIterationCleanupAction as A, UnitIterationExitKind as E,
    },
    source::Span,
    type_checking::{UnitExpressionId, UnitStatementId},
};
pub(in crate::ssa) fn check(
    f: &Input,
    a: &Anchors,
    guards: [UnitSymbolId; 2],
    owned: &CompilationUnitOwnership,
    unit: SourceUnitId,
) -> Result<[Span; 2], Failure> {
    let expression = |id| UnitExpressionId::new(unit, id);
    let mut expected_drops = Vec::new();
    let mut points = Vec::new();
    let mut origins = Vec::new();
    for (index, site) in a.sites.iter().enumerate() {
        let statement = UnitStatementId::new(unit, site.statement);
        let source = expression(site.source);
        let transfer_id = expression(site.transfer);
        let plan = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("temporary exits FE actual qualified cleanup plan", site))?;
        let d = plan.descriptor();
        let transfer = UnitDropPoint::ControlTransfer(transfer_id);
        let exhausted = UnitDropPoint::LoopExit(statement);
        let guard = actual_drop(owned, UnitDropTarget::Named(guards[index]), transfer)?;
        let source_drop = actual_drop(owned, UnitDropTarget::Temporary(source), exhausted)?;
        need(
            guards[index].source_unit() == unit
                && guard.value_origin() == a.guards[index].marker
                && source_drop.value_origin() == site.source_span,
            "temporary exits FE actual qualified Guard marker and factory Call drop origins",
            (guard, source_drop),
        )?;
        expected_drops.extend([*guard, *source_drop]);
        origins.push(source_drop.value_origin());
        let mut actions = vec![
            A::Drop(*guard),
            A::EndBinding {
                statement,
                symbol: d.value_binding(),
            },
            A::EndBinding {
                statement,
                symbol: d.key_binding(),
            },
        ];
        if f.exit != Exit::Continue {
            let moved = actual_drop(owned, UnitDropTarget::Temporary(source), transfer)?;
            need(
                moved.value_origin() == site.source_span,
                "temporary exits FE qualified transfer Drop actual source Call origin",
                moved,
            )?;
            actions.extend([
                A::FinishProvider(statement),
                A::EndSource(statement),
                A::Drop(*moved),
            ]);
            expected_drops.push(*moved);
        }
        need(
            actions.len() == if f.exit == Exit::Continue { 3 } else { 6 },
            "temporary exits FE qualified exact transfer action count",
            &actions,
        )?;
        let false_actions = vec![
            A::FinishProvider(statement),
            A::EndSource(statement),
            A::Drop(*source_drop),
        ];
        let kind = match f.exit {
            Exit::Continue => E::Continue(transfer_id),
            Exit::Break => E::Break(transfer_id),
            Exit::Return => E::Return(transfer_id),
        };
        need(
            plan.exits().len() == 2,
            "temporary exits FE qualified exact transfer and false-terminal exits",
            plan.exits(),
        )?;
        for (kind, point, actions) in [
            (kind, transfer, actions),
            (E::Exhaustion, exhausted, false_actions),
        ] {
            let exit = one(
                plan.exits().iter().filter(|exit| exit.kind() == kind),
                "temporary exits FE unique qualified actual exit kind",
            )?;
            need(
                exit.point() == point && exit.actions() == actions,
                "temporary exits FE independently expected qualified complete actions",
                (exit, &actions),
            )?;
            // 该 accessor 只筛同表，不能声称独立证明 acquired/owner/loan-end。
            need(
                owned.iteration_cleanup_at(point) == Some(actions.as_slice()),
                "temporary exits FE qualified same-table full-point getter consistency",
                (point, &actions),
            )?;
            let plans = owned
                .map_iterations()
                .iter()
                .filter(|p| p.exits().iter().any(|e| e.point() == point))
                .collect::<Vec<_>>();
            need(
                plans.len() == 1 && plans[0].descriptor().statement() == statement,
                "temporary exits FE qualified point belongs to one independent provider",
                (point, plans),
            )?;
            points.push(point);
        }
    }
    need(
        points.len() == 4
            && points
                .iter()
                .enumerate()
                .all(|(i, p)| !points[..i].contains(p)),
        "temporary exits FE four distinct qualified cleanup points",
        &points,
    )?;
    let related = |drop: &UnitDropFact| {
        guards
            .iter()
            .any(|id| drop.target() == UnitDropTarget::Named(*id))
            || a.sites
                .iter()
                .any(|site| drop.target() == UnitDropTarget::Temporary(expression(site.source)))
    };
    let actual = owned
        .drops()
        .iter()
        .filter(|d| related(d))
        .copied()
        .collect::<Vec<_>>();
    need(
        actual.len() == expected_drops.len()
            && expected_drops
                .iter()
                .all(|d| actual.iter().filter(|actual| *actual == d).count() == 1),
        "temporary exits FE complete qualified relevant-target Drop coverage",
        (actual, expected_drops),
    )?;
    origins
        .try_into()
        .map_err(|v| failure("temporary exits FE two qualified source Drop spans", v))
}
fn actual_drop(
    owned: &CompilationUnitOwnership,
    target: UnitDropTarget,
    point: UnitDropPoint,
) -> Result<&UnitDropFact, Failure> {
    one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == target && d.point() == point),
        "temporary exits FE unique qualified actual target at point",
    )
}
}

pub(in crate::ssa) mod inlined_unit_cleanup_1 {
//! Unit 仅检查它自己的公开退出/Drop 表，getter 明确只是同表一致性。
use crate::ssa::map_provider_exits_pipeline_tests::nested_exits::ast::{Anchors, Exit, Input};
use crate::ssa::map_provider_exits_pipeline_tests::nested_exits::query_inputs::query_cleanup::{Failure, failure, need, one};
use lang_frontend::{
    name_resolution::{SourceUnitId, UnitSymbolId},
    ownership_checking::{
        CompilationUnitOwnership, UnitDropFact, UnitDropPoint as P, UnitDropTarget,
        UnitIterationCleanupAction as A, UnitIterationExitKind as E,
    },
    source::Span,
    type_checking::{UnitExpressionId, UnitStatementId},
};

pub(in crate::ssa) fn check(
    f: &Input,
    a: &Anchors,
    guards: [UnitSymbolId; 2],
    owned: &CompilationUnitOwnership,
    unit: SourceUnitId,
) -> Result<(), Failure> {
    let expression = |id| UnitExpressionId::new(unit, id);
    let statement = |id| UnitStatementId::new(unit, id);
    let transfer_id = expression(a.transfer);
    let transfer = P::ControlTransfer(transfer_id);
    let after_outer = P::AfterStatement(statement(a.sites[0].body));
    let inner_drop = actual_drop(owned, guards[1], transfer, a.guards[1].marker)?;
    let outer_drop = actual_drop(owned, guards[0], after_outer, a.guards[0].marker)?;
    let mut expected_drops = vec![inner_drop, outer_drop];
    let outer = owned
        .map_iteration(statement(a.sites[0].statement))
        .ok_or_else(|| failure("nested exits FE qualified outer cleanup plan", &a.sites[0]))?;
    let inner = owned
        .map_iteration(statement(a.sites[1].statement))
        .ok_or_else(|| failure("nested exits FE qualified inner cleanup plan", &a.sites[1]))?;
    let finish = |index: usize| {
        vec![
            A::FinishProvider(statement(a.sites[index].statement)),
            A::EndSource(statement(a.sites[index].statement)),
        ]
    };
    let end_bindings = |index: usize| {
        let d = if index == 0 {
            outer.descriptor()
        } else {
            inner.descriptor()
        };
        vec![
            A::EndBinding {
                statement: statement(a.sites[index].statement),
                symbol: d.value_binding(),
            },
            A::EndBinding {
                statement: statement(a.sites[index].statement),
                symbol: d.key_binding(),
            },
        ]
    };
    let mut exit_actions = vec![A::Drop(*inner_drop)];
    exit_actions.extend(end_bindings(1));
    if f.exit != Exit::Continue {
        exit_actions.extend(finish(1));
    }
    if f.exit == Exit::Return {
        let outer_return = actual_drop(owned, guards[0], transfer, a.guards[0].marker)?;
        expected_drops.push(outer_return);
        exit_actions.push(A::Drop(*outer_return));
        exit_actions.extend(end_bindings(0));
        exit_actions.extend(finish(0));
    }
    need(
        exit_actions.len()
            == match f.exit {
                Exit::Continue => 3,
                Exit::Break => 5,
                Exit::Return => 10,
            },
        "nested exits FE qualified full transfer action count",
        &exit_actions,
    )?;
    let mut outer_actions = vec![A::Drop(*outer_drop)];
    outer_actions.extend(end_bindings(0));
    let points = [
        (transfer, exit_actions),
        (P::LoopExit(statement(a.sites[1].statement)), finish(1)),
        (after_outer, outer_actions),
        (P::LoopExit(statement(a.sites[0].statement)), finish(0)),
    ];
    let exit_kind = match f.exit {
        Exit::Continue => E::Continue(transfer_id),
        Exit::Break => E::Break(transfer_id),
        Exit::Return => E::Return(transfer_id),
    };
    for (index, plan) in [outer, inner].into_iter().enumerate() {
        let mut expected = if index == 1 {
            vec![(exit_kind, 0), (E::Exhaustion, 1)]
        } else {
            vec![(E::Fallthrough, 2), (E::Exhaustion, 3)]
        };
        if index == 0 && f.exit == Exit::Return {
            expected.push((exit_kind, 0));
        }
        need(
            plan.exits().len() == expected.len(),
            "nested exits FE qualified exact per-provider exit coverage",
            plan.exits(),
        )?;
        for (kind, point_index) in expected {
            let actual = one(
                plan.exits().iter().filter(|e| e.kind() == kind),
                "nested exits FE unique qualified actual transfer exit",
            )?;
            let (point, actions) = &points[point_index];
            need(
                actual.point() == *point && actual.actions() == actions,
                "nested exits FE qualified full ordered per-plan sequence",
                (actual, actions),
            )?;
        }
        let descriptor = plan.descriptor();
        need(
            !owned.drops().iter().any(|drop| {
                drop.target() == UnitDropTarget::Temporary(expression(a.sites[index].source))
                    || descriptor
                        .bindings()
                        .iter()
                        .any(|id| drop.target() == UnitDropTarget::Named(*id))
            }),
            "nested exits FE qualified borrowed source/components have no Drop",
            plan,
        )?;
    }
    // 两个 Return plan 上方各核同一完整十动作；此 getter 按独立实际 point 只核一次。
    // Unit 没有公开 single 的 owner/condition/phi 表；这里不借用或假造那套事实。
    for (index, (point, actions)) in points.iter().enumerate() {
        need(
            !points[..index].iter().any(|(prior, _)| prior == point),
            "nested exits FE qualified unique full cleanup point",
            point,
        )?;
        need(
            owned.iteration_cleanup_at(*point) == Some(actions.as_slice()),
            "nested exits FE qualified full same-table cleanup getter consistency",
            (point, actions),
        )?;
    }
    let actual_drops = owned
        .drops()
        .iter()
        .filter(|d| {
            guards
                .iter()
                .any(|id| d.target() == UnitDropTarget::Named(*id))
        })
        .collect::<Vec<_>>();
    need(
        actual_drops.len() == expected_drops.len()
            && expected_drops
                .iter()
                .all(|drop| actual_drops.iter().filter(|v| *v == drop).count() == 1),
        "nested exits FE qualified all Guard Drop uses exactly accounted",
        (&actual_drops, expected_drops),
    )?;
    Ok(())
}

fn actual_drop(
    owned: &CompilationUnitOwnership,
    local: UnitSymbolId,
    point: P,
    origin: Span,
) -> Result<&UnitDropFact, Failure> {
    let drop = one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(local) && d.point() == point),
        "nested exits FE unique actual qualified Guard Drop target and point",
    )?;
    need(
        drop.value_origin() == origin,
        "nested exits FE qualified Guard Drop actual origin",
        drop,
    )?;
    Ok(drop)
}

pub(in crate::ssa) fn caller(
    local: UnitSymbolId,
    marker: Span,
    size: UnitExpressionId,
    owned: &CompilationUnitOwnership,
) -> Result<Span, Failure> {
    let drop = one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(local)),
        "nested exits FE unique qualified caller local Drop",
    )?;
    need(
        drop.point() == P::CallReturn(size) && drop.value_origin() == marker,
        "nested exits FE qualified caller Drop follows actual size with local origin",
        (local, marker, size, drop),
    )?;
    Ok(drop.value_origin())
}
}

pub(in crate::ssa) mod structure {
//! Exact ordinary-source SSA lineage; only transparent block parameters are followed.
use crate::ssa::model::{
    BinaryOperator, BlockId, Definition, Edge, EntityId, Function, Operation, Origin, Program,
    ScalarConstant, TerminatorKind,
};
fn edges(function: &Function) -> Vec<(BlockId, &Edge)> {
    function
        .blocks
        .iter()
        .flat_map(|block| {
            let edges = match &block.terminator.as_ref().unwrap().kind {
                TerminatorKind::Branch(edge) => vec![edge],
                TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } => vec![when_true, when_false],
                _ => Vec::new(),
            };
            edges.into_iter().map(move |edge| (block.id, edge))
        })
        .collect()
}
fn forwards(function: &Function, value: EntityId, expected: EntityId, _depth: usize) -> bool {
    fn check(
        function: &Function,
        value: EntityId,
        expected: EntityId,
        visited: &mut std::collections::BTreeSet<EntityId>,
        found: &mut bool,
    ) -> bool {
        if value == expected {
            *found = true;
            return true;
        }
        if !visited.insert(value) {
            return true;
        }
        let Some(ed) = function.entity(value) else {
            return false;
        };
        let Definition::BlockParameter { block, index } = ed.definition else {
            return false;
        };
        let incoming = edges(function)
            .into_iter()
            .filter(|(_, edge)| edge.target == block)
            .collect::<Vec<_>>();
        !incoming.is_empty()
            && incoming
                .iter()
                .all(|(_, edge)| check(function, edge.arguments[index], expected, visited, found))
    }
    let mut visited = std::collections::BTreeSet::new();
    let mut found = false;
    check(function, value, expected, &mut visited, &mut found) && found
}
pub(in crate::ssa) fn check(program: &Program, live: bool, seed_span: lang_frontend::source::Span) {
    let f = program.modules[0]
        .functions
        .iter()
        .find(|f| {
            f.instructions.iter().any(|i| {
                i.origin == Origin::Source(seed_span)
                    && matches!(i.operation, Operation::ContainerConstruct { .. })
            })
        })
        .unwrap();
    let seeds = f
        .instructions
        .iter()
        .filter(|i| {
            i.origin == Origin::Source(seed_span)
                && matches!(i.operation, Operation::ContainerConstruct { .. })
        })
        .collect::<Vec<_>>();
    assert_eq!(seeds.len(), 1);
    let seed = seeds[0].results[0];
    let all_edges = edges(f);
    let entries = all_edges
        .iter()
        .filter_map(|(block, edge)| {
            edge.arguments
                .iter()
                .position(|arg| *arg == seed)
                .map(|slot| (*block, *edge, slot))
        })
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1);
    let (preheader, entry, owner_slot) = entries[0];
    let header = f.block(entry.target).unwrap();
    let lengths = f
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::ContainerLength { .. }))
        .collect::<Vec<_>>();
    let root_place = f.instructions.iter().find(|i| matches!(i.operation, Operation::RootPlace { owner } if EntityId::Value(owner) == seed)).unwrap();
    let source_begin = f.instructions.iter().find(|i| matches!(i.operation, Operation::BorrowBegin { place, .. } if EntityId::Place(place) == root_place.results[0])).unwrap();
    let source = source_begin.results[0];
    let snapshots = lengths
        .iter()
        .filter(|i| matches!(i.operation, Operation::ContainerLength { owner } if owner == source))
        .collect::<Vec<_>>();
    assert_eq!(
        snapshots.len(),
        1,
        "one length snapshot for the actual initial source loan"
    );
    let snapshot = snapshots[0];
    assert_eq!(snapshot.block, preheader);
    let source_slot = entry
        .arguments
        .iter()
        .position(|arg| *arg == source)
        .unwrap();
    let length_slot = entry
        .arguments
        .iter()
        .position(|arg| *arg == snapshot.results[0])
        .unwrap();
    let zero = f
        .instructions
        .iter()
        .find(|i| {
            i.block == preheader
                && matches!(i.operation, Operation::Constant(ScalarConstant::Integer(0)))
        })
        .unwrap()
        .results[0];
    let cursor_slot = entry.arguments.iter().position(|arg| *arg == zero).unwrap();
    let one = f
        .instructions
        .iter()
        .find(|i| {
            i.block == preheader
                && i.id.index() > snapshot.id.index()
                && matches!(i.operation, Operation::Constant(ScalarConstant::Integer(1)))
        })
        .unwrap()
        .results[0];
    let root_place = f.instructions.iter().find(|i| matches!(i.operation, Operation::RootPlace { owner } if EntityId::Value(owner) == seed)).unwrap();
    let source_begin = f
        .instructions
        .iter()
        .find(|i| i.results.contains(&source))
        .unwrap();
    assert!(
        matches!(source_begin.operation, Operation::BorrowBegin { place, .. } if EntityId::Place(place) == root_place.results[0])
    );
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("provider guard")
    };
    let body = f.block(when_true.target).unwrap();
    let exhausted = f.block(when_false.target).unwrap();
    let body_slot = |slot| {
        when_true
            .arguments
            .iter()
            .position(|arg| *arg == header.parameters[slot])
            .unwrap()
    };
    let exhausted_slot = |slot| {
        when_false
            .arguments
            .iter()
            .position(|arg| *arg == header.parameters[slot])
            .unwrap()
    };
    let body_owner = body.parameters[body_slot(owner_slot)];
    let body_source = body.parameters[body_slot(source_slot)];
    let body_cursor = body.parameters[body_slot(cursor_slot)];
    let exhausted_owner = exhausted.parameters[exhausted_slot(owner_slot)];
    let exhausted_source = exhausted.parameters[exhausted_slot(source_slot)];
    let place = f
        .instructions
        .iter()
        .find(|i| matches!(i.operation, Operation::ContainerElementPlace { owner, .. } if owner == body_source))
        .unwrap();
    assert!(
        matches!(place.operation, Operation::ContainerElementPlace { owner, index } if owner == body_source && EntityId::Value(index) == body_cursor)
    );
    let element = f.instructions.iter().find(|i| matches!(i.operation, Operation::BorrowBegin { place: p, .. } if EntityId::Place(p) == place.results[0])).unwrap().results[0];
    let back = all_edges
        .iter()
        .filter(|(block, edge)| edge.target == header.id && *block != preheader)
        .collect::<Vec<_>>();
    assert_eq!(
        back.len(),
        2,
        "continue and normal fallthrough both transport"
    );
    for (block, edge) in back {
        for (slot, expected) in [(owner_slot, body_owner), (source_slot, body_source)] {
            assert!(forwards(f, edge.arguments[slot], expected, 0));
        }
        assert!(
            forwards(
                f,
                edge.arguments[length_slot],
                header.parameters[length_slot],
                0
            ),
            "same immutable length snapshot"
        );
        let advance = f
            .instructions
            .iter()
            .find(|i| i.results.contains(&edge.arguments[cursor_slot]))
            .unwrap();
        assert_eq!(advance.block, *block);
        assert!(
            matches!(advance.operation, Operation::Binary { operator: BinaryOperator::Add, left, right }
            if forwards(f, EntityId::Value(left), body_cursor, 0) && forwards(f, EntityId::Value(right), one, 0))
        );
        let ends = f
            .instructions
            .iter()
            .filter(|i| i.block == *block && matches!(i.operation, Operation::BorrowEnd { .. }))
            .collect::<Vec<_>>();
        assert!(!ends.iter().any(|i| matches!(i.operation, Operation::BorrowEnd { loan } if forwards(f, EntityId::Loan(loan), body_source, 0))), "source loan survives both backedges");
        let ends = ends.into_iter().filter(|i| matches!(i.operation, Operation::BorrowEnd { loan } if forwards(f, EntityId::Loan(loan), element, 0))).collect::<Vec<_>>();
        assert_eq!(
            ends.len(),
            1,
            "the actual element ends once on each iteration path"
        );
        let ids = &f.block(*block).unwrap().instructions;
        assert!(
            ids.iter().position(|id| *id == ends[0].id).unwrap()
                < ids.iter().position(|id| *id == advance.id).unwrap()
        );
    }
    let source_end = f.instructions.iter().filter(|i| i.block == exhausted.id && matches!(i.operation, Operation::BorrowEnd { loan } if EntityId::Loan(loan) == exhausted_source)).collect::<Vec<_>>();
    assert_eq!(source_end.len(), 1);
    let drops = f.instructions.iter().filter(|i| matches!(i.operation, Operation::Drop { owner } if forwards(f, EntityId::Value(owner), exhausted_owner, 0))).collect::<Vec<_>>();
    assert_eq!(drops.len(), 1, "exact exhausted source owner consumed once");
    if live {
        let post = lengths.iter().find(|i| i.block == drops[0].block).unwrap();
        let Operation::ContainerLength { owner: post_loan } = post.operation else {
            unreachable!()
        };
        let begin = f
            .instructions
            .iter()
            .find(|i| i.results.contains(&post_loan))
            .unwrap();
        let Operation::BorrowBegin { place, .. } = begin.operation else {
            panic!("post-use source loan")
        };
        let root = f
            .instructions
            .iter()
            .find(|i| i.results.contains(&EntityId::Place(place)))
            .unwrap();
        assert!(
            matches!(root.operation, Operation::RootPlace { owner } if forwards(f, EntityId::Value(owner), exhausted_owner, 0))
        );
        assert_eq!(post.block, drops[0].block);
        let ids = &f.block(post.block).unwrap().instructions;
        assert!(
            ids.iter().position(|id| *id == post.id).unwrap()
                < ids.iter().position(|id| *id == drops[0].id).unwrap()
        );
    } else {
        assert_eq!(drops[0].block, exhausted.id);
        let ids = &exhausted.instructions;
        assert!(
            ids.iter().position(|id| *id == source_end[0].id).unwrap()
                < ids.iter().position(|id| *id == drops[0].id).unwrap()
        );
    }
}
}
