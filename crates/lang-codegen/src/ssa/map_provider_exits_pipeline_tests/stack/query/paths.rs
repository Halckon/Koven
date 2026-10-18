//! Enumerate finite abstract CFG states; backedges close only with identical live roles.
use super::super::facts::{Action, Cleanup};
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    phase: Vec<u8>, // inactive, provider, K, KV, V-ended, K-ended
    sources: Vec<Option<EntityId>>,
    cursors: Vec<Option<EntityId>>,
    components: Vec<[Option<EntityId>; 2]>,
    owners: Vec<Option<EntityId>>,
    guards: Vec<Option<EntityId>>,
    loans: BTreeMap<EntityId, Option<EntityId>>,
    stack: Vec<usize>,
    events: Vec<Action>,
    returned: Option<(usize, EntityId)>,
    end_origins: Vec<(usize, Origin)>,
}
struct ReturnSite {
    span: Span,
    value: Span,
}

pub(super) fn check(
    parsed: &ParsedFile,
    g: &Graph<'_>,
    inputs: &Inputs,
    layers: &[Layer<'_>],
    guards: &[Guard<'_>],
    unit: bool,
) -> Result<(), Failure> {
    let mut roots = Vec::new();
    for l in layers {
        if let Some(seed) = l.owner {
            if !roots.contains(&seed) {
                roots.push(seed);
            }
        }
    }
    let entry = g.function.entry_block().unwrap();
    let parameters = &g.block(entry)?.parameters;
    let owners = roots
        .iter()
        .map(|id| parameters.contains(id).then_some(*id))
        .collect();
    let start = State {
        phase: vec![0; layers.len()],
        sources: vec![None; layers.len()],
        cursors: vec![None; layers.len()],
        components: vec![[None; 2]; layers.len()],
        owners,
        guards: vec![None; guards.len()],
        loans: BTreeMap::new(),
        stack: Vec::new(),
        events: Vec::new(),
        returned: None,
        end_origins: Vec::new(),
    };
    let returns = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, n)| {
            let Expression::Return {
                value: Some(value), ..
            } = n.payload()
            else {
                return None;
            };
            layers
                .iter()
                .any(|l| facts::inside(n.span(), l.site.ast.body_span))
                .then(|| ReturnSite {
                    span: n.span(),
                    value: parsed.ast().expressions().get(*value).unwrap().span(),
                })
        })
        .collect::<Vec<_>>();
    let mut pending = vec![(entry, start)];
    let mut visited = Vec::new();
    let mut finite_returns = 0;
    while let Some((block, mut state)) = pending.pop() {
        if visited.contains(&(block, state.clone())) {
            continue;
        }
        visited.push((block, state.clone()));
        need(
            visited.len() < 100_000,
            "stack abstract lifetime graph reaches finite fixed point",
            visited.len(),
        )?;
        for i in g.ops(block)? {
            let is_event = matches!(
                i.operation,
                Operation::Drop { .. }
                    | Operation::BorrowEnd { .. }
                    | Operation::MapIterationNext { .. }
            );
            if !is_event && state.returned.is_none() && !state.events.is_empty() {
                finish(&mut state, inputs, None, layers, unit)?;
            }
            for (r, site) in returns.iter().enumerate() {
                if at(i, site.value) && i.results.len() == 1 {
                    need(
                        state.returned.is_none() && state.events.is_empty(),
                        "stack actual Return operand formed before any cleanup",
                        i,
                    )?;
                    state.returned = Some((r, result(i)?));
                }
            }
            for (r, &seed) in roots.iter().enumerate() {
                if i.results.contains(&seed) {
                    need(
                        state.owners[r].is_none(),
                        "stack temporary source has one fresh birth per execution",
                        i,
                    )?;
                    state.owners[r] = Some(seed);
                }
            }
            // Every protected loan operand must be live on this exact path.
            for entity in i.operation.entities() {
                if layers.iter().any(|l| {
                    l.source.contains(&entity) || l.components.iter().any(|c| c.contains(&entity))
                }) {
                    need(
                        state.loans.contains_key(&entity),
                        "stack every actual source/component use is live",
                        (i, entity, &state),
                    )?;
                }
                if let Some(r) = roots.iter().position(|seed| {
                    layers
                        .iter()
                        .any(|l| l.owner == Some(*seed) && l.owners.contains(&entity))
                }) {
                    need(
                        state.owners[r] == Some(entity),
                        "stack every operation uses exact current owner column",
                        (i, entity, &state),
                    )?;
                }
            }
            if let Some(li) = layers.iter().position(|l| l.birth.id == i.id) {
                let l = &layers[li];
                need(
                    state.phase[li] == 0 && state.returned.is_none(),
                    "stack provider born once in active scope",
                    (i, &state),
                )?;
                for parent in &state.stack {
                    need(
                        state.phase[*parent] == 3,
                        "stack entering child preserves parent's K/V",
                        (i, &state),
                    )?;
                }
                if let Some(seed) = l.owner {
                    let r = roots.iter().position(|&v| v == seed).unwrap();
                    let Operation::BorrowBegin { place, .. } = i.operation else {
                        unreachable!()
                    };
                    let root = g.definition(EntityId::Place(place))?;
                    need(
                        matches!(root.operation,Operation::RootPlace{owner} if state.owners[r]==Some(EntityId::Value(owner))),
                        "stack provider borrows actual current source owner",
                        (i, &state),
                    )?;
                }
                let source = result(i)?;
                state.sources[li] = Some(source);
                state.loans.insert(source, None);
                state.phase[li] = 1;
                state.stack.push(li);
                continue;
            }
            if let Some(gi) = guards.iter().position(|guard| guard.birth.id == i.id) {
                let li = local_layer(layers, guards[gi].layer)?;
                need(
                    state.phase[li] == 3
                        && state.guards[gi].is_none()
                        && state.stack.last() == Some(&li),
                    "stack Guard allocation belongs to live actual body layer",
                    i,
                )?;
                state.guards[gi] = Some(result(i)?);
            }
            match &i.operation {
                Operation::MapIterationFirst { source } => {
                    let li = one(
                        layers
                            .iter()
                            .enumerate()
                            .filter_map(|(li, l)| (l.first.id == i.id).then_some(li)),
                        "stack First actual layer",
                    )?;
                    need(
                        state.phase[li] == 1
                            && state.sources[li] == Some(EntityId::Loan(*source))
                            && state.cursors[li].is_none(),
                        "stack First uses own live source once",
                        (i, &state),
                    )?;
                    state.cursors[li] = Some(result(i)?);
                }
                Operation::MapIterationHasNext { source, cursor } => {
                    let li = one(
                        layers
                            .iter()
                            .enumerate()
                            .filter_map(|(li, l)| (l.has.id == i.id).then_some(li)),
                        "stack Has actual layer",
                    )?;
                    need(
                        state.phase[li] == 1
                            && state.sources[li] == Some(EntityId::Loan(*source))
                            && state.cursors[li] == Some(EntityId::Value(*cursor))
                            && state.stack.last() == Some(&li),
                        "stack header exact current source/cursor and empty component state",
                        (i, &state),
                    )?;
                }
                Operation::MapIterationKeyLoan { source, cursor }
                | Operation::MapIterationValueLoan { source, cursor } => {
                    let (li, role) = one(
                        layers.iter().enumerate().filter_map(|(li, l)| {
                            if l.key.id == i.id {
                                Some((li, 0))
                            } else if l.val.id == i.id {
                                Some((li, 1))
                            } else {
                                None
                            }
                        }),
                        "stack actual K/V reader layer",
                    )?;
                    need(
                        state.phase[li] == if role == 0 { 1 } else { 2 }
                            && state.sources[li] == Some(EntityId::Loan(*source))
                            && state.cursors[li] == Some(EntityId::Value(*cursor)),
                        "stack ordered exact K/V reader current operands",
                        (i, &state),
                    )?;
                    let id = result(i)?;
                    state.components[li][role] = Some(id);
                    state.loans.insert(id, None);
                    state.phase[li] += 1;
                }
                Operation::SharedReborrow { source } => {
                    let parent = EntityId::Loan(*source);
                    if state.loans.contains_key(&parent) {
                        need(
                            state.loans.insert(result(i)?, Some(parent)).is_none(),
                            "stack exact derived loan birth",
                            i,
                        )?;
                    }
                }
                Operation::BorrowEnd { loan } => {
                    let entity = EntityId::Loan(*loan);
                    if let Some(parent) = state.loans.get(&entity).copied() {
                        need(
                            !state.loans.values().any(|p| *p == Some(entity)),
                            "stack all descendants end before parent",
                            (i, &state),
                        )?;
                        if parent.is_none() {
                            if let Some(li) = state.sources.iter().position(|s| *s == Some(entity))
                            {
                                need(
                                    state.stack.last() == Some(&li)
                                        && (state.phase[li] == 1 || state.phase[li] == 5)
                                        && guards.iter().enumerate().all(|(gi, guard)| {
                                            guard.layer != layers[li].index
                                                || state.guards[gi].is_none()
                                        }),
                                    "stack EndSource after own Guard/V/K and inner source",
                                    (i, &state),
                                )?;
                                push(
                                    &mut state,
                                    inputs,
                                    Action::End(layers[li].index),
                                    layers,
                                    unit,
                                )?;
                                state.end_origins.push((li, i.origin.clone()));
                                state.phase[li] = 0;
                                state.sources[li] = None;
                                state.cursors[li] = None;
                                state.stack.pop();
                            } else if let Some((li, role)) =
                                state.components.iter().enumerate().find_map(|(li, c)| {
                                    c.iter().position(|s| *s == Some(entity)).map(|r| (li, r))
                                })
                            {
                                need(
                                    state.stack.last() == Some(&li)
                                        && state.phase[li] == if role == 1 { 3 } else { 4 }
                                        && guards.iter().enumerate().all(|(gi, guard)| {
                                            guard.layer != layers[li].index
                                                || state.guards[gi].is_none()
                                        }),
                                    "stack exact Guard then VEnd then KEnd",
                                    (i, &state),
                                )?;
                                push(
                                    &mut state,
                                    inputs,
                                    if role == 1 {
                                        Action::Value(layers[li].index)
                                    } else {
                                        Action::Key(layers[li].index)
                                    },
                                    layers,
                                    unit,
                                )?;
                                state.end_origins.push((li, i.origin.clone()));
                                state.components[li][role] = None;
                                state.phase[li] += 1;
                            } else {
                                return Err(failure(
                                    "stack root End belongs to exact active role",
                                    (i, &state),
                                ));
                            }
                        }
                        state.loans.remove(&entity);
                    }
                }
                Operation::MapIterationNext { source, cursor } => {
                    let li = one(
                        layers.iter().enumerate().filter_map(|(li, l)| {
                            l.next.iter().any(|n| n.id == i.id).then_some(li)
                        }),
                        "stack actual Next layer",
                    )?;
                    need(
                        state.phase[li] == 5
                            && state.stack.last() == Some(&li)
                            && state.sources[li] == Some(EntityId::Loan(*source))
                            && state.cursors[li] == Some(EntityId::Value(*cursor))
                            && state.returned.is_none(),
                        "stack Next advances only its own current cursor after cleanup",
                        (i, &state),
                    )?;
                    let cleanup = one(
                        inputs
                            .cleanups
                            .iter()
                            .filter(|c| c.kind == 0 && c.point == layers[li].site.ast.body_span),
                        "stack original normal body cleanup",
                    )?;
                    exact(&state.events, cleanup)?;
                    check_origins(&state, inputs, layers, unit, Some(cleanup))?;
                    state.events.clear();
                    state.end_origins.clear();
                    state.phase[li] = 1;
                    state.cursors[li] = Some(result(i)?);
                }
                Operation::Drop { owner } => {
                    let entity = EntityId::Value(*owner);
                    if let Some(gi) = guards
                        .iter()
                        .position(|guard| guard.owners.contains(&entity))
                    {
                        need(
                            state.guards[gi] == Some(entity) && at(i, guards[gi].marker),
                            "stack Guard Drop consumes exact current allocation once",
                            (i, &state),
                        )?;
                        push(
                            &mut state,
                            inputs,
                            Action::Drop(guards[gi].marker),
                            layers,
                            unit,
                        )?;
                        state.guards[gi] = None;
                    } else if let Some(r) = roots.iter().position(|seed| {
                        layers
                            .iter()
                            .any(|l| l.owner == Some(*seed) && l.owners.contains(&entity))
                    }) {
                        need(
                            state.owners[r] == Some(entity)
                                && layers.iter().enumerate().all(|(li, l)| {
                                    l.owner != Some(roots[r]) || state.phase[li] == 0
                                }),
                            "stack source sole Drop after all of its active provider leases",
                            (i, &state),
                        )?;
                        let mut markers = Vec::new();
                        for l in layers.iter().filter(|l| l.owner == Some(roots[r])) {
                            let marker = match l.site.source {
                                Source::Parameter { marker, .. } => marker,
                                Source::Call { span, .. } => span,
                            };
                            if !markers.contains(&marker) {
                                markers.push(marker);
                            }
                        }
                        let marker = one(markers.into_iter(), "stack original owner root marker")?;
                        need(at(i, marker), "stack source Drop original birth marker", i)?;
                        let event = Action::Drop(marker);
                        let mut candidate = state.events.clone();
                        candidate.push(event.clone());
                        if state.returned.is_some()
                            || inputs
                                .cleanups
                                .iter()
                                .any(|c| actions(c).starts_with(&candidate))
                        {
                            push(&mut state, inputs, event, layers, unit)?;
                        } else if !state.events.is_empty() {
                            // Resource source remains alive after exhaustion;
                            // its later lexical Return Drop is not an iteration
                            // exit action. It still consumes this exact current
                            // owner once, as checked above and at every Return.
                            finish(&mut state, inputs, None, layers, unit)?;
                        }
                        state.owners[r] = None;
                    }
                }
                _ => {}
            }
        }
        let term = g.term(block)?;
        match &term.kind {
            TerminatorKind::Return { values } => {
                finite_returns += 1;
                if let Some((r, value)) = state.returned {
                    need(
                        values
                            .iter()
                            .copied()
                            .map(EntityId::Value)
                            .collect::<Vec<_>>()
                            == [value],
                        "stack Return delivers actual operand result after cleanup",
                        (term, &state),
                    )?;
                    let cleanup = one(
                        inputs
                            .cleanups
                            .iter()
                            .filter(|c| c.kind == 2 && c.point == returns[r].span),
                        "stack original exact Return cleanup plan",
                    )?;
                    exact(&state.events, cleanup)?;
                    check_origins(&state, inputs, layers, unit, Some(cleanup))?;
                    state.events.clear();
                    state.end_origins.clear();
                } else if !state.events.is_empty() {
                    finish(&mut state, inputs, None, layers, unit)?;
                }
                need(
                    state.phase.iter().all(|&p| p == 0)
                        && state.stack.is_empty()
                        && state.loans.is_empty()
                        && state.guards.iter().all(Option::is_none)
                        && state.owners.iter().all(Option::is_none),
                    "stack each finite Return cleans every layer/source exactly once",
                    (term, &state),
                )?;
                need(
                    values.len() == g.function.return_types.len(),
                    "stack actual ordinary Unit/Int Return arity",
                    term,
                )?;
            }
            TerminatorKind::Abort => {
                return Err(failure("stack scanner unexpectedly aborts", term));
            }
            _ => {
                for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
                    let mut next = state.clone();
                    transport(g, edge, &mut next)?;
                    pending.push((edge.target, next));
                }
            }
        }
    }
    need(
        finite_returns > 0,
        "stack real reachable finite Return",
        g.function.id,
    )
}

