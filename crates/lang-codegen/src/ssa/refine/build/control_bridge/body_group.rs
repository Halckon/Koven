//! One group of normal-body consumers, keyed by actual control, never entry tokens.
use super::*;
use lang_frontend::ownership_checking::{
    UnitOwnerControlChainBodyV1, UnitOwnerMultiRootNormalTransportV1,
};

pub(in super::super) struct BodyGroup<'a> {
    members: Vec<Member<'a>>,
}
struct Member<'a> {
    evidence: UnitOwnerControlChainBodyV1<'a>,
    stages: Vec<consumption::Stage>,
    lowered: Option<(BlockRef, Vec<Slot>)>,
}
pub(super) struct Selected<'a> {
    pub control: UnitStatementId,
    pub rule: &'a UnitOwnerMultiRootNormalTransportV1,
    pub stages: &'a [consumption::Stage],
    pub block: BlockRef,
    pub roots: &'a [Slot],
}
impl<'a> BodyGroup<'a> {
    #[cfg(test)]
    pub(super) fn other_control(&self, control: UnitStatementId) -> Option<UnitStatementId> {
        self.members
            .iter()
            .map(|m| m.evidence.control())
            .find(|c| *c != control)
    }
    #[cfg(test)]
    pub(super) fn is_last(&self, control: UnitStatementId) -> bool {
        self.members
            .last()
            .is_some_and(|m| m.evidence.control() == control)
    }
    pub(in super::super) fn new(inputs: &RefineInputs<'a>) -> Result<Option<Self>, LoweringError> {
        let Some(item) = inputs.function_item else {
            return Ok(None);
        };
        let callable = UnitOwnerCallable::Function(UnitItemId::new(inputs.source_unit, item));
        let mut groups = inputs
            .owned
            .owner_observations()
            .iter()
            .filter(|o| o.callable() == callable)
            .filter_map(|o| o.control_chain_bodies_v1());
        let Some(group) = groups.next() else {
            return Ok(None);
        };
        if groups.next().is_some()
            || inputs
                .owned
                .owner_observations()
                .iter()
                .filter(|o| o.callable() == callable)
                .any(|o| o.control_bridge_evidence_v1().is_some())
        {
            return Err(error(LoweringErrorKind::InvalidModel, inputs.span));
        }
        Ok(Some(Self {
            members: group
                .members()
                .iter()
                .map(|e| Member {
                    evidence: *e,
                    stages: vec![consumption::Stage::Ready; e.normal().steps().len()],
                    lowered: None,
                })
                .collect(),
        }))
    }
}
impl Builder<'_, '_> {
    pub(super) fn bridge_select_body(
        &self,
        matches: impl Fn(&UnitOwnerMultiRootNormalTransportV1) -> bool,
    ) -> Result<Option<Selected<'_>>, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        if let Some(group) = &self.control_chain_bodies {
            if self.control_bridge_plan.is_some() {
                return Err(fail());
            }
            let mut selected = group
                .members
                .iter()
                .filter(|m| matches(m.evidence.normal()));
            let Some(member) = selected.next() else {
                return Ok(None);
            };
            if selected.next().is_some() {
                return Err(fail());
            }
            let (block, roots) = member.lowered.as_ref().ok_or_else(fail)?;
            return Ok(Some(Selected {
                control: member.evidence.control(),
                rule: member.evidence.normal(),
                stages: &member.stages,
                block: *block,
                roots,
            }));
        }
        let Some(plan) = &self.control_bridge_plan else {
            return Ok(None);
        };
        let Some(rule) = plan.evidence.next_normal_transport().filter(|r| matches(r)) else {
            return Ok(None);
        };
        let (_, [_, block, _, _], roots) = plan.next.as_ref().ok_or_else(fail)?;
        Ok(Some(Selected {
            control: plan.evidence.next_control(),
            rule,
            stages: &plan.chain,
            block: *block,
            roots,
        }))
    }
    pub(super) fn bridge_body_stages_mut(
        &mut self,
        control: UnitStatementId,
    ) -> Result<&mut Vec<consumption::Stage>, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        if let Some(group) = &mut self.control_chain_bodies {
            if self.control_bridge_plan.is_some() {
                return Err(fail());
            }
            return group
                .members
                .iter_mut()
                .find(|m| m.evidence.control() == control)
                .map(|m| &mut m.stages)
                .ok_or_else(fail);
        }
        self.control_bridge_plan
            .as_mut()
            .filter(|p| p.evidence.next_control() == control)
            .map(|p| &mut p.chain)
            .ok_or_else(fail)
    }
    pub(super) fn control_chain_body_while(
        &mut self,
        control: UnitStatementId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        blocks: [BlockRef; 4],
    ) -> Result<(), LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let Some(group) = &mut self.control_chain_bodies else {
            return Ok(());
        };
        let Some(index) = group
            .members
            .iter()
            .position(|m| m.evidence.control() == control)
        else {
            return Ok(());
        };
        if group.members[..index].iter().any(|m| {
            m.lowered.is_none() || m.stages.iter().any(|s| *s != consumption::Stage::Dropped)
        }) {
            return Err(fail());
        }
        let member = &mut group.members[index];
        if member.lowered.is_some()
            || member.evidence.condition() != condition
            || member.evidence.body() != body
        {
            return Err(fail());
        }
        let roots = member
            .evidence
            .roots()
            .iter()
            .map(|s| self.bindings.get(s).copied().ok_or_else(fail))
            .collect::<Result<Vec<_>, _>>()?;
        if roots
            .iter()
            .enumerate()
            .any(|(i, r)| roots[..i].contains(r))
        {
            return Err(fail());
        }
        member.lowered = Some((blocks[1], roots));
        Ok(())
    }
    pub(super) fn finish_control_chain_bodies(&self) -> Result<(), LoweringError> {
        let Some(group) = &self.control_chain_bodies else {
            return Ok(());
        };
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        for member in &group.members {
            let (block, _) = member.lowered.as_ref().ok_or_else(fail)?;
            if member.stages.len() != member.evidence.normal().steps().len()
                || member
                    .stages
                    .iter()
                    .any(|s| *s != consumption::Stage::Dropped)
                || self.body.blocks[block.0]
                    .statements
                    .iter()
                    .filter(|s| matches!(s.op, ROp::RootReplace { .. }))
                    .count()
                    != member.stages.len()
            {
                return Err(fail());
            }
        }
        #[cfg(test)]
        owner_consumer_test_support::record_action(
            "control-chain-bodies-complete",
            group.members.len(),
        );
        Ok(())
    }
}
