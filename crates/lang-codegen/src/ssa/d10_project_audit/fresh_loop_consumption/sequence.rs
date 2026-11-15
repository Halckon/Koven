//! Atomic certification of exactly two original sequential fresh-loop scopes.
use super::*;
use lang_frontend::{
    parser::{Expression, Statement},
    type_checking::UnitStatementId,
};

pub(super) fn original_order(inputs: &RefineInputs<'_>) -> Option<[UnitStatementId; 2]> {
    let crate::ssa::refine::RefineBody::Block(body) = inputs.body else {
        return None;
    };
    let ast = inputs.parsed.ast();
    let Statement::Block { elements } = ast.statements().get(body).ok()?.payload() else {
        return None;
    };
    let [first, second] = elements.as_slice() else {
        return None;
    };
    let mut flag = None;
    for statement in [first, second] {
        let Statement::While { condition, .. } = ast.statements().get(*statement).ok()?.payload()
        else {
            return None;
        };
        let node = ast.expressions().get(*condition).ok()?;
        if !matches!(node.payload(), Expression::Name) {
            return None;
        }
        let symbol = *inputs
            .references
            .get(&(node.span().start(), node.span().end()))?;
        if flag.is_some_and(|previous| previous != symbol) {
            return None;
        }
        flag = Some(symbol);
    }
    Some([
        UnitStatementId::new(inputs.source_unit, *first),
        UnitStatementId::new(inputs.source_unit, *second),
    ])
}

pub(super) fn finish(
    transaction: &Transaction,
    state: &State,
    group: &[&Expected],
    program: &Program,
    mapping: &BTreeMap<UnitFunctionInstanceKey, FunctionId>,
    owned: &dyn UnitLoweringFacts,
    used: &mut BTreeSet<usize>,
) -> Option<[Edge; 2]> {
    let [anchor, _] = group else {
        return None;
    };
    let order = anchor.sequence?;
    if order[0] == order[1] {
        return None;
    }
    let mut expected = Vec::new();
    for statement in order {
        let matches: Vec<_> = group
            .iter()
            .copied()
            .filter(|e| e.fact.loop_statement() == statement)
            .collect();
        let [e] = matches.as_slice() else {
            return None;
        };
        if e.sequence != Some(order)
            || e.key != anchor.key
            || e.fact.callable() != anchor.fact.callable()
            || e.fact.source() != anchor.fact.source()
            || e.abi != anchor.abi
            || e.source_index != anchor.source_index
            || e.fact.consumers().len() != 1
        {
            return None;
        }
        expected.push(*e);
    }
    let UnitCallableTarget::Declaration(declaration) = anchor.key.target() else {
        return None;
    };
    if anchor.key != UnitFunctionInstanceKey::for_entry(declaration)
        || transaction.planned.get(&anchor.token)
            != Some(&(anchor.key.clone(), anchor.fact.callable()))
    {
        return None;
    }
    let originals: Vec<_> = owned
        .no_carry_loops()
        .iter()
        .filter(|f| f.callable() == anchor.fact.callable())
        .collect();
    if originals.len() != 2 {
        return None;
    }
    for e in &expected {
        let matches: Vec<_> = originals
            .iter()
            .filter(|f| f.loop_statement() == e.fact.loop_statement())
            .collect();
        let [original] = matches.as_slice() else {
            return None;
        };
        if !owned.owns_no_carry_loop(&e.fact)
            || !original.same_identity(&e.fact)
            || ***original != e.fact
            || owned.d10_fresh_loop_link(e.fact.loop_statement()).ok() != Some(e.fact.callable())
        {
            return None;
        }
    }
    let first = &expected[0].fact;
    let second = &expected[1].fact;
    if first.binding() == second.binding()
        || first.body() == second.body()
        || first.initializer() == second.initializer()
    {
        return None;
    }
    let caller = *mapping.get(&anchor.key)?;
    if mapping.values().filter(|id| **id == caller).count() != 1 {
        return None;
    }
    let matches: Vec<_> = state
        .snapshots
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            s.receipt.caller == caller
                || s.receipt
                    .loops
                    .iter()
                    .any(|l| order.contains(&l.fact.loop_statement()))
        })
        .collect();
    let [(index, snapshot)] = matches.as_slice() else {
        return None;
    };
    if used.contains(index)
        || snapshot.receipt.caller != caller
        || snapshot.receipt.loops.len() != 2
        || !snapshot.receipt.has_empty_segments(3)
    {
        return None;
    }
    // Validate the complete caller CFG once, including both loop exits and their connecting edge.
    let function = validate_caller(snapshot, program, anchor.abi?, anchor.source_index?)?;
    let mut edges = Vec::new();
    for (index, (e, local)) in expected.iter().zip(&snapshot.receipt.loops).enumerate() {
        if !local.fact.same_identity(&e.fact) || local.fact != e.fact {
            return None;
        }
        let calls = e.calls.as_ref()?;
        if calls.len() != 2 {
            return None;
        }
        for (key, id) in calls {
            if mapping.get(key) != Some(id)
                || mapping.values().filter(|v| *v == id).count() != 1
                || program
                    .modules
                    .iter()
                    .flat_map(|m| &m.functions)
                    .filter(|f| f.id == *id)
                    .count()
                    != 1
            {
                return None;
            }
        }
        edges.push(validate_loop(snapshot, function, calls, index)?);
    }
    let edges: [Edge; 2] = edges.try_into().ok()?;
    if edges[0] == edges[1] {
        return None;
    }
    used.insert(*index);
    Some(edges)
}
