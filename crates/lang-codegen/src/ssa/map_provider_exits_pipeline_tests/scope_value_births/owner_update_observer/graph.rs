//! Actual control paths preserve source epochs through every incoming column and cleanup.
use super::super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, function, result, value},
    query_inputs::one,
};
use super::{
    Case,
    ast::{Input, span},
};
use crate::ssa::model::*;
use lang_frontend::{
    ast::ExpressionId,
    parser::{Expression, ParsedFile},
};
use std::collections::{BTreeMap, BTreeSet};
struct State {
    live: BTreeSet<ValueId>,
    origins: BTreeMap<ValueId, ExpressionId>,
    places: BTreeMap<PlaceId, ValueId>,
    loans: BTreeMap<LoanId, ValueId>,
    known_loans: BTreeSet<LoanId>,
    scalars: BTreeMap<EntityId, bool>,
    births: Vec<ExpressionId>,
    puts: usize,
    observes: usize,
    drops: Vec<(ExpressionId, InstructionId)>,
}
fn expected(c: &Case, i: &Input, flag: bool, index: usize) -> ExpressionId {
    match c.code {
        7 => i.births[usize::from(flag)].expression,
        8 => i.births[usize::from(flag && index == 1)].expression,
        9 => {
            if flag {
                i.put.unwrap()
            } else {
                i.births[0].expression
            }
        }
        10 => i.births[1].expression,
        11 => i.births[usize::from(index == 0)].expression,
        _ => unreachable!(),
    }
}
fn transfer(g: &Graph<'_>, edge: &Edge, state: &mut State) -> Result<(), Failure> {
    need(
        state.loans.is_empty(),
        "update no Map call loan escapes a control branch",
        &state.loans,
    )?;
    let mut live = BTreeSet::new();
    let mut origins = Vec::new();
    let mut scalars = Vec::new();
    for (&argument, &parameter) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
        if let EntityId::Value(old) = argument
            && let Some(origin) = state.origins.get(&old)
        {
            let EntityId::Value(new) = parameter else {
                return Err(failure(
                    "update owner stays Value through actual edge",
                    edge,
                ));
            };
            need(
                state.live.contains(&old) && live.insert(new),
                "update every incoming owner column transports a live unique actual epoch",
                (edge, old, new),
            )?;
            origins.push((new, *origin));
        }
        if let Some(flag) = state.scalars.get(&argument) {
            scalars.push((parameter, *flag));
        }
    }
    let mapped = edge
        .arguments
        .iter()
        .filter_map(|arg| match arg {
            EntityId::Value(v) if state.origins.contains_key(v) => Some(*v),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    need(
        mapped == state.live && live.len() == mapped.len(),
        "update complete actual control edge neither loses nor duplicates a live Map owner",
        (edge, &state.live, &mapped),
    )?;
    state.live = live;
    state.origins.extend(origins);
    state.scalars.extend(scalars);
    Ok(())
}
pub(super) fn check(c: &Case, p: &ParsedFile, i: &Input, program: &Program) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "update actual original SSA module")?;
    let scan = function(module, i.scan)?;
    let observer = function(module, i.observer)?;
    let g = Graph::new(scan)?;
    println!(
        "SCOPE C UPDATE SSA label={} function={scan:#?} edges={:#?}",
        c.label, g.edges
    );
    let entry = scan.entry_block().unwrap();
    let parameters = &g.block(entry)?.parameters;
    need(
        parameters.len() == usize::from(c.code <= 9) && scan.return_types.is_empty(),
        "update actual scanner ABI",
        parameters,
    )?;
    let mut actual_births = BTreeMap::new();
    for b in &i.births {
        let factory = function(module, b.factory)?;
        let call = one(
            g.instructions.iter().copied().filter(|op| {
                op.origin == Origin::Source(span(p, b.expression))
                    && op.operation
                        == Operation::DirectCall {
                            callee: factory.id,
                            receiver: None,
                            arguments: vec![],
                        }
            }),
            "update actual distinct original factory instance",
        )?;
        need(
            matches!(result(call)?, EntityId::Value(_))
                && factory.return_types.as_slice()
                    == [scan.entity(result(call)?).unwrap().ty.semantic_type()],
            "update exact owned factory result ABI",
            call,
        )?;
        actual_births.insert(call.id, b.expression);
    }
    let mut actual_observes = BTreeMap::new();
    for &(call, arg) in &i.observes {
        let op = one(
            g.instructions.iter().copied().filter(|op| {
                op.origin == Origin::Source(span(p, call))
                    && matches!(op.operation,Operation::DirectCall{callee,..}if callee==observer.id)
            }),
            "update actual original observe instance",
        )?;
        actual_observes.insert(op.id, (call, arg));
    }
    if let Some(put) = i.put {
        put_clones(&g, p, put)?;
    }
    let mut visited = BTreeSet::new();
    let mut counted = BTreeSet::new();
    for flag in if c.code <= 9 {
        vec![true, false]
    } else {
        vec![false]
    } {
        let mut state = State {
            live: BTreeSet::new(),
            origins: BTreeMap::new(),
            places: BTreeMap::new(),
            loans: BTreeMap::new(),
            known_loans: BTreeSet::new(),
            scalars: BTreeMap::new(),
            births: vec![],
            puts: 0,
            observes: 0,
            drops: vec![],
        };
        if let Some(parameter) = parameters.first() {
            state.scalars.insert(*parameter, flag);
        }
        let mut path = BTreeSet::new();
        let mut block = entry;
        let mut conditionals = 0;
        loop {
            need(
                path.insert(block),
                "update ordinary control CFG has no invented cycle",
                block,
            )?;
            visited.insert(block);
            for op in g.ops(block)? {
                counted.insert(op.id);
                if let Some(&birth) = actual_births.get(&op.id) {
                    let owner = value(op)?;
                    need(
                        state.live.insert(owner),
                        "update fresh factory owner is installed once",
                        op,
                    )?;
                    state.origins.insert(owner, birth);
                    state.births.push(birth);
                    continue;
                }
                match &op.operation {
                    Operation::RootPlace { owner } => {
                        if state.origins.contains_key(owner) {
                            need(
                                state.live.contains(owner),
                                "update RootPlace cannot borrow a dropped or stale owner",
                                op,
                            )?;
                            let EntityId::Place(place) = result(op)? else {
                                return Err(failure("update actual root place result", op));
                            };
                            state.places.insert(place, *owner);
                        }
                    }
                    Operation::BorrowBegin { place, kind } => {
                        if let Some(&owner) = state.places.get(place) {
                            need(
                                *kind == LoanKind::Shared && state.live.contains(&owner),
                                "update observe begins Shared loan on current live epoch",
                                op,
                            )?;
                            let EntityId::Loan(loan) = result(op)? else {
                                return Err(failure("update actual Map loan result", op));
                            };
                            state.loans.insert(loan, owner);
                            state.known_loans.insert(loan);
                        }
                    }
                    Operation::SharedReborrow { source } => {
                        if let Some(&owner) = state.loans.get(source) {
                            let EntityId::Loan(loan) = result(op)? else {
                                return Err(failure("update actual reborrow result", op));
                            };
                            state.loans.insert(loan, owner);
                            state.known_loans.insert(loan);
                        }
                    }
                    Operation::BorrowEnd { loan } => {
                        if state.known_loans.contains(loan) {
                            need(
                                state.loans.remove(loan).is_some(),
                                "update actual Map loan ends once",
                                op,
                            )?;
                        }
                    }
                    Operation::Read {
                        source: PlaceAccess::Loan(loan),
                    } => {
                        if let Some(value) = state.scalars.get(&EntityId::Loan(*loan)).copied() {
                            state.scalars.insert(result(op)?, value);
                        }
                    }
                    Operation::Copy { source } => {
                        if let Some(value) = state.scalars.get(&EntityId::Value(*source)).copied() {
                            state.scalars.insert(result(op)?, value);
                        }
                    }
                    Operation::DirectCall {
                        receiver: None,
                        arguments,
                        ..
                    } => {
                        let &(call, arg) = actual_observes.get(&op.id).ok_or_else(|| {
                            failure("update scanner has no unrelated ordinary Call", op)
                        })?;
                        let [EntityId::Loan(loan)] = arguments.as_slice() else {
                            return Err(failure("update observe actual one Shared loan", op));
                        };
                        let owner = *state.loans.get(loan).ok_or_else(|| {
                            failure("update observe actual loan has original Map root", op)
                        })?;
                        need(
                            state.live.contains(&owner)
                                && state.origins.get(&owner)
                                    == Some(&expected(c, i, flag, state.observes))
                                && i.observes.get(state.observes) == Some(&(call, arg)),
                            "update observe reads exact current source epoch in original source order",
                            (op, &state.origins, flag, state.observes),
                        )?;
                        let end = one(
                            g.instructions.iter().copied().filter(|end| {
                                end.operation == Operation::BorrowEnd { loan: *loan }
                            }),
                            "update unique actual observe argument End",
                        )?;
                        need(
                            end.block == block
                                && end.origin == Origin::Source(span(p, call))
                                && g.position(end)? == g.position(op)? + 1,
                            "update real observe completes before its adjacent argument End",
                            end,
                        )?;
                        state.observes += 1;
                    }
                    Operation::MapPut { owner, .. } => {
                        let put = i
                            .put
                            .ok_or_else(|| failure("update no invented MapPut", op))?;
                        need(
                            flag && op.origin == Origin::Source(span(p, put))
                                && state.live.remove(owner)
                                && state.origins.get(owner) == Some(&i.births[0].expression)
                                && !state.loans.values().any(|v| v == owner),
                            "update true put consumes current MutableMap without extra Drop",
                            op,
                        )?;
                        let current = value(op)?;
                        need(
                            current != *owner && state.live.insert(current),
                            "update put produces a fresh current SSA owner",
                            op,
                        )?;
                        state.origins.insert(current, put);
                        state.puts += 1;
                    }
                    Operation::Drop { owner } => {
                        if let Some(&origin) = state.origins.get(owner) {
                            need(
                                state.live.remove(owner)
                                    && !state.loans.values().any(|v| v == owner),
                                "update each actual epoch is dropped once after all its loans end",
                                op,
                            )?;
                            state.drops.push((origin, op.id));
                        }
                    }
                    Operation::MapConstruct { .. } | Operation::MapConsume { .. } => {
                        return Err(failure(
                            "update Name/replacement control cannot synthesize another Map owner",
                            op,
                        ));
                    }
                    _ => {
                        need(
                            !op.operation.entities().iter().any(|entity| match entity {
                                EntityId::Value(v) => state.origins.contains_key(v),
                                EntityId::Place(v) => state.places.contains_key(v),
                                EntityId::Loan(v) => state.loans.contains_key(v),
                            }),
                            "update all protected Map operands have an exact supported owner operation",
                            op,
                        )?;
                    }
                }
            }
            match &g.term(block)?.kind {
                TerminatorKind::Branch(edge) => {
                    transfer(&g, edge, &mut state)?;
                    block = edge.target;
                }
                TerminatorKind::Conditional {
                    condition,
                    when_true,
                    when_false,
                } => {
                    let (original, _) = i.conditional.ok_or_else(|| {
                        failure("update no invented conditional", g.term(block).unwrap())
                    })?;
                    need(
                        g.term(block)?.origin == Origin::Source(span(p, original))
                            && conditionals == 0,
                        "update actual original conditional write/implicit false edge",
                        g.term(block)?,
                    )?;
                    let selected =
                        *state
                            .scalars
                            .get(&EntityId::Value(*condition))
                            .ok_or_else(|| {
                                failure(
                                    "update actual condition derives original Boolean parameter",
                                    condition,
                                )
                            })?;
                    need(selected == flag, "update original flag polarity", condition)?;
                    conditionals += 1;
                    let edge = if selected { when_true } else { when_false };
                    transfer(&g, edge, &mut state)?;
                    block = edge.target;
                }
                TerminatorKind::Return { values } => {
                    need(
                        values.is_empty()
                            && state.observes == i.observes.len()
                            && state.live.is_empty()
                            && state.loans.is_empty()
                            && conditionals == usize::from(c.code <= 9)
                            && state.puts == usize::from(c.code == 9 && flag),
                        "update every original control path observes current epochs and fully cleans up",
                        (&state.live, &state.loans, state.observes, state.puts),
                    )?;
                    let expected_births = i
                        .births
                        .iter()
                        .filter(|b| {
                            c.code >= 10
                                || c.code == 9
                                || flag
                                || b.expression == i.births[0].expression
                        })
                        .map(|b| b.expression)
                        .collect::<Vec<_>>();
                    need(
                        state.births == expected_births,
                        "update factory executes only in its real original path",
                        (&state.births, &expected_births),
                    )?;
                    let expected_drops = if c.code == 9 {
                        vec![if flag {
                            i.put.unwrap()
                        } else {
                            i.births[0].expression
                        }]
                    } else {
                        expected_births
                    };
                    let actual = state.drops.iter().map(|(e, _)| *e).collect::<Vec<_>>();
                    need(
                        actual.len() == expected_drops.len()
                            && expected_drops.iter().all(|expected| {
                                actual.iter().filter(|actual| *actual == expected).count() == 1
                            }),
                        "update each required path epoch has exactly one real cleanup and no moved-name duplicate",
                        (&actual, &expected_drops),
                    )?;
                    println!(
                        "SCOPE C UPDATE PATH label={} flag={flag} births={:?} drops={:?} blocks={path:?}",
                        c.label, state.births, state.drops
                    );
                    break;
                }
                other => return Err(failure("update original control normal terminal", other)),
            }
        }
    }
    need(
        visited == g.blocks.iter().copied().collect()
            && counted == g.instructions.iter().map(|op| op.id).collect(),
        "update all actual blocks/instructions covered by original Boolean paths",
        (&visited, &counted),
    )?;
    for &block in &g.blocks {
        if block != entry {
            need(
                !g.incoming(block).is_empty(),
                "update actual non-entry full incoming exists",
                block,
            )?;
        }
    }
    Ok(())
}