fn local_layer(layers: &[Layer<'_>], global: usize) -> Result<usize, Failure> {
    one(
        layers
            .iter()
            .enumerate()
            .filter_map(|(i, l)| (l.index == global).then_some(i)),
        "stack original global/local layer mapping",
    )
}
fn actions(c: &Cleanup) -> Vec<Action> {
    c.actions
        .iter()
        .filter(|a| !matches!(a, Action::Finish(_)))
        .cloned()
        .collect()
}
fn exact(events: &[Action], cleanup: &Cleanup) -> Result<(), Failure> {
    need(
        events == actions(cleanup),
        "stack complete original ordered cleanup, no missing/extra owner or layer",
        (events, cleanup),
    )
}
fn push(
    state: &mut State,
    inputs: &Inputs,
    event: Action,
    layers: &[Layer<'_>],
    unit: bool,
) -> Result<(), Failure> {
    let mut candidate = state.events.clone();
    candidate.push(event.clone());
    if !inputs
        .cleanups
        .iter()
        .any(|c| actions(c).starts_with(&candidate))
    {
        need(
            state.returned.is_none(),
            "stack Return cleanup cannot split into shorter normal exits",
            (&state.events, &event),
        )?;
        finish(state, inputs, None, layers, unit)?;
        candidate = vec![event];
    }
    need(
        inputs
            .cleanups
            .iter()
            .any(|c| actions(c).starts_with(&candidate)),
        "stack actual cleanup is prefix of a full original FE action list",
        (&candidate, &inputs.cleanups),
    )?;
    state.events = candidate;
    Ok(())
}
fn finish(
    state: &mut State,
    inputs: &Inputs,
    kind: Option<u8>,
    layers: &[Layer<'_>],
    unit: bool,
) -> Result<(), Failure> {
    let candidates = inputs
        .cleanups
        .iter()
        .filter(|c| c.kind != 2 && kind.is_none_or(|k| c.kind == k) && actions(c) == state.events)
        .collect::<Vec<_>>();
    need(
        !candidates.is_empty(),
        "stack finite ordinary exit matches complete FE plan",
        (&state.events, &inputs.cleanups),
    )?;
    need(
        candidates
            .iter()
            .any(|c| check_origins(state, inputs, layers, unit, Some(c)).is_ok()),
        "stack ordinary cleanup has exact matching For/transfer origin",
        (&state.end_origins, candidates),
    )?;
    state.events.clear();
    state.end_origins.clear();
    Ok(())
}
fn check_origins(
    state: &State,
    _inputs: &Inputs,
    layers: &[Layer<'_>],
    unit: bool,
    cleanup: Option<&Cleanup>,
) -> Result<(), Failure> {
    for (li, origin) in &state.end_origins {
        let expected = if !unit && cleanup.is_some_and(|c| c.kind >= 2) {
            cleanup.unwrap().point
        } else {
            layers[*li].site.ast.span
        };
        need(
            *origin == Origin::Source(expected),
            "stack exact entry-specific cleanup source span",
            (li, origin, expected),
        )?;
    }
    Ok(())
}
fn transport(g: &Graph<'_>, edge: &Edge, state: &mut State) -> Result<(), Failure> {
    let map = |entity: EntityId| -> Result<EntityId, Failure> {
        if edge.arguments.contains(&entity) {
            g.transported(edge, entity)
        } else {
            Ok(entity)
        }
    };
    for id in state
        .sources
        .iter_mut()
        .chain(state.cursors.iter_mut())
        .chain(state.owners.iter_mut())
        .chain(state.guards.iter_mut())
        .flatten()
    {
        *id = map(*id)?;
    }
    for id in state
        .components
        .iter_mut()
        .flat_map(|c| c.iter_mut())
        .flatten()
    {
        *id = map(*id)?;
    }
    state.loans = state
        .loans
        .iter()
        .map(|(&id, &parent)| Ok((map(id)?, parent.map(map).transpose()?)))
        .collect::<Result<_, Failure>>()?;
    if let Some((r, id)) = state.returned {
        state.returned = Some((r, map(id)?));
    }
    Ok(())
}
