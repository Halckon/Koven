//! Actual straight-line controls prove Value delivery/Return rather than absence of Drop.
mod provider;
use super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, function, result, value},
    query_inputs::one,
};
use super::{
    Case,
    ast::{self, Input},
};
use crate::ssa::model::*;
use lang_frontend::{
    ast::ExpressionId,
    parser::{Expression, ParsedFile},
};
use std::collections::{BTreeMap, BTreeSet};
pub(super) fn check(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    proof: &super::facts::Proof,
    program: &Program,
    entry: FunctionId,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "D actual original module")?;
    need(
        function(module, i.entry)?.id == entry,
        "D original returned entry selection",
        entry,
    )?;
    if c.provider {
        provider::check(c, p, i, proof, module)?;
    } else {
        control(c, p, i, module, false)?;
    }
    control(c, p, i, module, true)?;
    for (scope, origin) in [(i.scan, proof.final_origin), (i.entry, proof.caller_origin)] {
        if let Some(origin) = origin {
            let f = function(module, scope)?;
            let g = Graph::new(f)?;
            let drop=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::Drop{owner}if module.map_container(f.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())),"D exact only Map cleanup in current function")?;
            need(
                drop.origin == Origin::Source(origin),
                "D actual Map cleanup retains same-analysis published origin",
                drop,
            )?;
        }
    }
    Ok(())
}
fn control(c: &Case, p: &ParsedFile, i: &Input, m: &Module, caller: bool) -> Result<(), Failure> {
    let f = function(m, if caller { i.entry } else { i.scan })?;
    let g = Graph::new(f)?;
    println!(
        "SCOPE D FUTURE SSA label={} caller={caller} function={f:#?}",
        c.label
    );
    need(
        g.blocks.len() == 1
            && g.edges.is_empty()
            && f.receiver.is_none()
            && f.borrow_return.is_none()
            && f.carrier_return.is_none(),
        "D original control exact straight-line CFG and ordinary ABI",
        f.id,
    )?;
    let block = g.block(
        f.entry_block()
            .ok_or_else(|| failure("D actual entry block", f.id))?,
    )?;
    let mut current = None;
    let mut tracked = BTreeSet::new();
    let mut places = BTreeMap::new();
    let mut loans = BTreeMap::new();
    let mut known_loans = BTreeSet::new();
    if !caller && c.code != 21 {
        let [EntityId::Value(owner)] = block.parameters.as_slice() else {
            return Err(failure("D own Map parameter is actual Value ABI", block));
        };
        need(
            m.map_container(
                f.entity(EntityId::Value(*owner))
                    .unwrap()
                    .ty
                    .semantic_type(),
            )
            .is_some(),
            "D own parameter actual Map representation",
            owner,
        )?;
        current = Some(*owner);
        tracked.insert(*owner);
    } else {
        need(
            block.parameters.is_empty(),
            "D actual zero-argument original entry/scanner",
            block,
        )?;
    }
    let scan = function(m, i.scan)?;
    let births = if caller { i.entry_birth } else { i.birth };
    let factory = if births.is_some() {
        Some(function(m, i.factory)?)
    } else {
        None
    };
    let mut observes = if caller {
        i.entry_observe.into_iter().collect::<Vec<_>>()
    } else {
        i.observes.clone()
    };
    observes.sort_by_key(|(e, _)| ast::span(p, *e).start());
    let observer = if observes.is_empty() {
        None
    } else {
        Some(function(m, i.observer)?)
    };
    let mut observe_index = 0;
    let mut births_count = 0;
    let mut calls = 0;
    let mut puts = 0;
    let mut drops = 0;
    let mut returned = false;
    for op in g.ops(block.id)? {
        if let Some(birth) = births
            && op.origin == Origin::Source(ast::span(p, birth))
            && matches!(op.operation, Operation::DirectCall { .. })
        {
            let actual = factory.unwrap();
            need(
                op.operation
                    == Operation::DirectCall {
                        callee: actual.id,
                        receiver: None,
                        arguments: vec![],
                    }
                    && current.is_none(),
                "D actual original factory installs sole owned epoch",
                op,
            )?;
            let owner = value(op)?;
            need(
                m.map_container(f.entity(EntityId::Value(owner)).unwrap().ty.semantic_type())
                    .is_some()
                    && tracked.insert(owner),
                "D actual fresh factory Map result",
                op,
            )?;
            current = Some(owner);
            births_count += 1;
            continue;
        }
        if caller
            && op.origin == Origin::Source(ast::span(p, i.scan_call))
            && matches!(op.operation, Operation::DirectCall { .. })
        {
            let arguments = if c.code == 21 {
                vec![]
            } else {
                vec![EntityId::Value(current.take().ok_or_else(|| {
                    failure("D caller owns actual factory argument", op)
                })?)]
            };
            need(
                op.operation
                    == Operation::DirectCall {
                        callee: scan.id,
                        receiver: None,
                        arguments,
                    }
                    && loans.is_empty(),
                "D actual selected scanner consumes exact Value factory argument",
                op,
            )?;
            if c.code == 19 {
                let owner = value(op)?;
                need(
                    tracked.insert(owner)
                        && m.map_container(
                            f.entity(EntityId::Value(owner)).unwrap().ty.semantic_type(),
                        )
                        .is_some()
                        && scan.return_types.as_slice()
                            == [f.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()],
                    "D actual scanner result is a new owned Call result with matching ABI",
                    op,
                )?;
                current = Some(owner);
            } else {
                need(
                    op.results.is_empty() && scan.return_types.is_empty(),
                    "D void scanner has no synthetic returned owner",
                    op,
                )?;
            }
            calls += 1;
            continue;
        }
        match &op.operation {
            Operation::RootPlace { owner } if tracked.contains(owner) => {
                need(
                    current == Some(*owner),
                    "D exact current live root place",
                    op,
                )?;
                let EntityId::Place(place) = result(op)? else {
                    return Err(failure("D root place result", op));
                };
                places.insert(place, *owner);
            }
            Operation::BorrowBegin { place, kind } if places.contains_key(place) => {
                let owner = places[place];
                need(
                    *kind == LoanKind::Shared && current == Some(owner),
                    "D actual current owner Shared borrow",
                    op,
                )?;
                let EntityId::Loan(loan) = result(op)? else {
                    return Err(failure("D actual loan result", op));
                };
                need(
                    loans.insert(loan, owner).is_none() && known_loans.insert(loan),
                    "D independent loan birth",
                    op,
                )?;
            }
            Operation::SharedReborrow { source } if loans.contains_key(source) => {
                let owner = loans[source];
                let EntityId::Loan(loan) = result(op)? else {
                    return Err(failure("D child loan result", op));
                };
                need(
                    loans.insert(loan, owner).is_none() && known_loans.insert(loan),
                    "D independent current-owner child loan",
                    op,
                )?;
            }
            Operation::BorrowEnd { loan } if known_loans.contains(loan) => {
                need(
                    loans.remove(loan).is_some(),
                    "D each Map loan ends exactly once",
                    op,
                )?;
            }
            Operation::DirectCall {
                callee,
                receiver: None,
                arguments,
            } => {
                let (call, arg) = *observes
                    .get(observe_index)
                    .ok_or_else(|| failure("D no unrelated actual Call in original control", op))?;
                let [EntityId::Loan(loan)] = arguments.as_slice() else {
                    return Err(failure("D actual Borrow observe argument", op));
                };
                need(
                    *callee == observer.unwrap().id
                        && op.origin == Origin::Source(ast::span(p, call))
                        && loans.get(loan).copied() == current
                        && current.is_some(),
                    "D actual observation reads current owned epoch after Return/MoveName/put",
                    op,
                )?;
                let begin = g.definition(EntityId::Loan(*loan))?;
                need(
                    begin.origin == Origin::Source(ast::span(p, arg)),
                    "D exact original observer argument origin",
                    begin,
                )?;
                let end = one(
                    g.instructions
                        .iter()
                        .copied()
                        .filter(|end| end.operation == Operation::BorrowEnd { loan: *loan }),
                    "D actual observe sole CallReturn End",
                )?;
                need(
                    end.block == op.block
                        && end.origin == Origin::Source(ast::span(p, call))
                        && g.position(end)? == g.position(op)? + 1,
                    "D actual observer immediately ends only its argument loan",
                    end,
                )?;
                observe_index += 1;
            }
            Operation::MapPut {
                owner,
                key,
                value: val,
            } => {
                let put = i.put.ok_or_else(|| failure("D no unrelated MapPut", op))?;
                need(
                    !caller
                        && c.code == 21
                        && current == Some(*owner)
                        && loans.is_empty()
                        && op.origin == Origin::Source(ast::span(p, put)),
                    "D actual put consumes sole current owner after earlier observation End",
                    op,
                )?;
                clones(&g, p, put, op, [*key, *val])?;
                let updated = value(op)?;
                need(
                    updated != *owner && tracked.insert(updated),
                    "D actual put new current owner",
                    op,
                )?;
                current = Some(updated);
                puts += 1;
            }
            Operation::Drop { owner } if tracked.contains(owner) => {
                need(
                    current == Some(*owner) && loans.is_empty(),
                    "D unique live Map Drop occurs only after all loans end",
                    op,
                )?;
                current = None;
                drops += 1;
            }
            Operation::MapConstruct { .. } | Operation::MapConsume { .. } => {
                return Err(failure("D original control cannot invent a Map birth", op));
            }
            _ => {
                need(
                    !op.operation.entities().iter().any(|e| match e {
                        EntityId::Value(v) => tracked.contains(v),
                        EntityId::Place(v) => places.contains_key(v),
                        EntityId::Loan(v) => known_loans.contains(v),
                    }),
                    "D all protected old/current Map operands accounted for",
                    op,
                )?;
            }
        }
    }
    let term = g.term(block.id)?;
    if !caller && c.code == 19 {
        let (ret, _) = i
            .returned
            .ok_or_else(|| failure("D original Return AST", i))?;
        need(
            matches!(&term.kind,TerminatorKind::Return{values}if values.as_slice()==current.as_slice())
                && current.is_some()
                && term.origin == Origin::Source(ast::span(p, ret))
                && drops == 0
                && loans.is_empty(),
            "D actual Return directly delivers original own Map parameter, without Drop",
            term,
        )?;
        returned = true;
        current = None;
    } else {
        need(
            matches!(&term.kind,TerminatorKind::Return{values}if values.is_empty())
                && current.is_none()
                && loans.is_empty(),
            "D actual void Return after transfer or sole cleanup",
            term,
        )?;
    }
    let expected_drops = usize::from(if caller { c.code == 19 } else { c.code != 19 });
    need(
        observe_index == observes.len()
            && births_count == usize::from(births.is_some())
            && calls == usize::from(caller)
            && puts == usize::from(!caller && c.code == 21)
            && drops == expected_drops
            && returned == (!caller && c.code == 19)
            && current.is_none(),
        "D complete actual owner handoff/Return/put/unique cleanup counts",
        (observe_index, births_count, calls, puts, drops, returned),
    )?;
    let maps = f
        .values
        .iter()
        .enumerate()
        .filter_map(|(index, data)| {
            m.map_container(data.ty.semantic_type())
                .is_some()
                .then_some(ValueId {
                    function: f.id,
                    index,
                })
        })
        .collect::<BTreeSet<_>>();
    need(
        maps == tracked,
        "D every actual Map Value belongs to original parameter/Call/put result",
        (&maps, &tracked),
    )?;
    println!(
        "SCOPE D FUTURE CONTROL label={} caller={caller} observes={observe_index} births={births_count} scan_calls={calls} puts={puts} drops={drops} returned={returned}",
        c.label
    );
    Ok(())
}
fn clones(
    g: &Graph<'_>,
    p: &ParsedFile,
    put: ExpressionId,
    op: &Instruction,
    values: [ValueId; 2],
) -> Result<(), Failure> {
    let Expression::Call { arguments, .. } = p.ast().expressions().get(put).unwrap().payload()
    else {
        return Err(failure("D actual put AST", put));
    };
    let [key, val] = arguments.as_slice() else {
        return Err(failure("D actual put two argument slots", arguments));
    };
    for (arg, actual) in [(key.value, values[0]), (val.value, values[1])] {
        let clone = g.definition(EntityId::Value(actual))?;
        need(
            clone.origin == Origin::Source(ast::span(p, arg))
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
            "D independent actual clone owner consumed by exact original put slot",
            clone,
        )?;
    }
    Ok(())
}