pub(super) fn put_clones(g: &Graph<'_>, p: &ParsedFile, put: ExpressionId) -> Result<(), Failure> {
    let Expression::Call { arguments, .. } = p.ast().expressions().get(put).unwrap().payload()
    else {
        return Err(failure("update actual put Call AST", put));
    };
    let [key, val] = arguments.as_slice() else {
        return Err(failure("update put actual two value arguments", arguments));
    };
    let op = one(
        g.instructions.iter().copied().filter(|op| {
            op.origin == Origin::Source(span(p, put))
                && matches!(op.operation, Operation::MapPut { .. })
        }),
        "update actual original conditional MapPut",
    )?;
    let Operation::MapPut {
        key: k, value: v, ..
    } = op.operation
    else {
        unreachable!()
    };
    for (arg, actual) in [(key.value, k), (val.value, v)] {
        let clone = g.definition(EntityId::Value(actual))?;
        need(
            clone.origin == Origin::Source(span(p, arg))
                && matches!(clone.operation, Operation::StringClone { .. })
                && clone.block == op.block
                && g.position(clone)? < g.position(op)?
                && g.instructions
                    .iter()
                    .filter(|other| {
                        other
                            .operation
                            .entities()
                            .contains(&EntityId::Value(actual))
                    })
                    .map(|other| other.id)
                    .collect::<Vec<_>>()
                    == [op.id],
            "update actual independent String clone is consumed only by its original put slot",
            clone,
        )?;
    }
    Ok(())
}
