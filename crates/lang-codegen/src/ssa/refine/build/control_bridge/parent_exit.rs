//! Observe the approved parent boundary; no new cleanup or consumption permission.
use super::*;
impl Builder<'_, '_> {
    pub(super) fn parent_owner_while(
        &mut self,
        control: UnitStatementId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        blocks: [BlockRef; 4],
    ) -> Result<(), LoweringError> {
        let Some(plan) = &mut self.nested_owner_body else {
            return Ok(());
        };
        if control == plan.evidence.parent() {
            if plan.parent_initial.is_some() {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            plan.parent_initial = self.block;
        }
        let Some(parent) = plan.evidence.parent_boundary_v1() else {
            return Ok(());
        };
        if control != parent.control() {
            return Ok(());
        }
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        if !parent.is_consistent()
            || plan.parent_lowered.is_some()
            || !self.loops.is_empty()
            || condition != parent.condition()
            || body != parent.body()
        {
            return Err(fail());
        }
        let roots = parent
            .roots()
            .iter()
            .map(|r| {
                self.bindings
                    .get(r)
                    .copied()
                    .filter(|_| !self.dead.contains(r))
                    .ok_or_else(fail)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if roots
            .iter()
            .enumerate()
            .any(|(i, r)| roots[..i].contains(r))
        {
            return Err(fail());
        }
        plan.parent_lowered = Some((blocks, roots));
        Ok(())
    }
    pub(super) fn finish_parent_owner_exit(&mut self) -> Result<(), LoweringError> {
        let Some(plan) = &self.nested_owner_body else {
            return Ok(());
        };
        let Some(parent) = plan.evidence.parent_boundary_v1() else {
            return Ok(());
        };
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let (blocks, roots) = plan.parent_lowered.as_ref().ok_or_else(fail)?;
        if parent.control() != plan.evidence.parent()
            || plan.parent_header != Some(blocks[0])
            || !matches!(self.body.blocks[blocks[0].0].terminator, Some((RTerm::Branch {when_true, when_false, ..}, _)) if when_true == blocks[1] && when_false == blocks[2])
        {
            return Err(fail());
        }
        let slot = |r: UnitSymbolId| {
            parent
                .roots()
                .iter()
                .position(|s| *s == r)
                .and_then(|i| roots.get(i).copied())
                .ok_or_else(fail)
        };
        let (live, drops) = parent.false_exit_partition();
        let mut sites = Vec::new();
        let mut previous = None;
        for fact in drops {
            let UnitDropTarget::Named(root) = fact.target() else {
                return Err(fail());
            };
            if fact.point() != UnitDropPoint::LoopExit(parent.control()) {
                return Err(fail());
            }
            let root = slot(root)?;
            let instruction = self.multi_root_drop(*fact, blocks[2], root)?;
            if previous.is_some_and(|p| p >= instruction) {
                return Err(fail());
            }
            previous = Some(instruction);
            sites.push(super::super::super::nested_exit_receipt::DropSite {
                root,
                instruction,
                span: fact.value_origin(),
            });
        }
        self.body.nested_exit_plans.parent_required = true;
        self.body.nested_exit_plans.parent = Some(super::super::super::nested_exit_receipt::Plan {
            control: parent.control(),
            header: blocks[0],
            exit: blocks[2],
            roots: roots.clone(),
            live: live.iter().map(|r| slot(*r)).collect::<Result<_, _>>()?,
            drops: sites,
            parent: true,
        });
        #[cfg(test)]
        {
            use owner_consumer_test_support::{OwnerFault, record_action, take_fault};
            if take_fault(OwnerFault::ParentFalseMissingPlan) {
                self.body.nested_exit_plans.parent = None;
                record_action("parent-false-plan-fault-hit", 1);
            }
            if take_fault(OwnerFault::ParentFalseIncompletePartition) {
                self.body
                    .nested_exit_plans
                    .parent
                    .as_mut()
                    .unwrap()
                    .live
                    .clear();
                record_action("parent-false-plan-fault-hit", 1);
            }
        }
        Ok(())
    }
}
