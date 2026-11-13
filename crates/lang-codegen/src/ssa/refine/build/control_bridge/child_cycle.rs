//! Child cycle roles captured before folding, only for the checked symbolic rule.
use super::super::super::child_cycle_receipt::Plan as CyclePlan;
use super::*;
impl Builder<'_, '_> {
    pub(super) fn child_cycle_plan(
        &self,
        nested: &NestedBody<'_>,
        blocks: &[BlockRef; 4],
        roots: &[Slot],
    ) -> Result<Option<CyclePlan>, LoweringError> {
        let Some(rule) = nested.evidence.child_generation_rule_v1() else {
            return Ok(None);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let mut selected = self
            .checked_edge_receipts
            .iter()
            .filter(|receipt| receipt.expression == rule.checked_expression());
        let checked = selected.next().ok_or_else(fail)?;
        let entry = nested.entry.ok_or_else(fail)?;
        let [body, backedge, abort] = checked.blocks;
        let statement = self
            .body
            .blocks
            .get(body.0)
            .and_then(|b| b.statements.get(checked.instruction))
            .ok_or_else(fail)?;
        if selected.next().is_some()
            || !rule.is_consistent()
            || body != blocks[1]
            || !checked.matches(&self.body, statement.span)
            || statement.span != self.span(rule.checked_expression().expression())
            || !matches!(self.body.blocks[entry.0].terminator, Some((RTerm::Goto(target), _)) if target == blocks[0])
            || !matches!(self.body.blocks[backedge.0].terminator, Some((RTerm::Goto(target), _)) if target == blocks[0])
        {
            return Err(fail());
        }
        Ok(Some(CyclePlan {
            control: rule.control(),
            parent_header: nested.parent_header.ok_or_else(fail)?,
            entry,
            header: blocks[0],
            body,
            backedge,
            exit: blocks[2],
            abort,
            roots: roots.to_vec(),
            checked_instruction: checked.instruction,
            checked_operator: checked.operator,
            checked_span: statement.span,
        }))
    }
}
