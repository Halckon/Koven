//! Actual paired owner/view CFG columns, with one active outer-while birth.
use super::*;
use crate::ssa::model::Definition;
use std::collections::BTreeSet;

impl ExpressionLowerer<'_> {
    /// Locate the active outer branch and its real root slot, not a similarly typed loan.
    pub(super) fn numeric_while_birth(&self, root: SymbolId, proof: NonNullProof) -> Option<Pair> {
        let (statement, birth) = proof.while_origin()?;
        let [outer, rest @ ..] = self.loops.as_slice() else {
            return None;
        };
        if rest.len() > 1
            || rest.iter().any(|context| context.numeric_loop.is_none())
            || outer.for_loop.is_some()
            || outer.numeric_loop.is_some()
            || !outer.entry_views.is_empty()
            || !outer.breaks.is_empty()
            || !outer.continues.is_empty()
        {
            return None;
        }
        let term = self.function.block(outer.header)?.terminator.as_ref()?;
        let TerminatorKind::NullableBranch {
            owner,
            view,
            when_non_null,
            when_null,
        } = &term.kind
        else {
            return None;
        };
        let span = self.parsed.ast().statements().get(statement).ok()?.span();
        if *view != birth
            || term.origin != Origin::Source(span)
            || when_non_null.target == when_null.target
            || incoming(self.function, when_non_null.target).as_slice()
                != [(outer.header, when_non_null)]
        {
            return None;
        }
        let (header, column) = parameter(self.function, EntityId::Value(*owner))?;
        let slot = outer.carried.slots.get(column)?;
        if header != outer.header
            || slot.symbol != Some(root)
            || outer
                .carried
                .slots
                .iter()
                .filter(|slot| slot.symbol == Some(root))
                .count()
                != 1
            || when_non_null.arguments.get(column) != Some(&EntityId::Value(*owner))
        {
            return None;
        }
        let EntityId::Value(owner) = *self
            .function
            .block(when_non_null.target)?
            .parameters
            .get(column)?
        else {
            return None;
        };
        let pair = Pair {
            block: when_non_null.target,
            owner,
            loan: birth,
        };
        pair_columns(self.function, pair, proof.inner, self.ssa_types)?;
        Some(pair)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Pair {
    pub(super) block: BlockId,
    pub(super) owner: ValueId,
    pub(super) loan: LoanId,
}

fn parameter(function: &Function, entity: EntityId) -> Option<(BlockId, usize)> {
    let Definition::BlockParameter { block, index } = function.entity(entity)?.definition else {
        return None;
    };
    (function.block(block)?.parameters.get(index) == Some(&entity)).then_some((block, index))
}
fn pair_columns(
    function: &Function,
    pair: Pair,
    inner: SsaTypeId,
    types: &[SsaTypeKind],
) -> Option<(usize, usize)> {
    let (owner_block, owner) = parameter(function, EntityId::Value(pair.owner))?;
    let (loan_block, loan) = parameter(function, EntityId::Loan(pair.loan))?;
    let EntityType::Value(nullable) = function.entity(EntityId::Value(pair.owner))?.ty else {
        return None;
    };
    (owner_block == pair.block
        && loan_block == pair.block
        && owner != loan
        && types.get(nullable.index()) == Some(&SsaTypeKind::NullableHandle { inner })
        && function.entity(EntityId::Loan(pair.loan))?.ty
            == (EntityType::Loan {
                kind: LoanKind::Shared,
                target: inner,
            }))
    .then_some((owner, loan))
}
fn incoming(function: &Function, target: BlockId) -> Vec<(BlockId, &Edge)> {
    function
        .blocks
        .iter()
        .flat_map(|block| {
            let edges = match block.terminator.as_ref().map(|term| &term.kind) {
                Some(TerminatorKind::Branch(edge)) => vec![edge],
                Some(TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                }) => vec![when_true, when_false],
                Some(TerminatorKind::NullableBranch {
                    when_null,
                    when_non_null,
                    ..
                }) => vec![when_null, when_non_null],
                _ => Vec::new(),
            };
            edges
                .into_iter()
                .filter(move |edge| edge.target == target)
                .map(move |edge| (block.id, edge))
        })
        .collect()
}

