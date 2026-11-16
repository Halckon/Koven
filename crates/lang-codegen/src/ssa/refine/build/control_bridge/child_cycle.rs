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
            initial_base: self.initial_base(nested, roots),
            scalar_entry: nested.scalar_entry.clone(),
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

impl Builder<'_, '_> {
    // Select direct entry own/literal roots or one concat of two literal temporaries.
    // Other initial producers retain existing lowering, without a base claim.
    fn initial_base(&self, nested: &NestedBody<'_>, roots: &[Slot]) -> Option<BlockRef> {
        let initial = nested.parent_initial?;
        let entry = if let Some(prefix) = &nested.scalar_entry {
            let [entry, _, _] = prefix.split?;
            if prefix.join != Some(initial) || entry != BlockRef(0) {
                return None;
            }
            entry
        } else {
            if initial != BlockRef(0) {
                return None;
            }
            initial
        };
        let block = self.body.blocks.get(entry.0)?;
        let mut known = self
            .body
            .parameters
            .iter()
            .map(|s| (*s, *s))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut used = std::collections::BTreeSet::new();
        let mut literals = std::collections::BTreeSet::new();
        let mut concat_count = 0;
        for statement in &block.statements {
            concat_count += usize::from(matches!(statement.op, ROp::StringConcat { .. }));
            let source = match statement.op {
                ROp::StringLiteral { .. } if statement.results.len() == 1 => {
                    literals.insert(statement.results[0]);
                    Some(statement.results[0])
                }
                ROp::Alias { source } => known.get(&source).copied(),
                ROp::StringConcat { left, right }
                    if statement.results.len() == 1
                        && left != right
                        && literals.contains(&left)
                        && literals.contains(&right) =>
                {
                    used.extend([left, right]);
                    Some(statement.results[0])
                }
                _ => {
                    used.extend(
                        statement
                            .op
                            .operands()
                            .iter()
                            .filter_map(|s| known.get(s).copied()),
                    );
                    None
                }
            };
            for result in &statement.results {
                known.remove(result);
                if let Some(source) = source {
                    known.insert(*result, source);
                }
            }
        }
        (concat_count <= 1
            && roots
                .iter()
                .all(|r| known.get(r).is_some_and(|s| !used.contains(s))))
        .then_some(initial)
    }
}
