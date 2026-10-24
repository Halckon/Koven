//! Independent all-incoming identities, followed by a finite CFG lifetime proof.
mod calls;
mod paths;
use super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, result},
    query_inputs::one,
};
use super::facts::{self, Inputs, Site, Source};
use crate::ssa::model::*;
use lang_frontend::{
    parser::{Expression, Item, NameMarker, ParsedFile},
    source::{SourceMap, Span},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
struct Layer<'a> {
    index: usize,
    site: &'a Site,
    first: &'a Instruction,
    has: &'a Instruction,
    key: &'a Instruction,
    val: &'a Instruction,
    next: Vec<&'a Instruction>,
    birth: &'a Instruction,
    source: BTreeSet<EntityId>,
    cursors: BTreeSet<EntityId>,
    components: [BTreeSet<EntityId>; 2],
    owner: Option<EntityId>,
    owners: BTreeSet<EntityId>,
}
#[derive(Debug)]
struct Guard<'a> {
    marker: Span,
    layer: usize,
    birth: &'a Instruction,
    owners: BTreeSet<EntityId>,
}

pub(super) fn check(
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
    inputs: &Inputs,
    unit: bool,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "stack actual module")?;
    let mut functions = Vec::new();
    for site in &inputs.sites {
        if !functions.contains(&site.ast.function) {
            functions.push(site.ast.function);
        }
    }
    for span in functions {
        let f = function(module, span)?;
        let g = Graph::with_abort(f)?;
        let mut uses = Uses::default();
        let mut layers = Vec::new();
        for (index, site) in inputs
            .sites
            .iter()
            .enumerate()
            .filter(|(_, s)| s.ast.function == span)
        {
            layers.push(provider(module, &g, index, site, &mut uses)?);
        }
        need(
            g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapIterationFirst { .. }))
                .count()
                == layers.len(),
            "stack every actual provider is bound to original descriptor",
            &layers,
        )?;
        for (i, a) in layers.iter().enumerate() {
            for b in &layers[i + 1..] {
                need(
                    a.source.is_disjoint(&b.source)
                        && a.cursors.is_disjoint(&b.cursors)
                        && a.components
                            .iter()
                            .all(|x| b.components.iter().all(|y| x.is_disjoint(y))),
                    "stack independent per-layer leases/cursors/components even for shared root",
                    (a.index, b.index),
                )?;
            }
        }
        let guards = guards(module, &g, inputs, &layers, &mut uses)?;
        for layer in &layers {
            for &marker in &layer.site.owner_markers {
                need(guards.iter().any(|g|g.marker==marker) || layers.iter().any(|l|matches!(l.site.source,Source::Parameter{marker:m,borrow:false,..} if m==marker) && l.owner.is_some()),
                    "stack observed FE owner root resolves to exact SSA source/Guard birth", (layer.index,marker))?;
            }
        }
        calls::provider_calls(parsed, module, &g, inputs, &layers, &mut uses)?;
        paths::check(parsed, &g, inputs, &layers, &guards, unit)?;
        let mut expected = BTreeSet::new();
        let mut map_types = BTreeSet::new();
        for layer in &layers {
            expected.extend(layer.owners.iter().copied());
            expected.extend(layer.source.iter().copied());
            map_types.insert(f.entity(result(layer.birth)?).unwrap().ty.semantic_type());
            if let Source::Parameter {
                slot, borrow: true, ..
            } = layer.site.source
            {
                let seed = g.block(f.entry_block().unwrap())?.parameters[slot];
                expected.extend(family(&g, &[seed])?);
            }
            if let Operation::BorrowBegin { place, .. } = layer.birth.operation {
                // root_birth independently proved this exact provider acquisition.
                // Do not absorb unrelated RootPlace results merely by matching a type/owner.
                let place = EntityId::Place(place);
                need(
                    !g.edges
                        .iter()
                        .any(|(_, edge)| edge.arguments.contains(&place)),
                    "stack original provider RootPlace remains local to its actual current owner",
                    layer.birth,
                )?;
                expected.insert(place);
            }
        }
        crate::ssa::map_provider_exits_pipeline_tests::query_inventory::check(
            &g,
            &map_types.into_iter().collect::<Vec<_>>(),
            &expected,
        )?;
        uses.check(&g, None)?;
    }
    calls::callers(sources, parsed, module, inputs)?;
    Ok(())
}

