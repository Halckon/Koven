//! Straight-line CFG folding with receipt relocation.
use super::*;

/// Folds a block into its `Goto` predecessor when that predecessor is its only one, so
/// no SSA block merely forwards its parameters.
pub(super) fn merge_straight_lines(
    body: &RefinedBody,
    span: Span,
) -> Result<RefinedBody, LoweringError> {
    let mut body = body.clone();
    loop {
        let reachable = reachable(&body, span)?;
        let mut predecessors: BTreeMap<BlockRef, usize> = BTreeMap::new();
        for block in &reachable {
            let (terminator, _) = body.blocks[block.0].terminator.as_ref().expect("reachable");
            for successor in terminator.successors() {
                *predecessors.entry(successor).or_default() += 1;
            }
        }
        let candidate = reachable.iter().find_map(|block| {
            match body.blocks[block.0].terminator.as_ref().expect("reachable") {
                (RTerm::Goto(successor), _)
                    if successor != block
                        && successor.0 != 0
                        && predecessors.get(successor) == Some(&1)
                        && body.blocks[successor.0].view.is_none()
                        && !body.blocks[successor.0].preserve_header =>
                {
                    Some((*block, *successor))
                }
                _ => None,
            }
        });
        let Some((block, successor)) = candidate else {
            return Ok(body);
        };
        let prefix_len = body.blocks[block.0].statements.len();
        body.child_cycle_plans
            .fold(block, successor, prefix_len, span)?;
        body.nested_body_plans
            .fold(block, successor, prefix_len, span)?;
        body.nested_resume_plans
            .fold(block, successor, prefix_len, span)?;
        body.nested_exit_plans
            .fold(block, successor, prefix_len, span)?;
        body.owner_bridge_plans
            .fold(block, successor, prefix_len, span)?;
        let folded = std::mem::take(&mut body.blocks[successor.0]);
        let target = &mut body.blocks[block.0];
        if target.origin.is_none() {
            target.origin = folded.origin;
        }
        for slot in folded.pinned {
            if !target.pinned.contains(&slot) {
                target.pinned.push(slot);
            }
        }
        target.statements.extend(folded.statements);
        target.terminator = folded.terminator;
        body.branch_plans.fold(block, successor, span)?;
        body.nested_plans.fold(block, successor, span)?;
        body.bounded_plans.fold(block, successor, span)?;
        body.single_plans.fold(block, successor, span)?;
        body.multi_plans.fold(block, successor, span)?;
        body.loop_plans.fold(block, successor, span)?;
        body.fresh_loop_plans.fold(block, successor, span)?;
        body.arm_loop_plans.fold(block, successor, prefix_len);
    }
}
