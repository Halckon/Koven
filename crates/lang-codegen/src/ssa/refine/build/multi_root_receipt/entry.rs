//! Follow actual, single-definition, ordered entry moves to a catalog producer.
use super::*;
use crate::ssa::refine::body::RStmt;

pub(super) fn producer(statements: &[RStmt], root: Slot, origin: Span) -> Option<Slot> {
    let mut current = root;
    let mut before = statements.len();
    loop {
        let mut definitions = statements
            .iter()
            .enumerate()
            .filter(|(_, statement)| statement.results.contains(&current));
        let (index, statement) = definitions.next()?;
        if definitions.next().is_some() || index >= before || statement.results != [current] {
            return None;
        }
        match statement.op {
            ROp::StringLiteral { .. } if statement.span == origin => return Some(current),
            ROp::Alias { source } => {
                // Each intermediate is consumed by this one move, not another
                // alias/drop/read. The final root may be used later by the loop.
                if statements
                    .iter()
                    .flat_map(|s| s.op.operands())
                    .filter(|operand| *operand == source)
                    .count()
                    != 1
                {
                    return None;
                }
                before = index;
                current = source;
            }
            _ => return None,
        }
    }
}

/// Every incoming prefix path must reach the same untouched initializer.
pub(super) fn prefix(
    body: &RefinedBody,
    entry: BlockRef,
    root: Slot,
    origin: Span,
) -> Option<(BlockRef, Slot)> {
    let mut candidates = body
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(i, b)| producer(&b.statements, root, origin).map(|s| (BlockRef(i), s)));
    let (initialized, slot) = candidates.next()?;
    if candidates.next().is_some() {
        return None;
    }
    fn visit(
        body: &RefinedBody,
        current: BlockRef,
        initialized: BlockRef,
        root: Slot,
        states: &mut [u8],
    ) -> Option<()> {
        let block = &body.blocks[current.0];
        if block
            .terminator
            .as_ref()
            .is_some_and(|(term, _)| term.operands().contains(&root))
        {
            return None;
        }
        if current == initialized {
            let index = block
                .statements
                .iter()
                .position(|s| s.results.contains(&root))?;
            return (!block.statements[index + 1..]
                .iter()
                .any(|s| s.results.contains(&root) || s.op.operands().contains(&root)))
            .then_some(());
        }
        match states[current.0] {
            1 => return None,
            2 => return Some(()),
            _ => {}
        }
        if block
            .statements
            .iter()
            .any(|s| s.results.contains(&root) || s.op.operands().contains(&root))
        {
            return None;
        }
        states[current.0] = 1;
        let predecessors: Vec<_> = body
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| {
                b.terminator
                    .as_ref()
                    .filter(|(t, _)| t.successors().contains(&current))
                    .map(|_| BlockRef(i))
            })
            .collect();
        if predecessors.is_empty() {
            return None;
        }
        for predecessor in predecessors {
            visit(body, predecessor, initialized, root, states)?;
        }
        states[current.0] = 2;
        Some(())
    }
    visit(
        body,
        entry,
        initialized,
        root,
        &mut vec![0; body.blocks.len()],
    )?;
    Some((initialized, slot))
}
