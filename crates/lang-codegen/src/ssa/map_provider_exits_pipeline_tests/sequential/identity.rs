//! Independent queries of the original AST/public FE products and the real SSA graph.
mod graph {
    use super::{Case, Failure, for_spans};
    use crate::ssa::{
        map_provider_exits_pipeline_tests::{
            fixture::{failure, need},
            query_graph::{Graph, Uses, loan, result, value},
            query_inputs::one,
        },
        model::*,
    };
    use lang_frontend::{
        parser::{Expression, Item, NameMarker, ParsedFile},
        source::SourceMap,
    };
    use std::collections::{BTreeMap, BTreeSet};

    // Follow every real predecessor/column. Instruction aliases cannot prove a parameter root;
    // each visited parameter must have a path to the separately selected source anchor.
    fn transported(
        g: &Graph<'_>,
        current: EntityId,
        anchor: EntityId,
    ) -> Result<BTreeSet<EntityId>, Failure> {
        let ty = g
            .function
            .entity(anchor)
            .ok_or_else(|| failure("anchor", anchor))?
            .ty;
        let mut parents = BTreeMap::new();
        let mut pending = vec![current];
        while let Some(entity) = pending.pop() {
            if entity == anchor || parents.contains_key(&entity) {
                continue;
            }
            let data = g
                .function
                .entity(entity)
                .ok_or_else(|| failure("transport entity", entity))?;
            let Definition::BlockParameter { block, index } = data.definition else {
                return Err(failure("transport must be a real parameter", data));
            };
            need(
                data.ty == ty && g.block(block)?.parameters.get(index) == Some(&entity),
                "transport original definition/type",
                data,
            )?;
            let incoming = g.incoming(block);
            need(
                !incoming.is_empty(),
                "transport cannot originate at another entry",
                entity,
            )?;
            let mut values = Vec::new();
            for (_, edge) in incoming {
                let argument = g.argument(edge, entity)?;
                values.push(argument);
                pending.push(argument)
            }
            parents.insert(entity, values);
        }
        let mut anchored = BTreeSet::from([anchor]);
        loop {
            let old = anchored.len();
            for (&entity, values) in &parents {
                if values.iter().any(|v| anchored.contains(v)) {
                    anchored.insert(entity);
                }
            }
            if old == anchored.len() {
                break;
            }
        }
        need(
            parents.keys().all(|v| anchored.contains(v)),
            "every incoming transport reaches original anchor",
            &parents,
        )?;
        Ok(anchored)
    }

