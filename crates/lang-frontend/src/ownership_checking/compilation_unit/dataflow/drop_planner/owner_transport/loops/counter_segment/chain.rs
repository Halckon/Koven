//! Atomic internal three-control seal. Raw members are consumed only after validation.
use super::*;
use crate::ownership_checking::compilation_unit::owner_transport::{
    PrivateChainMember, PrivateControlChain,
};
impl Segment {
    fn completed_first_bridge(
        &self,
    ) -> Option<crate::ownership_checking::UnitOwnerControlBridgeEvidenceV1> {
        if self.invalid || !self.captured || self.continuation.is_some() {
            return None;
        }
        let control = *self.statements.last()?;
        let (condition, body) = self.bridge_body?;
        if self.bridge_condition != Some(condition)
            || condition != self.condition
            || body != self.body
        {
            return None;
        }
        let normal = self.normal.as_ref()?;
        let entry = self.entry.as_ref()?.sites()?;
        self.bridge
            .as_ref()?
            .evidence(control, condition, body)?
            .with_next_normal(Some(normal.result()?))?
            .with_next_counter_tail(Some(normal.counter_tail()?))?
            .with_next_false_exit_partition(Some(normal.exit_partition()?))?
            .with_next_false_exit_suffix(normal.suffix_result())?
            .with_next_counter_entry(Some(entry))
    }
}
impl ControlTransactions {
    pub(in super::super::super) fn take_private_chain(&mut self) -> Option<PrivateControlChain> {
        if self.root_branches_invalid || !self.root_branch_arms.is_empty() {
            return None;
        }
        let [first, last] = self.counter_segments.as_slice() else {
            return None;
        };
        let first = first.completed_first_bridge()?;
        if last.invalid
            || !last.captured
            || last.bridge.is_some()
            || last.predecessor != first.next_control()
        {
            return None;
        }
        let control = *last.statements.last()?;
        let (condition, body) = last.bridge_body?;
        if last.bridge_condition != Some(condition)
            || condition != last.condition
            || body != last.body
        {
            return None;
        }
        let (predecessor, sources, targets) = last.continuation.as_ref()?.chain_inputs(&first)?;
        let normal = last.normal.as_ref()?;
        let rule = normal.result()?;
        if rule.header_roots() != targets {
            return None;
        }
        let member = PrivateChainMember::seal(
            (control, condition, body),
            rule,
            normal.counter_tail()?,
            last.entry.as_ref()?.sites()?,
            normal.exit_partition()?,
            normal.suffix_result(),
        )?;
        let group = PrivateControlChain::seal(first, predecessor, sources, targets, member)?;
        // Nothing has been taken before the whole group succeeded. No first-member fallback.
        self.counter_segments.clear();
        Some(group)
    }
}

#[cfg(test)]
pub(super) mod tests;