/// Follow paired columns together. Every predecessor is checked, and every
/// visited node must have a path to the unique actual birth. A closed SCC with
/// no entry cannot become evidence merely because its pair was visited before.
pub(super) fn transport(
    function: &Function,
    current: Pair,
    anchor: Pair,
    inner: SsaTypeId,
    types: &[SsaTypeKind],
) -> bool {
    let mut graph = BTreeMap::<Pair, Vec<Pair>>::new();
    let mut work = vec![current];
    while let Some(pair) = work.pop() {
        if graph.contains_key(&pair) {
            continue;
        }
        let Some((owner, loan)) = pair_columns(function, pair, inner, types) else {
            return false;
        };
        if pair == anchor {
            graph.insert(pair, Vec::new());
            continue;
        }
        let predecessors = incoming(function, pair.block);
        if predecessors.is_empty() {
            return false;
        }
        let mut parents = Vec::new();
        for (block, edge) in predecessors {
            if !matches!(
                function
                    .block(block)
                    .and_then(|block| block.terminator.as_ref())
                    .map(|term| &term.kind),
                Some(TerminatorKind::Branch(_) | TerminatorKind::Conditional { .. })
            ) {
                return false;
            }
            let (Some(EntityId::Value(value)), Some(EntityId::Loan(view))) =
                (edge.arguments.get(owner), edge.arguments.get(loan))
            else {
                return false;
            };
            let Some((source_block, _)) = parameter(function, EntityId::Value(*value)) else {
                return false;
            };
            let source = Pair {
                block: source_block,
                owner: *value,
                loan: *view,
            };
            if source_block != block
                || pair_columns(function, source, inner, types).is_none()
                || edge
                    .arguments
                    .iter()
                    .filter(|entity| **entity == EntityId::Value(*value))
                    .count()
                    != 1
                || edge
                    .arguments
                    .iter()
                    .filter(|entity| **entity == EntityId::Loan(*view))
                    .count()
                    != 1
            {
                return false;
            }
            parents.push(source);
            work.push(source);
        }
        graph.insert(pair, parents);
    }
    if !graph.contains_key(&anchor) {
        return false;
    }
    let mut anchored = BTreeSet::from([anchor]);
    loop {
        let before = anchored.len();
        for (&pair, parents) in &graph {
            if parents.iter().any(|parent| anchored.contains(parent)) {
                anchored.insert(pair);
            }
        }
        if before == anchored.len() {
            return anchored.len() == graph.len();
        }
    }
}

/// The consumer's pair must be the actual numerical exhaustion column. Check
/// the zero-iteration edge and every emitted terminal/increment pair against the
/// complete frontend exit list, including Continue and normal fallthrough.
pub(super) fn numeric_exit(
    lowerer: &ExpressionLowerer<'_>,
    plan: &Plan,
    current: Pair,
    anchor: Pair,
) -> bool {
    let Some(iteration) = lowerer.owned.iteration(plan.statement) else {
        return false;
    };
    let Some(proof) = lowerer.non_null_bindings.get(&plan.root) else {
        return false;
    };
    let Some((owner, loan)) =
        pair_columns(lowerer.function, current, proof.inner, lowerer.ssa_types)
    else {
        return false;
    };
    let Ok(node) = lowerer.parsed.ast().statements().get(plan.statement) else {
        return false;
    };
    let span = node.span();
    if lowerer
        .function
        .block(current.block)
        .map(|block| &block.origin)
        != Some(&Origin::Source(span))
    {
        return false;
    }
    let Some(term) = lowerer
        .function
        .block(anchor.block)
        .and_then(|block| block.terminator.as_ref())
    else {
        return false;
    };
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &term.kind
    else {
        return false;
    };
    if term.origin != Origin::Source(span)
        || when_true.target != current.block
        || when_false.target == current.block
        || when_true.arguments.get(owner) != Some(&EntityId::Value(anchor.owner))
        || when_true.arguments.get(loan) != Some(&EntityId::Loan(anchor.loan))
        || when_false.arguments.get(owner) != Some(&EntityId::Value(anchor.owner))
        || when_false.arguments.get(loan) != Some(&EntityId::Loan(anchor.loan))
    {
        return false;
    }
    let header = when_false.target;
    let edges = incoming(lowerer.function, current.block);
    let mut expected = Vec::new();
    for exit in iteration.exits() {
        match exit.kind() {
            IterationExitKind::Continue(expression) => {
                let Ok(node) = lowerer.parsed.ast().expressions().get(expression) else {
                    return false;
                };
                expected.push(node.span());
            }
            IterationExitKind::Fallthrough => expected.push(span),
            IterationExitKind::Exhaustion => {}
            _ => return false,
        }
    }
    if edges.len() != expected.len() + 1 || !edges.contains(&(anchor.block, when_true)) {
        return false;
    }
    let mut header_sources = vec![(anchor.block, when_false)];
    for (block, edge) in edges {
        if block == anchor.block {
            continue;
        }
        let Some(term) = lowerer
            .function
            .block(block)
            .and_then(|block| block.terminator.as_ref())
        else {
            return false;
        };
        let Origin::Source(origin) = &term.origin else {
            return false;
        };
        let Some(index) = expected.iter().position(|span| *span == *origin) else {
            return false;
        };
        expected.remove(index);
        let TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } = &term.kind
        else {
            return false;
        };
        if edge != when_true || when_false.target == current.block {
            return false;
        }
        let Some(increment) = lowerer.function.block(when_false.target) else {
            return false;
        };
        let Some(term) = increment.terminator.as_ref() else {
            return false;
        };
        let TerminatorKind::Branch(backedge) = &term.kind else {
            return false;
        };
        if backedge.target != header
            || term.origin != Origin::Source(*origin)
            || incoming(lowerer.function, increment.id).as_slice() != [(block, when_false)]
        {
            return false;
        }
        header_sources.push((increment.id, backedge));
    }
    let incoming_header = incoming(lowerer.function, header);
    expected.is_empty()
        && incoming_header.len() == header_sources.len()
        && incoming_header
            .iter()
            .all(|edge| header_sources.contains(edge))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