    pub(in super::super) fn ssa(
        c: &Case,
        s: &SourceMap,
        p: &ParsedFile,
        program: &Program,
        unit: bool,
    ) -> Result<(), Failure> {
        let scan_span=one(p.roots().iter().filter_map(|id|{let n=p.ast().items().get(*id).ok()?;matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..} if s.slice(*name)==Ok("scan")).then_some(n.span())}),"original scan AST")?;
        let function = one(
            program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .filter(|f| f.origin == Origin::Source(scan_span)),
            "actual scan function",
        )?;
        let g = Graph::new(function)?;
        let module = program.module(function.id.module()).unwrap();
        let read_span=one(p.roots().iter().filter_map(|id|{let n=p.ast().items().get(*id).ok()?;matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..} if s.slice(*name)==Ok("read")).then_some(n.span())}),"original read declaration")?;
        let read_callee = one(
            module
                .functions
                .iter()
                .filter(|f| f.origin == Origin::Source(read_span)),
            "actual read callee",
        )?
        .id;
        let mut uses = Uses::default();
        let entry = g.block(function.entry_block().unwrap())?;
        need(
            entry.parameters.len() == c.parameters,
            "actual Borrow ABI parameters",
            entry,
        )?;
        let spans = for_spans(p);
        need(spans.len() == 2, "two original AST For", &spans)?;
        let mut roots = BTreeSet::new();
        let mut all_provider_ends = BTreeSet::new();
        let mut expected_map = BTreeSet::new();
        let mut map_types = BTreeSet::new();
        for (index, span) in spans.into_iter().enumerate() {
            let first = one(
                g.instructions.iter().copied().filter(|i| {
                    i.origin == Origin::Source(span)
                        && matches!(i.operation, Operation::MapIterationFirst { .. })
                }),
                "actual original First",
            )?;
            let Operation::MapIterationFirst { source } = first.operation else {
                unreachable!()
            };
            let birth = g.definition(EntityId::Loan(source))?;
            let Operation::SharedReborrow { source: seed } = birth.operation else {
                return Err(failure("actual Borrow source lease", birth));
            };
            uses.permit(EntityId::Loan(seed), birth);
            uses.protect(EntityId::Loan(source));
            uses.permit(EntityId::Loan(source), first);
            uses.protect(result(first)?);
            let anchor = entry.parameters[c.slots[index]];
            map_types.insert(function.entity(anchor).unwrap().ty.semantic_type());
            need(
                matches!(
                    function.entity(anchor).unwrap().ty,
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        ..
                    }
                ),
                "Borrow Map ABI",
                anchor,
            )?;
            roots.extend(transported(&g, EntityId::Loan(seed), anchor)?);
            need(
                birth.block == first.block && g.position(birth)? < g.position(first)?,
                "current source lease before First",
                (birth, first),
            )?;
            if unit && index == 1 {
                need(
                    EntityId::Loan(seed) != anchor
                        && matches!(function.entity(EntityId::Loan(seed)).unwrap().definition,Definition::BlockParameter{block,..} if block!=entry.id),
                    "second provider consumes current CFG view, not entry loan",
                    seed,
                )?;
            }
            let guard = one(
                g.instructions.iter().copied().filter(|i| {
                    i.origin == Origin::Source(span)
                        && matches!(i.operation, Operation::MapIterationHasNext { .. })
                }),
                "original HasNext",
            )?;
            let preheader = g.branch(first.block)?;
            let Operation::MapIterationHasNext {
                source: hs,
                cursor: hc,
            } = guard.operation
            else {
                unreachable!()
            };
            for entity in [EntityId::Loan(hs), EntityId::Value(hc)] {
                uses.protect(entity);
                uses.permit(entity, guard);
            }
            need(
                preheader.target == guard.block
                    && g.transported(preheader, EntityId::Loan(source))? == EntityId::Loan(hs)
                    && g.transported(preheader, result(first)?)? == EntityId::Value(hc),
                "actual First-to-header source/cursor columns",
                preheader,
            )?;
            let TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } = &g.term(guard.block)?.kind
            else {
                return Err(failure("provider guard CFG", guard));
            };
            need(
                *condition == value(guard)?,
                "actual guard condition",
                condition,
            )?;
            let key = one(
                g.instructions.iter().copied().filter(|i| {
                    i.origin == Origin::Source(span)
                        && matches!(i.operation, Operation::MapIterationKeyLoan { .. })
                }),
                "original key",
            )?;
            let val = one(
                g.instructions.iter().copied().filter(|i| {
                    i.origin == Origin::Source(span)
                        && matches!(i.operation, Operation::MapIterationValueLoan { .. })
                }),
                "original value",
            )?;
            let k = loan(key)?;
            let v = loan(val)?;
            let bs = g.transported(when_true, EntityId::Loan(hs))?;
            let bc = g.transported(when_true, EntityId::Value(hc))?;
            need(
                key.block == when_true.target
                    && val.block == key.block
                    && matches!(key.operation,Operation::MapIterationKeyLoan{source,cursor} if EntityId::Loan(source)==bs&&EntityId::Value(cursor)==bc)
                    && matches!(val.operation,Operation::MapIterationValueLoan{source,cursor} if EntityId::Loan(source)==bs&&EntityId::Value(cursor)==bc),
                "exact body provider identity",
                (key, val),
            )?;
            let builtin = index == 1 && c.label.ends_with("-int-key");
            let read_call = one(
                p.ast().expressions().iter().filter_map(|(_, node)| {
                    let Expression::Call { callee, .. } = node.payload() else {
                        return None;
                    };
                    let callee = p.ast().expressions().get(*callee).ok()?;
                    (node.span().start() >= span.start()
                        && node.span().end() <= span.end()
                        && s.slice(callee.span()) == Ok(if builtin { "println" } else { "read" }))
                    .then_some(node.span())
                }),
                "original component read call",
            )?;
            let read=one(g.ops(key.block)?.into_iter().filter(|i| i.origin==Origin::Source(read_call)
                && if builtin{matches!(i.operation,Operation::PrintString{value} if value==v)}else{
                    matches!(&i.operation,Operation::DirectCall{callee,receiver:None,arguments} if *callee==read_callee&&arguments.as_slice()==[EntityId::Loan(k),EntityId::Loan(v)])
                }),"actual source callee and component operands")?;
            for entity in [bs, bc] {
                uses.protect(entity);
                uses.permit(entity, key);
                uses.permit(entity, val);
            }
            for l in [k, v] {
                uses.protect(EntityId::Loan(l));
            }
            if !builtin {
                uses.permit(EntityId::Loan(k), read);
            }
            uses.permit(EntityId::Loan(v), read);
            let ends = [v, k].map(|l| {
                one(
                    g.instructions
                        .iter()
                        .copied()
                        .filter(|i| i.operation == Operation::BorrowEnd { loan: l }),
                    "unique real component End",
                )
            });
            let [ve, ke] = ends;
            let (ve, ke) = (ve?, ke?);
            uses.permit(EntityId::Loan(v), ve);
            uses.permit(EntityId::Loan(k), ke);
            need(
                ve.block == key.block
                    && ke.block == key.block
                    && g.position(read)? < g.position(ve)?
                    && g.position(ve)? < g.position(ke)?,
                "read before value then key End",
                (read, ve, ke),
            )?;
            let breaks = index == 0 && c.label.ends_with("-break");
            let next_span = if index == 0 && c.label.ends_with("-continue") {
                one(
                    p.ast().expressions().iter().filter_map(|(_, node)| {
                        (matches!(node.payload(), Expression::Continue { .. })
                            && node.span().start() >= span.start()
                            && node.span().end() <= span.end())
                        .then_some(node.span())
                    }),
                    "original Continue transfer span",
                )?
            } else {
                span
            };
            let nexts = g
                .instructions
                .iter()
                .copied()
                .filter(|i| {
                    i.origin == Origin::Source(next_span)
                        && matches!(i.operation, Operation::MapIterationNext { .. })
                })
                .collect::<Vec<_>>();
            need(
                nexts.len() == usize::from(!breaks),
                "Next only on live backedge",
                &nexts,
            )?;
            if let Some(next) = nexts.first() {
                uses.permit(bs, next);
                uses.permit(bc, next);
                uses.protect(result(next)?);
                need(
                    next.block == key.block
                        && g.position(ke)? < g.position(next)?
                        && matches!(next.operation,Operation::MapIterationNext{source,cursor} if EntityId::Loan(source)==bs&&EntityId::Value(cursor)==bc),
                    "component ends before same-provider Next",
                    next,
                )?;
                let back = g.branch(key.block)?;
                need(back.target == guard.block, "actual backedge", back)?;
                let back_source = g.argument(back, EntityId::Loan(hs))?;
                let back_cursor = g.argument(back, EntityId::Value(hc))?;
                need(
                    back_source == bs && back_cursor == result(next)?,
                    "complete backedge source/cursor",
                    back,
                )?;
            }
            need(
                g.incoming(guard.block).len() == if breaks { 1 } else { 2 },
                "all actual header incoming edges",
                guard.block,
            )?;
            let exhausted = g.transported(when_false, EntityId::Loan(hs))?;
            uses.protect(exhausted);
            uses.protect(g.transported(when_false, EntityId::Value(hc))?);
            let source_end=one(g.ops(when_false.target)?.into_iter().filter(|i|matches!(i.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==exhausted)),"exact exhausted provider End")?;
            all_provider_ends.insert(source_end.id);
            uses.permit(exhausted, source_end);
            expected_map.extend(transported(&g, exhausted, EntityId::Loan(source))?);
            expected_map.extend(transported(&g, bs, EntityId::Loan(source))?);
            if breaks {
                let end=one(g.ops(key.block)?.into_iter().filter(|i|matches!(i.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==bs)),"exact break provider End")?;
                need(
                    g.position(ke)? < g.position(end)?,
                    "break K/V before provider End",
                    end,
                )?;
                all_provider_ends.insert(end.id);
                uses.permit(bs, end);
            }
            if index == 0 && (breaks || c.label.ends_with("-continue")) {
                let drop = one(
                    g.ops(key.block)?
                        .into_iter()
                        .filter(|i| matches!(i.operation, Operation::Drop { .. })),
                    "original Guard cleanup",
                )?;
                need(
                    g.position(read)? < g.position(drop)? && g.position(drop)? < g.position(ve)?,
                    "body-local owner before component End",
                    drop,
                )?;
            }
        }
        // Include all current caller views, even columns that are unused in this body.
        for &block in &g.blocks {
            for &entity in &g.block(block)?.parameters {
                for &anchor in &entry.parameters {
                    if let Ok(aliases) = transported(&g, entity, anchor) {
                        roots.extend(aliases);
                    }
                }
            }
        }
        for &root in &roots {
            uses.protect(root);
        }
        expected_map.extend(roots.iter().copied());
        crate::ssa::map_provider_exits_pipeline_tests::query_inventory::check(
            &g,
            &map_types.into_iter().collect::<Vec<_>>(),
            &expected_map,
        )?;
        uses.check(&g, None)?;
        for i in &g.instructions {
            if let Operation::BorrowEnd { loan } = i.operation {
                need(
                    !roots.contains(&EntityId::Loan(loan)),
                    "callee never ends caller root/view",
                    i,
                )?;
                if matches!(function.entity(EntityId::Loan(loan)).unwrap().ty,EntityType::Loan{target,..} if matches!(module.type_kind(target),Some(SsaTypeKind::MapContainer{..})))
                {
                    need(
                        all_provider_ends.contains(&i.id),
                        "every Map End is an exact provider exit",
                        i,
                    )?;
                }
            }
        }
        Ok(())
    }
}
use super::{Case, Failure};
use crate::ssa::map_provider_exits_pipeline_tests::{
    fixture::{failure, need},
    query_inputs::one,
};
pub(super) use graph::ssa;
use lang_frontend::{
    name_resolution::{Namespace, UnitReferenceTarget, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, UnitDropTarget, UnitIterationCleanupAction as UA,
        UnitIterationExitKind as UE, UnitIterationSourceAccess, UnitLoanTarget,
    },
    parser::{ParsedFile, Statement},
    source::SourceMap,
    type_checking::{CompilationUnitTypes, ParameterMode},
};

