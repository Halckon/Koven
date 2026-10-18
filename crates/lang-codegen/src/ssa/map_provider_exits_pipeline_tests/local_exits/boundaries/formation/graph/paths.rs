//! Exhaustive finite owner/loan state transitions over the actual reachable graph.
use super::*;
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct State {
    owners: BTreeMap<EntityId, EntityId>,
    loans: BTreeSet<LoanId>,
    dropped: BTreeSet<EntityId>,
    component_ends: Vec<usize>,
}
pub(super) fn check(
    g: &Graph<'_>,
    module: &Module,
    input: &Inputs,
    parsed: &ParsedFile,
    source_seed: EntityId,
    (leases, components): (&BTreeSet<EntityId>, &[BTreeSet<EntityId>; 2]),
    (next, has, first): (&Instruction, &Instruction, &Instruction),
) -> Result<(), Failure> {
    let mut seeds = BTreeSet::new();
    for i in &g.instructions {
        for &r in &i.results {
            if map_value(module, g, r) {
                seeds.insert(r);
            }
        }
    }
    let entry = g.function.entry_block().unwrap();
    let mut state = State {
        owners: BTreeMap::new(),
        loans: BTreeSet::new(),
        dropped: BTreeSet::new(),
        component_ends: vec![],
    };
    for &p in &g.block(entry)?.parameters {
        if map_value(module, g, p) {
            seeds.insert(p);
            state.owners.insert(p, p);
        }
    }
    let mut pending = vec![(entry, state)];
    let mut seen = BTreeSet::new();
    let mut returned = 0;
    let mut drops_seen = BTreeSet::new();
    while let Some((block, mut state)) = pending.pop() {
        if !seen.insert((block, state.clone())) {
            continue;
        }
        need(
            seen.len() < 300,
            "formation finite exact current-owner states",
            &seen,
        )?;
        for i in g.ops(block)? {
            for operand in i.operation.entities() {
                if map_value(module, g, operand) {
                    need(
                        state.owners.values().any(|v| *v == operand),
                        "formation every owner use is the actual live current value",
                        (i, &state),
                    )?;
                }
                if let EntityId::Loan(l) = operand {
                    need(
                        state.loans.contains(&l),
                        "formation every loan use remains live",
                        (i, &state),
                    )?;
                }
            }
            match &i.operation {
                Operation::BorrowBegin { .. }
                | Operation::SharedReborrow { .. }
                | Operation::MapIterationKeyLoan { .. }
                | Operation::MapIterationValueLoan { .. } => {
                    state.loans.insert(loan(i)?);
                }
                Operation::BorrowEnd { loan } => {
                    if leases.contains(&EntityId::Loan(*loan)) {
                        need(
                            state.loans.iter().all(|l| {
                                !components
                                    .iter()
                                    .any(|set| set.contains(&EntityId::Loan(*l)))
                            }),
                            "formation source End after all components",
                            i,
                        )?;
                    }
                    for (index, set) in components.iter().enumerate() {
                        if set.contains(&EntityId::Loan(*loan)) {
                            need(
                                state.component_ends.as_slice()
                                    == if index == 1 { &[][..] } else { &[1][..] },
                                "formation exact VEnd before KEnd",
                                (i, &state),
                            )?;
                            state.component_ends.push(index);
                        }
                    }
                    need(
                        state.loans.remove(loan),
                        "formation unique actual loan End",
                        i,
                    )?;
                }
                Operation::MapPut { owner, .. } => {
                    let key = one(
                        state
                            .owners
                            .iter()
                            .filter_map(|(k, v)| (*v == EntityId::Value(*owner)).then_some(*k)),
                        "formation put consumes actual current owner",
                    )?;
                    state.owners.remove(&key);
                    state.owners.insert(result(i)?, result(i)?);
                }
                Operation::Drop { owner } if map_value(module, g, EntityId::Value(*owner)) => {
                    let key = one(
                        state
                            .owners
                            .iter()
                            .filter_map(|(k, v)| (*v == EntityId::Value(*owner)).then_some(*k)),
                        "formation Drop consumes actual current owner",
                    )?;
                    need(
                        state.dropped.insert(key),
                        "formation each epoch consumed exactly once",
                        (i, &state),
                    )?;
                    state.owners.remove(&key);
                    let candidates = input
                        .drops
                        .iter()
                        .filter(|d| i.origin == Origin::Source(d.origin))
                        .filter(|d| {
                            let expected = if d.root == input.site.root
                                && match d.point {
                                    Point::Loop(_) => true,
                                    Point::Statement(at) => inside(input.site.for_span, at),
                                    _ => false,
                                } {
                                Ok(source_seed)
                            } else {
                                match d.birth {
                                    Birth::Call(id) => {
                                        original_call(g, module, input, span(parsed, id))
                                            .and_then(result)
                                    }
                                    Birth::Parameter(_) => one(
                                        g.block(g.function.entry_block().unwrap())
                                            .unwrap()
                                            .parameters
                                            .iter()
                                            .copied()
                                            .filter(|&p| map_value(module, g, p)),
                                        "formation Drop actual parameter birth",
                                    ),
                                }
                            };
                            expected.is_ok_and(|seed| seed == key)
                        })
                        .collect::<Vec<_>>();
                    let fact = one(
                        candidates.into_iter(),
                        "formation Drop original same-analysis identity",
                    )?;
                    need(
                        inside(fact.root, input.site.scan),
                        "formation original named root marker",
                        fact,
                    )?;
                    match fact.point {
                        Point::Loop(s) => {
                            need(
                                s == input.site.for_span
                                    && block != has.block
                                    && block != next.block
                                    && key == source_seed,
                                "formation exact exhausted current source Drop",
                                (i, fact),
                            )?;
                            let end=one(g.ops(block)?.into_iter().filter(|v|matches!(v.operation,Operation::BorrowEnd{loan} if leases.contains(&EntityId::Loan(loan)))),"formation source End at original LoopExit")?;
                            need(
                                g.position(end)? < g.position(i)?,
                                "formation EndSource before exact current Drop",
                                (end, i),
                            )?;
                        }
                        Point::Expression(at) | Point::Call(at) => {
                            let call = original_call(g, module, input, at)?;
                            need(
                                call.block == block && g.position(call)? < g.position(i)?,
                                "formation RHS/borrow Call completes before its exact Drop",
                                (call, i),
                            )?;
                            if matches!(fact.point, Point::Call(_)) {
                                need(
                                    !state.loans.iter().any(|l| {
                                        g.ops(block).unwrap().iter().any(|x| {
                                            x.id == call.id
                                                && x.operation
                                                    .entities()
                                                    .contains(&EntityId::Loan(*l))
                                        })
                                    }),
                                    "formation borrowed current owner drops after actual call loan End",
                                    (i, &state),
                                )?;
                            }
                        }
                        Point::Statement(at) => {
                            if inside(input.site.for_span, at) {
                                need(
                                    state.loans.is_empty(),
                                    "formation lexical source Drop after completed provider",
                                    (i, fact, &state),
                                )?;
                                for operation in g.ops(block)? {
                                    if let Origin::Source(origin) = operation.origin
                                        && origin.start() > input.site.for_span.end()
                                        && origin.end() < at.end()
                                        && !matches!(operation.operation, Operation::Drop { .. })
                                    {
                                        need(
                                            g.position(operation)? < g.position(i)?,
                                            "formation original lexical final uses precede source Drop",
                                            (operation, i),
                                        )?;
                                    }
                                }
                            } else {
                                let producer = g.definition(key)?;
                                let original = match producer.origin {
                                    Origin::Source(s) => s,
                                    _ => {
                                        return Err(failure(
                                            "formation original initializer producer span",
                                            producer,
                                        ));
                                    }
                                };
                                need(
                                    at.end() < input.site.for_span.start()
                                        && at.start() <= original.start()
                                        && original.end() <= at.end()
                                        && producer.block == block
                                        && g.position(producer)? < g.position(i)?,
                                    "formation early unused declaration Drop follows its actual complete initializer",
                                    (producer, i, fact),
                                )?;
                            }
                        }
                        other => {
                            return Err(failure(
                                "formation unexpected source Drop boundary",
                                other,
                            ));
                        }
                    }
                    drops_seen.insert(i.id);
                }
                Operation::DirectCall {
                    callee,
                    receiver: None,
                    arguments,
                } => {
                    let target = module
                        .functions
                        .iter()
                        .find(|f| f.id == *callee)
                        .ok_or_else(|| failure("formation actual called function", callee))?;
                    let params = &target
                        .block(target.entry_block().unwrap())
                        .unwrap()
                        .parameters;
                    need(
                        arguments.len() == params.len(),
                        "formation exact actual selected ABI arity",
                        i,
                    )?;
                    for (&a, &p) in arguments.iter().zip(params) {
                        need(
                            g.function.entity(a).unwrap().ty == target.entity(p).unwrap().ty,
                            "formation exact selected ABI operand type",
                            (i, a, p),
                        )?;
                        if map_value(module, g, a) {
                            let key = one(
                                state
                                    .owners
                                    .iter()
                                    .filter_map(|(k, v)| (*v == a).then_some(*k)),
                                "formation owning Call exact current owner",
                            )?;
                            need(
                                state.dropped.insert(key),
                                "formation unique owning delivery",
                                i,
                            )?;
                            state.owners.remove(&key);
                            if let Some(sink) = input.site.sink {
                                need(
                                    i.origin == Origin::Source(span(parsed, sink))
                                        && key == source_seed
                                        && state.loans.is_empty(),
                                    "formation terminal sink gets exact source current Value after every lease End",
                                    (i, &state),
                                )?;
                                sink_drop(module, target, p)?;
                            }
                        }
                    }
                    for &r in &i.results {
                        if seeds.contains(&r) {
                            need(
                                state.owners.insert(r, r).is_none(),
                                "formation unique actual factory birth",
                                i,
                            )?;
                        }
                    }
                }
                _ => {}
            }
            if i.id == first.id {
                let Operation::MapIterationFirst { source } = i.operation else {
                    unreachable!()
                };
                let begin = g.definition(EntityId::Loan(source))?;
                let Operation::BorrowBegin { place, .. } = begin.operation else {
                    unreachable!()
                };
                let root = g.definition(EntityId::Place(place))?;
                need(
                    matches!(root.operation,Operation::RootPlace{owner} if state.owners.get(&source_seed)==Some(&EntityId::Value(owner))),
                    "formation First uses the real currently installed source owner",
                    (root, &state),
                )?;
            }
            if i.id == next.id {
                need(
                    state.component_ends.as_slice() == [1, 0],
                    "formation every Next follows exact VEnd/KEnd order",
                    (i, &state),
                )?;
                state.component_ends.clear();
                need(
                    state.loans.iter().all(|l| {
                        !components
                            .iter()
                            .any(|set| set.contains(&EntityId::Loan(*l)))
                    }),
                    "formation every actual Next follows all component Ends",
                    (i, &state),
                )?;
            }
        }
        let term = g.term(block)?;
        match &term.kind {
            TerminatorKind::Return { values } => {
                need(
                    values.is_empty() && state.owners.is_empty() && state.loans.is_empty(),
                    "formation each finite Unit Return consumed every owner and loan once",
                    (term, &state),
                )?;
                returned += 1;
            }
            TerminatorKind::Branch(_) | TerminatorKind::Conditional { .. } => {
                for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
                    let mut successor = state.clone();
                    for current in successor.owners.values_mut() {
                        let columns = edge
                            .arguments
                            .iter()
                            .enumerate()
                            .filter(|(_, a)| **a == *current)
                            .map(|(n, _)| n)
                            .collect::<Vec<_>>();
                        need(
                            columns.len() <= 1,
                            "formation owner never copied to multiple incoming slots",
                            (edge, *current),
                        )?;
                        if let Some(&slot) = columns.first() {
                            *current = g.block(edge.target)?.parameters[slot];
                        }
                    }
                    let mut loans = BTreeSet::new();
                    for l in &successor.loans {
                        let e = EntityId::Loan(*l);
                        let slots = edge
                            .arguments
                            .iter()
                            .enumerate()
                            .filter_map(|(n, &a)| (a == e).then_some(n))
                            .collect::<Vec<_>>();
                        need(
                            slots.len() <= 1,
                            "formation loan unique actual edge column",
                            edge,
                        )?;
                        let transported = if let Some(&n) = slots.first() {
                            g.block(edge.target)?.parameters[n]
                        } else {
                            e
                        };
                        let EntityId::Loan(l) = transported else {
                            return Err(failure("formation actual transported Loan", transported));
                        };
                        loans.insert(l);
                    }
                    successor.loans = loans;
                    pending.push((edge.target, successor));
                }
            }
            other => return Err(failure("formation unsupported test terminator", other)),
        }
    }
    need(
        returned > 0 && drops_seen.len() == input.drops.len(),
        "formation all exact FE Drop obligations are reached and no extra Drop exists",
        (returned, drops_seen, &input.drops),
    )
}
fn sink_drop(module: &Module, sink: &Function, parameter: EntityId) -> Result<(), Failure> {
    let g = Graph::with_abort(sink)?;
    need(
        !g.instructions
            .iter()
            .any(|i| matches!(i.operation, Operation::DirectCall { .. })),
        "formation original sink has only its closed print/size/error primitives",
        sink.id,
    )?;

    let drops=g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::Drop{owner} if map_value(module,&g,EntityId::Value(owner)))).collect::<Vec<_>>();
    let drop = one(
        drops.into_iter(),
        "formation sink owns unique final Map Drop instruction",
    )?;
    let entry = sink.entry_block().unwrap();
    let mut pending = vec![(entry, Some(parameter), BTreeSet::<LoanId>::new(), false)];
    let mut seen = BTreeSet::new();
    let mut returns = 0;
    let mut uses = Uses::default();
    for e in family(&g, parameter)? {
        uses.protect(e);
    }
    while let Some((block, mut current, mut loans, mut dropped)) = pending.pop() {
        if !seen.insert((block, current, loans.clone(), dropped)) {
            continue;
        }
        for i in g.ops(block)? {
            for e in i.operation.entities() {
                if map_value(module, &g, e) {
                    need(
                        current == Some(e),
                        "formation sink every use is exact delivered current owner",
                        (i, current),
                    )?;
                    uses.permit(e, i);
                }
                if let EntityId::Loan(l) = e {
                    need(loans.contains(&l), "formation sink actual live loan", i)?;
                }
            }
            match i.operation {
                Operation::BorrowBegin { .. } | Operation::SharedReborrow { .. } => {
                    loans.insert(loan(i)?);
                }
                Operation::BorrowEnd { loan } => {
                    need(
                        loans.remove(&loan),
                        "formation sink unique actual loan End",
                        i,
                    )?;
                }
                Operation::Drop { owner } if map_value(module, &g, EntityId::Value(owner)) => {
                    need(
                        i.id == drop.id
                            && current == Some(EntityId::Value(owner))
                            && loans.is_empty()
                            && !dropped,
                        "formation sink uniquely disposes current delivered owner after final read",
                        i,
                    )?;
                    current = None;
                    dropped = true;
                }
                _ => {}
            }
        }
        match &g.term(block)?.kind {
            TerminatorKind::Return { values } => {
                need(
                    values.is_empty() && current.is_none() && dropped && loans.is_empty(),
                    "formation every normal sink Return follows unique current-owner Drop",
                    g.term(block)?,
                )?;
                returns += 1;
            }
            TerminatorKind::Abort => {}
            _ => {
                for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
                    let next = if let Some(owner) = current {
                        let slots = edge
                            .arguments
                            .iter()
                            .enumerate()
                            .filter_map(|(n, &e)| (e == owner).then_some(n))
                            .collect::<Vec<_>>();
                        need(
                            slots.len() <= 1,
                            "formation sink unique actual owner transport",
                            edge,
                        )?;
                        Some(
                            slots
                                .first()
                                .map(|&n| g.block(edge.target).unwrap().parameters[n])
                                .unwrap_or(owner),
                        )
                    } else {
                        None
                    };
                    let mut transported = BTreeSet::new();
                    for &l in &loans {
                        let slots = edge
                            .arguments
                            .iter()
                            .enumerate()
                            .filter_map(|(n, &e)| (e == EntityId::Loan(l)).then_some(n))
                            .collect::<Vec<_>>();
                        need(
                            slots.len() <= 1,
                            "formation sink unique actual loan transport",
                            edge,
                        )?;
                        let e = slots
                            .first()
                            .map(|&n| g.block(edge.target).unwrap().parameters[n])
                            .unwrap_or(EntityId::Loan(l));
                        let EntityId::Loan(l) = e else {
                            return Err(failure("formation sink transported Loan", e));
                        };
                        transported.insert(l);
                    }
                    pending.push((edge.target, next, transported, dropped));
                }
            }
        }
    }
    need(returns > 0, "formation sink actual normal Return", sink.id)?;
    uses.check(&g, None)
}
