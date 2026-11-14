//! Optional raw-CFG candidate for a preserved resumed root, never a new permission.
use super::super::super::nested_resume_receipt::{Carry, CarryChecked, Plan as ResumePlan};
use super::*;
use std::collections::BTreeSet;

impl Builder<'_, '_> {
    pub(super) fn nested_resume_carry(&self, plan: &ResumePlan) -> Option<Carry> {
        let nested = self.nested_owner_body.as_ref()?;
        if nested.evidence.parent() != plan.parent || nested.evidence.control() != plan.child {
            return None;
        }
        let header = nested.parent_header?;
        if header == plan.resume {
            return None;
        }
        let mut current = plan.resume;
        let mut start = plan.positions[0].checked_add(1)?;
        let mut seen = BTreeSet::new();
        let mut edges = Vec::new();
        let mut checked = Vec::new();
        // Stop at the captured parent's header: never inspect a hypothetical next iteration.
        while current != header {
            if !seen.insert(current) {
                return None;
            }
            let block = self.body.blocks.get(current.0)?;
            if block
                .statements
                .get(start..)?
                .iter()
                .any(|s| s.results.contains(&plan.root) || s.op.operands().contains(&plan.root))
            {
                return None;
            }
            let (term, _) = block.terminator.as_ref()?;
            if term.operands().contains(&plan.root) {
                return None;
            }
            let next = match term {
                RTerm::Goto(next) => *next,
                RTerm::Branch { .. } => {
                    let mut receipts = self
                        .checked_edge_receipts
                        .iter()
                        .filter(|r| r.blocks[0] == current);
                    let receipt = receipts.next()?;
                    if receipts.next().is_some() {
                        return None;
                    }
                    let statement = block.statements.get(receipt.instruction)?;
                    // matches verifies actual instruction/operands, branch polarity and terminal abort.
                    if statement.span != self.span(receipt.expression.expression())
                        || !receipt.matches(&self.body, statement.span)
                    {
                        return None;
                    }
                    let [body, success, abort] = receipt.blocks;
                    let aborted = self.body.blocks.get(abort.0)?;
                    let (abort_term, _) = aborted.terminator.as_ref()?;
                    if !abort_term.successors().is_empty()
                        || abort_term.operands().contains(&plan.root)
                        || !aborted.statements.is_empty()
                        || abort == header
                    {
                        return None;
                    }
                    checked.push(CarryChecked {
                        body,
                        success,
                        abort,
                        instruction: receipt.instruction,
                        span: statement.span,
                        operator: receipt.operator,
                    });
                    success
                }
                _ => return None,
            };
            // Every edge endpoint must be a real block even when it is the stopping header.
            self.body.blocks.get(next.0)?;
            edges.push((current, next));
            current = next;
            start = 0;
        }
        Some(Carry {
            header,
            edges,
            checked,
        })
    }
}