pub(super) fn unit(
    c: &Case,
    s: &SourceMap,
    p: &ParsedFile,
    n: &ValidatedCompilationUnitNames,
    t: &CompilationUnitTypes,
    o: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let original = one(n.names().source_units().iter(), "unit original source")?;
    let unit = original.source_unit();
    let mut descriptors = t.map_iterations().iter().collect::<Vec<_>>();
    descriptors.sort_by_key(|d| {
        p.ast()
            .statements()
            .get(d.statement().statement())
            .unwrap()
            .span()
            .start()
    });
    need(
        descriptors.len() == 2 && o.map_iterations().len() == 2,
        "unit complete two plans",
        &descriptors,
    )?;
    for (index, d) in descriptors.into_iter().enumerate() {
        need(
            d.statement().source_unit() == unit,
            "unit original For source",
            d,
        )?;
        let plan = o
            .map_iteration(d.statement())
            .ok_or_else(|| failure("unit plan", d))?;
        let Some(UnitLoanTarget::Place(place)) = plan.source() else {
            return Err(failure("unit direct Borrow source", plan));
        };
        let root = place.root();
        let source = p
            .ast()
            .expressions()
            .get(d.source().expression())
            .unwrap()
            .span();
        let expected = if c.slots[index] == 0 {
            "first"
        } else {
            "second"
        };
        let symbol = one(
            original
                .resolution()
                .symbols()
                .iter()
                .filter(|v| v.id() == root.symbol()),
            "unit root symbol",
        )?;
        let reference = one(
            n.names().references().iter().filter(|r| {
                r.source_unit() == unit
                    && r.span() == source
                    && r.namespace() == Some(Namespace::Value)
            }),
            "unit original reference",
        )?;
        let (signature, slot) = one(
            t.signatures()
                .declarations()
                .iter()
                .filter_map(|v| v.callable())
                .flat_map(|f| {
                    f.parameters()
                        .iter()
                        .enumerate()
                        .filter_map(move |(slot, v)| {
                            (v.symbol() == Some(root)).then_some((f, slot))
                        })
                }),
            "unit parameter slot",
        )?;
        need(
            place.is_root()
                && root.source_unit() == unit
                && plan.source_access() == UnitIterationSourceAccess::Shared
                && s.slice(source) == Ok(expected)
                && symbol.name() == expected
                && matches!(reference.target(),UnitReferenceTarget::Symbol(id) if *id==root)
                && slot == c.slots[index]
                && signature.parameters().len() == c.parameters
                && signature.parameters()[slot].mode() == ParameterMode::Borrow
                && !o
                    .drops()
                    .iter()
                    .any(|drop| drop.target() == UnitDropTarget::Named(root)),
            "unit exact Borrow root/slot/no owner drop",
            plan,
        )?;
        need(
            plan.descriptor() == d
                && plan.loans().iter().all(|l| {
                    l.statement() == d.statement()
                        && l.source() == d.source()
                        && Some(l.origin()) == plan.source()
                }),
            "unit same provider component origins",
            plan,
        )?;
        need(plan.exits().len() == 2, "unit complete exit count", plan)?;
        for exit in plan.exits() {
            let actual = exit
                .actions()
                .iter()
                .map(|a| match a {
                    UA::EndBinding { statement, symbol }
                        if *statement == d.statement() && *symbol == d.value_binding() =>
                    {
                        "value"
                    }
                    UA::EndBinding { statement, symbol }
                        if *statement == d.statement() && *symbol == d.key_binding() =>
                    {
                        "key"
                    }
                    UA::FinishProvider(id) if *id == d.statement() => "finish",
                    UA::EndSource(id) if *id == d.statement() => "end",
                    UA::Drop(_) => "drop",
                    _ => "unexpected",
                })
                .collect::<Vec<_>>();
            need(
                actual == cleanup(c, index, exit.kind() == UE::Exhaustion),
                "unit exact cleanup actions",
                (exit, actual),
            )?;
        }
    }
    Ok(())
}

fn cleanup(c: &Case, index: usize, exhaustion: bool) -> Vec<&'static str> {
    if exhaustion {
        return vec!["finish", "end"];
    }
    let mut expected = Vec::new();
    if index == 0 && (c.label.ends_with("-break") || c.label.ends_with("-continue")) {
        expected.push("drop")
    }
    expected.extend(["value", "key"]);
    if index == 0 && c.label.ends_with("-break") {
        expected.extend(["finish", "end"])
    }
    expected
}

pub(super) fn for_spans(p: &ParsedFile) -> Vec<lang_frontend::source::Span> {
    let mut spans = p
        .ast()
        .statements()
        .iter()
        .filter_map(|(_, n)| matches!(n.payload(), Statement::For { .. }).then_some(n.span()))
        .collect::<Vec<_>>();
    spans.sort_by_key(|s| s.start());
    spans
}