fn family(g: &Graph<'_>, anchors: &[EntityId]) -> Result<BTreeSet<EntityId>, Failure> {
    let mut all = anchors.iter().copied().collect::<BTreeSet<_>>();
    loop {
        let before = all.len();
        for (_, edge) in &g.edges {
            for (&arg, &parameter) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                if all.contains(&arg) {
                    all.insert(parameter);
                }
            }
        }
        if all.len() == before {
            break;
        }
    }
    for &entity in &all {
        g.anchored(entity, anchors)?;
    }
    Ok(all)
}
fn loan_family(g: &Graph<'_>, anchor: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
    let mut anchors = vec![anchor];
    loop {
        let all = family(g, &anchors)?;
        let before = anchors.len();
        for i in &g.instructions {
            if matches!(i.operation,Operation::SharedReborrow{source} if all.contains(&EntityId::Loan(source)))
            {
                let child = result(i)?;
                need(
                    g.function.entity(child).unwrap().ty
                        == g.function
                            .entity(EntityId::Loan(match i.operation {
                                Operation::SharedReborrow { source } => source,
                                _ => unreachable!(),
                            }))
                            .unwrap()
                            .ty,
                    "stack child Shared ABI preserves represented parent",
                    i,
                )?;
                if !anchors.contains(&child) {
                    anchors.push(child);
                }
            }
        }
        if before == anchors.len() {
            return Ok(all);
        }
    }
}
fn at(i: &Instruction, span: Span) -> bool {
    i.origin == Origin::Source(span)
}
fn provider<'a>(
    module: &Module,
    g: &Graph<'a>,
    index: usize,
    site: &'a Site,
    uses: &mut Uses,
) -> Result<Layer<'a>, Failure> {
    let first = one(
        g.instructions.iter().copied().filter(|i| {
            at(i, site.ast.span) && matches!(i.operation, Operation::MapIterationFirst { .. })
        }),
        "stack original First",
    )?;
    let has = one(
        g.instructions.iter().copied().filter(|i| {
            at(i, site.ast.span) && matches!(i.operation, Operation::MapIterationHasNext { .. })
        }),
        "stack original HasNext",
    )?;
    let key = one(
        g.instructions.iter().copied().filter(|i| {
            at(i, site.ast.span) && matches!(i.operation, Operation::MapIterationKeyLoan { .. })
        }),
        "stack original key reader",
    )?;
    let val = one(
        g.instructions.iter().copied().filter(|i| {
            at(i, site.ast.span) && matches!(i.operation, Operation::MapIterationValueLoan { .. })
        }),
        "stack original value reader",
    )?;
    let next = g
        .instructions
        .iter()
        .copied()
        .filter(|i| {
            at(i, site.ast.span) && matches!(i.operation, Operation::MapIterationNext { .. })
        })
        .collect::<Vec<_>>();
    need(!next.is_empty(), "stack actual normal backedge Next", site)?;
    let Operation::MapIterationFirst { source: source0 } = first.operation else {
        unreachable!()
    };
    let birth = g.definition(EntityId::Loan(source0))?;
    let entry = g.block(g.function.entry_block().unwrap())?;
    let (owner, owners) = match site.source {
        Source::Parameter { slot, borrow, .. } => {
            let seed = *entry
                .parameters
                .get(slot)
                .ok_or_else(|| failure("stack exact parameter slot", site))?;
            if borrow {
                let Operation::SharedReborrow { source } = birth.operation else {
                    return Err(failure(
                        "stack Borrow parameter actual provider reborrow",
                        birth,
                    ));
                };
                need(
                    loan_family(g, seed)?.contains(&EntityId::Loan(source)),
                    "stack Borrow provider descends through actual reborrows from original parameter",
                    birth,
                )?;
                let caller = family(g, &[seed])?;
                for &id in &caller {
                    uses.protect(id);
                }
                for i in &g.instructions {
                    if matches!(i.operation,Operation::SharedReborrow{source} if caller.contains(&EntityId::Loan(source)))
                    {
                        uses.permit(
                            EntityId::Loan(match i.operation {
                                Operation::SharedReborrow { source } => source,
                                _ => unreachable!(),
                            }),
                            i,
                        );
                    }
                }
                need(!g.instructions.iter().any(|i|matches!(i.operation,Operation::BorrowEnd{loan} if caller.contains(&EntityId::Loan(loan)))),"stack callee never ends original caller loan",site)?;
                (None, BTreeSet::new())
            } else {
                let roots = family(g, &[seed])?;
                root_birth(g, birth, &roots)?;
                (Some(seed), roots)
            }
        }
        Source::Call {
            span,
            callee,
            arity,
        } => {
            let factory = function(module, callee)?;
            let call=one(g.instructions.iter().copied().filter(|i|at(i,span)&&matches!(&i.operation,Operation::DirectCall{callee,receiver:None,arguments} if *callee==factory.id&&arguments.len()==arity)),"stack temporary exact selected callee/once evaluation")?;
            let seed = result(call)?;
            let roots = family(g, &[seed])?;
            root_birth(g, birth, &roots)?;
            (Some(seed), roots)
        }
    };
    let source = family(g, &[EntityId::Loan(source0)])?;
    let mut cursor_anchors = vec![result(first)?];
    for n in &next {
        cursor_anchors.push(result(n)?);
    }
    let cursors = family(g, &cursor_anchors)?;
    let (hs, hc) = match has.operation {
        Operation::MapIterationHasNext { source, cursor } => (source, cursor),
        _ => unreachable!(),
    };
    let ty = g.function.entity(EntityId::Loan(hs)).unwrap().ty;
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target: map,
    } = ty
    else {
        return Err(failure("stack actual Shared provider", ty));
    };
    let (kind, kt, vt) = module
        .map_container(map)
        .ok_or_else(|| failure("stack actual Map representation", map))?;
    need(
        kind == if site.mutable {
            MapContainerKind::MutableMap
        } else {
            MapContainerKind::Map
        } && module.type_kind(kt) == Some(&SsaTypeKind::StringOwner)
            && !site.key_type.is_empty()
            && !site.value_type.is_empty(),
        "stack original FE Map kind and represented K/V",
        site,
    )?;
    represented_value(module, vt, &site.representation)?;
    let components = [loan_family(g, result(key)?)?, loan_family(g, result(val)?)?];
    need(
        components[0].is_disjoint(&components[1]),
        "stack K/V cannot exchange identity",
        site,
    )?;
    for (reader, target) in [(key, kt), (val, vt)] {
        need(
            g.function.entity(result(reader)?).unwrap().ty
                == EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                },
            "stack exact represented reader Shared ABI",
            reader,
        )?;
    }
    for i in [has, key, val].into_iter().chain(next.iter().copied()) {
        let (s, c) = match i.operation {
            Operation::MapIterationHasNext { source, cursor }
            | Operation::MapIterationKeyLoan { source, cursor }
            | Operation::MapIterationValueLoan { source, cursor }
            | Operation::MapIterationNext { source, cursor } => (source, cursor),
            _ => unreachable!(),
        };
        need(
            source.contains(&EntityId::Loan(s)) && cursors.contains(&EntityId::Value(c)),
            "stack actual same-layer source/cursor operands",
            i,
        )?;
        if i.id == key.id || i.id == val.id {
            let (ks, kc) = match key.operation {
                Operation::MapIterationKeyLoan { source, cursor } => (source, cursor),
                _ => unreachable!(),
            };
            need(
                s == ks
                    && c == kc
                    && key.block == val.block
                    && g.position(key)? < g.position(val)?,
                "stack exact paired body current cursor",
                i,
            )?;
        }
        uses.permit(EntityId::Loan(s), i);
        uses.permit(EntityId::Value(c), i);
    }
    need(
        g.function.entity(result(first)?).unwrap().ty
            == g.function.entity(EntityId::Value(hc)).unwrap().ty
            && matches!(
                module.type_kind(
                    g.function
                        .entity(result(first)?)
                        .unwrap()
                        .ty
                        .semantic_type()
                ),
                Some(SsaTypeKind::Integer {
                    bits: 64,
                    signed: false
                })
            ),
        "stack complete cursor UInt64 representation",
        first,
    )?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        ..
    } = &g.term(has.block)?.kind
    else {
        return Err(failure("stack actual provider condition", has));
    };
    need(
        EntityId::Value(*condition) == result(has)? && when_true.target == key.block,
        "stack HasNext actual true body edge",
        has,
    )?;
    let (ks, kc) = match key.operation {
        Operation::MapIterationKeyLoan { source, cursor } => {
            (EntityId::Loan(source), EntityId::Value(cursor))
        }
        _ => unreachable!(),
    };
    for (body, header) in [(ks, EntityId::Loan(hs)), (kc, EntityId::Value(hc))] {
        need(
            body == header || g.argument(when_true, body)? == header,
            "stack actual true-edge source/cursor columns",
            when_true,
        )?;
    }
    let mut expected = vec![first.block];
    expected.extend(next.iter().map(|n| n.block));
    g.incoming_from(has.block, &expected)?;
    for (from, edge) in g.incoming(has.block) {
        let expected_cursor = if from == first.block {
            result(first)?
        } else {
            result(one(
                next.iter().copied().filter(|n| n.block == from),
                "stack exact Next predecessor",
            )?)?
        };
        need(
            g.argument(edge, EntityId::Value(hc))? == expected_cursor
                && source.contains(&g.argument(edge, EntityId::Loan(hs))?),
            "stack every header incoming cursor/source actual column",
            edge,
        )?;
    }
    for &id in source
        .iter()
        .chain(cursors.iter())
        .chain(components.iter().flat_map(|c| c.iter()))
    {
        uses.protect(id);
    }
    uses.permit(EntityId::Loan(source0), first);
    for i in &g.instructions {
        match i.operation {
            Operation::BorrowEnd { loan }
                if source.contains(&EntityId::Loan(loan))
                    || components.iter().any(|c| c.contains(&EntityId::Loan(loan))) =>
            {
                uses.permit(EntityId::Loan(loan), i)
            }
            Operation::SharedReborrow { source: s }
                if components.iter().any(|c| c.contains(&EntityId::Loan(s))) =>
            {
                uses.permit(EntityId::Loan(s), i)
            }
            Operation::RootPlace { owner } if owners.contains(&EntityId::Value(owner)) => {
                uses.permit(EntityId::Value(owner), i)
            }
            Operation::Drop { owner } if owners.contains(&EntityId::Value(owner)) => {
                uses.permit(EntityId::Value(owner), i)
            }
            _ => {}
        }
    }
    for &id in &owners {
        uses.protect(id);
    }
    Ok(Layer {
        index,
        site,
        first,
        has,
        key,
        val,
        next,
        birth,
        source,
        cursors,
        components,
        owner,
        owners,
    })
}
fn root_birth(
    g: &Graph<'_>,
    birth: &Instruction,
    owners: &BTreeSet<EntityId>,
) -> Result<(), Failure> {
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = birth.operation
    else {
        return Err(failure("stack owned provider Shared BorrowBegin", birth));
    };
    let root = g.definition(EntityId::Place(place))?;
    need(
        matches!(root.operation,Operation::RootPlace{owner} if owners.contains(&EntityId::Value(owner))),
        "stack exact actual current owner RootPlace",
        root,
    )
}
fn represented_value(
    module: &Module,
    ty: SsaTypeId,
    original: &facts::Representation,
) -> Result<(), Failure> {
    let inner = match (original.nullable, module.type_kind(ty)) {
        (true, Some(SsaTypeKind::NullableHandle { inner })) => *inner,
        (false, Some(SsaTypeKind::NullableHandle { .. })) | (true, _) => {
            return Err(failure(
                "stack actual nullable component representation",
                (ty, original),
            ));
        }
        (false, _) => ty,
    };
    let Some(deinit) = original.deinit else {
        return need(
            module.type_kind(inner) == Some(&SsaTypeKind::StringOwner),
            "stack original FE String maps to actual String owner",
            inner,
        );
    };
    let Some(SsaTypeKind::HeapOwner { payload, .. }) = module.type_kind(inner) else {
        return Err(failure("stack original Resource maps to heap owner", inner));
    };
    fn strings(module: &Module, ty: SsaTypeId) -> Result<usize, Failure> {
        match module.type_kind(ty) {
            Some(SsaTypeKind::StringOwner) => Ok(1),
            Some(SsaTypeKind::Aggregate { fields, .. }) => {
                fields.iter().map(|&t| strings(module, t)).sum()
            }
            Some(SsaTypeKind::ZeroSized { .. }) => Ok(0),
            _ => Err(failure(
                "stack original Resource payload layout",
                module.type_kind(ty),
            )),
        }
    }
    need(
        module
            .deinits
            .get(&inner)
            .and_then(|id| module.function(*id))
            .is_some_and(|f| f.origin == Origin::Source(deinit))
            && payload
                .map(|t| strings(module, t))
                .transpose()?
                .unwrap_or(0)
                == original.strings,
        "stack actual Resource payload/String fields and original same-FE deinit identity",
        (inner, original),
    )
}
fn guards<'a>(
    module: &Module,
    g: &Graph<'a>,
    inputs: &Inputs,
    layers: &[Layer<'a>],
    uses: &mut Uses,
) -> Result<Vec<Guard<'a>>, Failure> {
    let mut guards = Vec::new();
    for original in &inputs.guards {
        let Some(layer) = layers
            .iter()
            .filter(|l| facts::inside(original.initializer, l.site.ast.body_span))
            .min_by_key(|l| l.site.ast.body_span.end() - l.site.ast.body_span.start())
        else {
            continue;
        };
        let marker = original.marker;
        let deinit = original.deinit;
        let birth = one(
            g.instructions.iter().copied().filter(|i| {
                at(i, original.initializer) && matches!(i.operation, Operation::HeapAllocate { .. })
            }),
            "stack Guard actual original allocation",
        )?;
        let ty = g
            .function
            .entity(result(birth)?)
            .unwrap()
            .ty
            .semantic_type();
        need(
            module
                .deinits
                .get(&ty)
                .and_then(|id| module.function(*id))
                .is_some_and(|f| f.origin == Origin::Source(deinit)),
            "stack Guard actual allocation type and original deinit",
            birth,
        )?;
        let owners = family(g, &[result(birth)?])?;
        for &id in &owners {
            uses.protect(id);
        }
        for i in &g.instructions {
            if matches!(i.operation,Operation::Drop{owner} if owners.contains(&EntityId::Value(owner)))
            {
                uses.permit(
                    EntityId::Value(match i.operation {
                        Operation::Drop { owner } => owner,
                        _ => unreachable!(),
                    }),
                    i,
                );
            }
        }
        guards.push(Guard {
            marker,
            layer: layer.index,
            birth,
            owners,
        });
    }
    Ok(guards)
}
