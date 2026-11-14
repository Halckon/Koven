//! Mandatory scalar Alias emission observations; no new owner or initializer authority.
use super::*;
#[derive(Clone, Debug)]
pub(in super::super) struct Entry {
    pub control: UnitStatementId,
    pub index: usize,
    pub block: BlockRef,
    pub instruction: usize,
    pub source: Slot,
    pub target: Slot,
    pub span: Span,
}
impl Plans {
    pub(in super::super) fn with_scalar_entries(
        mut self,
        entries: Vec<Entry>,
        span: Span,
    ) -> Result<Self, LoweringError> {
        if self.plan.is_some() || self.group.len() != 2 || self.scalar_entries.is_some() {
            return Err(invalid(span));
        }
        let leading = self.leading.as_ref().ok_or_else(|| invalid(span))?;
        let mut controls = self.group.iter().map(|m| m.control).collect::<Vec<_>>();
        if leading.scalar.is_some() {
            controls.insert(0, leading.control);
        }
        if entries.len() != controls.len() * 2 {
            return Err(invalid(span));
        }
        for (i, entry) in entries.iter().enumerate() {
            if !controls.contains(&entry.control)
                || entry.index > 1
                || entry.source == entry.target
                || entries[..i].iter().any(|p| {
                    (p.control, p.index) == (entry.control, entry.index)
                        || (p.block, p.instruction) == (entry.block, entry.instruction)
                        || p.target == entry.target
                })
            {
                return Err(invalid(span));
            }
            if entry.control == leading.control
                && leading
                    .scalar
                    .is_none_or(|s| s[entry.index] != entry.target)
            {
                return Err(invalid(span));
            }
        }
        for control in controls {
            let a = entries
                .iter()
                .find(|e| e.control == control && e.index == 0)
                .ok_or_else(|| invalid(span))?;
            let b = entries
                .iter()
                .find(|e| e.control == control && e.index == 1)
                .ok_or_else(|| invalid(span))?;
            if a.block != b.block || a.instruction >= b.instruction {
                return Err(invalid(span));
            }
            if control != leading.control
                && self
                    .links
                    .iter()
                    .find(|l| l.to_control == control)
                    .is_none_or(|link| link.block != a.block)
            {
                return Err(invalid(span));
            }
        }
        self.scalar_entries = Some(entries);
        Ok(self)
    }
    pub(super) fn fold_scalar_entries(
        &mut self,
        block: BlockRef,
        successor: BlockRef,
        prefix: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        if let Some(entries) = &mut self.scalar_entries {
            for entry in entries {
                if entry.block == successor {
                    entry.block = block;
                    entry.instruction = entry
                        .instruction
                        .checked_add(prefix)
                        .ok_or_else(|| invalid(span))?;
                    #[cfg(test)]
                    if super::super::build::owner_consumer_test_support::take_fault(
                        super::super::build::owner_consumer_test_support::OwnerFault::ChainScalarSsaWrongFoldOffset,
                    ) {
                        entry.instruction = entry.instruction.checked_add(1).ok_or_else(|| invalid(span))?;
                        super::super::build::owner_consumer_test_support::record_action("control-chain-scalar-ssa-fault-hit", 1);
                    }
                }
            }
        }
        Ok(())
    }
}
#[derive(Default)]
pub(super) struct Observed {
    values: BTreeMap<(UnitStatementId, usize), EntityId>,
}
fn actual_alias(body: &RefinedBody, e: &Entry) -> bool {
    body.blocks
        .get(e.block.0)
        .and_then(|b| b.statements.get(e.instruction))
        .is_some_and(|s| {
            s.span == e.span
                && s.results.as_slice() == [e.target]
                && matches!(s.op, ROp::Alias {source} if source == e.source)
        })
}
impl Observed {
    pub(super) fn observe(
        &mut self,
        body: &RefinedBody,
        position: (BlockRef, usize),
        instruction: Option<InstructionId>,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(entries) = &body.owner_bridge_plans.scalar_entries else {
            return Ok(());
        };
        let mut matching = entries
            .iter()
            .filter(|e| (e.block, e.instruction) == position);
        let Some(e) = matching.next() else {
            return Ok(());
        };
        #[cfg(test)]
        if e.index == 1 && body.owner_bridge_plans.group.last().is_some_and(|m| m.control == e.control)
            && super::super::build::owner_consumer_test_support::take_fault(
                super::super::build::owner_consumer_test_support::OwnerFault::ChainScalarSsaMissingLast,
            ) {
            super::super::build::owner_consumer_test_support::record_action("control-chain-scalar-ssa-fault-hit", 0);
            return Ok(());
        }
        let value = current
            .get(&e.source)
            .copied()
            .ok_or_else(|| invalid(span))?;
        if matching.next().is_some()
            || instruction.is_some()
            || !actual_alias(body, e)
            || !matches!(value, EntityId::Value(_))
            || current.get(&e.target) != Some(&value)
            || self.values.insert((e.control, e.index), value).is_some()
        {
            return Err(invalid(span));
        }
        Ok(())
    }
    pub(super) fn finish(&self, body: &RefinedBody, span: Span) -> Result<(), LoweringError> {
        let plans = &body.owner_bridge_plans;
        if plans.group.is_empty() {
            return Ok(());
        }
        let entries = plans.scalar_entries.as_ref().ok_or_else(|| invalid(span))?;
        if self.values.len() != entries.len()
            || entries
                .iter()
                .any(|e| !actual_alias(body, e) || !self.values.contains_key(&(e.control, e.index)))
        {
            return Err(invalid(span));
        }
        Ok(())
    }
}
#[cfg(test)]
pub(super) fn record_complete(plans: &Plans) {
    if let Some(entries) = &plans.scalar_entries {
        super::super::build::owner_consumer_test_support::record_action(
            "control-chain-scalar-ssa-complete",
            entries.len(),
        );
    }
}
