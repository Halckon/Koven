//! Descriptor/root-loan pairs and provenance resolution across CFG edges.
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

pub(in crate::ssa::verify_ownership) fn is_view(
    module: &Module,
    function: &Function,
    loan: LoanId,
) -> bool {
    function.entity(EntityId::Loan(loan)).is_some_and(|data| {
        matches!(
            module.type_kind(data.ty.semantic_type()),
            Some(super::super::super::model::SsaTypeKind::RangeView { .. })
        )
    })
}

pub(in crate::ssa::verify_ownership) fn root(function: &Function, loan: LoanId) -> Option<LoanId> {
    let pairs = compute(function);
    let flows = LoanFlowAliases::compute(function);
    resolve(function, loan, &pairs, &flows, &mut BTreeSet::new(), true)
}

/// A child's capability remains live even after the original caller loan and parent metadata end.
pub(in crate::ssa::verify_ownership) fn capability(
    function: &Function,
    loan: LoanId,
) -> Option<LoanId> {
    let pairs = compute(function);
    let flows = LoanFlowAliases::compute(function);
    resolve(function, loan, &pairs, &flows, &mut BTreeSet::new(), false)
}

fn resolve(
    function: &Function,
    loan: LoanId,
    pairs: &BTreeSet<(ValueId, LoanId)>,
    flows: &LoanFlowAliases,
    visiting: &mut BTreeSet<LoanId>,
    flatten: bool,
) -> Option<LoanId> {
    if !visiting.insert(loan) {
        return None;
    }
    let result = (|| match function.entity(EntityId::Loan(loan))?.definition {
        Definition::BlockParameter { block, .. } if block.index() == 0 => Some(loan),
        Definition::BlockParameter { .. } => {
            let mut roots = BTreeSet::new();
            for origin in flows.origins.get(&loan)? {
                roots.insert(resolve(function, *origin, pairs, flows, visiting, flatten)?);
            }
            (roots.len() == 1).then(|| *roots.first().expect("one root"))
        }
        Definition::InstructionResult { instruction, .. } => {
            match function.instruction(instruction)?.operation {
                Operation::SharedReborrow { source } | Operation::BorrowCall { source, .. } => {
                    resolve(function, source, pairs, flows, visiting, flatten)
                }
                Operation::RangeConstruct { source, .. } | Operation::RangeCall { source, .. } => {
                    if flatten {
                        resolve(function, source, pairs, flows, visiting, flatten)
                    } else {
                        Some(loan)
                    }
                }
                Operation::BorrowBegin { place, .. }
                    if pairs.iter().any(|(view, _)| {
                        function
                            .entity(EntityId::Value(*view))
                            .map(|data| data.ty.semantic_type())
                            == function
                                .entity(EntityId::Loan(loan))
                                .map(|data| data.ty.semantic_type())
                    }) =>
                {
                    let Definition::InstructionResult { instruction, .. } =
                        function.entity(EntityId::Place(place))?.definition
                    else {
                        return None;
                    };
                    let Operation::RootPlace { owner } =
                        function.instruction(instruction)?.operation
                    else {
                        return None;
                    };
                    let mut roots = BTreeSet::new();
                    for (_, source) in pairs.iter().filter(|(view, _)| *view == owner) {
                        roots.insert(if flatten {
                            resolve(function, *source, pairs, flows, visiting, flatten)?
                        } else {
                            *source
                        });
                    }
                    (roots.len() == 1).then(|| *roots.first().expect("one root"))
                }
                Operation::BorrowBegin { .. } => Some(loan),
                _ => None,
            }
        }
    })();
    visiting.remove(&loan);
    result
}
