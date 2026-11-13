//! Unique normal-body selection across disjoint bridge, group and nested products.
use super::*;
impl Builder<'_, '_> {
    pub(in super::super) fn bridge_select_body(
        &self,
        matches: impl Fn(&UnitOwnerMultiRootNormalTransportV1) -> bool,
    ) -> Result<Option<Selected<'_>>, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        if let Some(nested) = &self.nested_owner_body {
            if self.control_bridge_plan.is_some() || self.control_chain_bodies.is_some() {
                return Err(fail());
            }
            return nested.select_body(&matches, self.inputs.span);
        }
        if let Some(group) = &self.control_chain_bodies {
            if self.control_bridge_plan.is_some() {
                return Err(fail());
            }
            let mut selected = group
                .members
                .iter()
                .filter(|m| matches(m.evidence.normal()));
            let member = selected.next();
            if matches(group.leading.normal()) {
                if member.is_some() {
                    return Err(fail());
                }
                let (blocks, _) = group.leading_lowered.as_ref().ok_or_else(fail)?;
                return Ok(Some(Selected {
                    control: group.leading.control(),
                    rule: group.leading.normal(),
                    stages: &group.leading_stages,
                    block: blocks[1],
                    roots: group.leading_body_roots.as_deref().ok_or_else(fail)?,
                }));
            }
            let Some(member) = member else {
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
                block: block[1],
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
    pub(in super::super) fn bridge_body_stages_mut(
        &mut self,
        control: UnitStatementId,
    ) -> Result<&mut Vec<consumption::Stage>, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        if let Some(nested) = &mut self.nested_owner_body {
            if self.control_bridge_plan.is_some() || self.control_chain_bodies.is_some() {
                return Err(fail());
            }
            return nested.stages_mut(control, self.inputs.span);
        }
        if let Some(group) = &mut self.control_chain_bodies {
            if self.control_bridge_plan.is_some() {
                return Err(fail());
            }
            if group.leading.control() == control {
                if group
                    .members
                    .iter()
                    .any(|m| m.evidence.control() == control)
                {
                    return Err(fail());
                }
                return Ok(&mut group.leading_stages);
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
}
