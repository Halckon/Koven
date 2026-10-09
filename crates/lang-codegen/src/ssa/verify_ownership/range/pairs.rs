//! Descriptor/root-loan pairs are correlated on every CFG edge, including backedges.
use super::*;

pub(super) fn compute(function: &Function) -> BTreeSet<(ValueId, LoanId)> {
    let seeds = function
        .instructions
        .iter()
        .filter_map(|instruction| {
            if !matches!(
                instruction.operation,
                Operation::RangeConstruct { .. } | Operation::RangeCall { .. }
            ) {
                return None;
            }
            let [EntityId::Value(view), EntityId::Loan(loan)] = instruction.results.as_slice()
            else {
                return None;
            };
            Some((*view, *loan))
        })
        .collect::<BTreeSet<_>>();
    if seeds.is_empty() {
        return seeds;
    }
    let seed_types = seeds
        .iter()
        .filter_map(|(view, loan)| {
            Some((
                function.entity(EntityId::Value(*view))?.ty,
                function.entity(EntityId::Loan(*loan))?.ty,
            ))
        })
        .collect::<Vec<_>>();
    let mut incoming = BTreeMap::<_, Vec<_>>::new();
    for block in &function.blocks {
        for edge in edges(&block.terminator.as_ref().expect("verified terminator").kind) {
            incoming.entry(edge.target).or_default().push(edge);
        }
    }
    let mut candidates = seeds.clone();
    let mut parameters = Vec::new();
    for block in &function.blocks {
        if !incoming.contains_key(&block.id) {
            continue;
        }
        for (vi, entity) in block.parameters.iter().enumerate() {
            let EntityId::Value(view) = entity else {
                continue;
            };
            for (li, entity) in block.parameters.iter().enumerate() {
                let EntityId::Loan(loan) = entity else {
                    continue;
                };
                if seed_types.iter().any(|(v, l)| {
                    function.entity(EntityId::Value(*view)).map(|d| d.ty) == Some(*v)
                        && function.entity(EntityId::Loan(*loan)).map(|d| d.ty) == Some(*l)
                }) {
                    candidates.insert((*view, *loan));
                    parameters.push((block.id, vi, li, *view, *loan));
                }
            }
        }
    }
    // Greatest fixed point keeps a loop only if every predecessor transports the same pair.
    loop {
        let invalid = parameters
            .iter()
            .filter_map(|(block, vi, li, view, loan)| {
                let valid = incoming[block].iter().all(|edge| {
                    match (edge.arguments.get(*vi), edge.arguments.get(*li)) {
                        (Some(EntityId::Value(v)), Some(EntityId::Loan(l))) => {
                            candidates.contains(&(*v, *l))
                        }
                        _ => false,
                    }
                });
                (!valid).then_some((*view, *loan))
            })
            .collect::<Vec<_>>();
        let mut changed = false;
        for pair in invalid {
            changed |= candidates.remove(&pair);
        }
        if !changed {
            break;
        }
    }
    // Then require a path from a real construct/call; a self-supporting cycle is no proof.
    let mut reachable = seeds;
    loop {
        let mut changed = false;
        for (block, vi, li, view, loan) in &parameters {
            if !candidates.contains(&(*view, *loan)) {
                continue;
            }
            if incoming[block].iter().any(|edge| matches!((edge.arguments.get(*vi),edge.arguments.get(*li)),(Some(EntityId::Value(v)),Some(EntityId::Loan(l))) if reachable.contains(&(*v,*l)))) {
                changed|=reachable.insert((*view,*loan));
            }
        }
        if !changed {
            break;
        }
    }
    reachable
}
