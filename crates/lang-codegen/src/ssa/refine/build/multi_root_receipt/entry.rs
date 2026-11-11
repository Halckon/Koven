//! Resolve actual moves across all incoming acyclic paths, never infer a phi.
use super::*;

pub(super) struct Entry {
    pub(super) root_block: BlockRef,
    pub(super) producer_block: BlockRef,
    pub(super) producer: Slot,
}

pub(super) fn prefix(
    body: &RefinedBody,
    entry: BlockRef,
    root: Slot,
    origin: Span,
) -> Option<Entry> {
    let mut block = entry;
    let mut before = body.blocks[block.0].statements.len();
    let mut slot = root;
    let mut root_block = None;
    let mut visited = Vec::new();
    loop {
        let (defined, index) = definition(
            body,
            block,
            before,
            slot,
            &mut vec![false; body.blocks.len()],
            &mut BTreeMap::new(),
        )?;
        if visited.contains(&(defined, index, slot)) {
            return None;
        }
        visited.push((defined, index, slot));
        root_block.get_or_insert(defined);
        let statement = &body.blocks[defined.0].statements[index];
        if statement.results != [slot] {
            return None;
        }
        match statement.op {
            ROp::StringLiteral { .. } if statement.span == origin => {
                return Some(Entry {
                    root_block: root_block?,
                    producer_block: defined,
                    producer: slot,
                });
            }
            ROp::Alias { source } => {
                block = defined;
                before = index;
                slot = source;
            }
            _ => return None,
        }
    }
}

/// Find the exact same reaching definition on every predecessor. Uses before the
/// consuming move and cycles are rejected; distinct path definitions are not merged.
fn definition(
    body: &RefinedBody,
    block: BlockRef,
    before: usize,
    slot: Slot,
    active: &mut [bool],
    cached: &mut BTreeMap<(usize, usize), (BlockRef, usize)>,
) -> Option<(BlockRef, usize)> {
    if let Some(found) = cached.get(&(block.0, before)) {
        return Some(*found);
    }
    if active[block.0] {
        return None;
    }
    active[block.0] = true;
    for (index, statement) in body.blocks[block.0].statements[..before]
        .iter()
        .enumerate()
        .rev()
    {
        if statement.op.operands().contains(&slot) {
            return None;
        }
        if statement.results.contains(&slot) {
            active[block.0] = false;
            cached.insert((block.0, before), (block, index));
            return Some((block, index));
        }
    }
    let predecessors: Vec<_> = body
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            b.terminator
                .as_ref()
                .filter(|(term, _)| term.successors().contains(&block))
                .map(|_| BlockRef(i))
        })
        .collect();
    let mut agreed = None;
    for predecessor in predecessors {
        let parent = &body.blocks[predecessor.0];
        if parent.terminator.as_ref()?.0.operands().contains(&slot) {
            return None;
        }
        let found = definition(
            body,
            predecessor,
            parent.statements.len(),
            slot,
            active,
            cached,
        )?;
        if agreed.is_some_and(|previous| previous != found) {
            return None;
        }
        agreed = Some(found);
    }
    active[block.0] = false;
    let found = agreed?;
    cached.insert((block.0, before), found);
    Some(found)
}
