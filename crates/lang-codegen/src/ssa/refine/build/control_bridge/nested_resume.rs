//! Independent one-shot parent resume; never reuses the child's consumed permit.
use super::*;
use consumption::{Permit, Stage};
impl Builder<'_, '_> {
    pub(super) fn resume_replace_authorization(
        &self,
        call: UnitExpressionId,
        root: Slot,
        replacement: Slot,
        span: Span,
    ) -> Result<Option<Permit>, LoweringError> {
        let Some(plan) = &self.nested_owner_body else {
            return Ok(None);
        };
        let Some(write) = plan
            .evidence
            .parent_resume_write()
            .filter(|w| w.call() == call)
        else {
            return Ok(None);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let (blocks, roots) = plan.lowered.as_ref().ok_or_else(fail)?;
        let index = plan
            .evidence
            .normal()
            .header_roots()
            .iter()
            .position(|r| *r == write.root())
            .ok_or_else(fail)?;
        if roots.get(index) != Some(&root)
            || self.bindings.get(&write.root()) != Some(&root)
            || self.multi_root_literal(
                lang_frontend::ownership_checking::UnitOwnerOrigin::String(write.producer()),
                blocks[3],
            )? != replacement
        {
            return Err(fail());
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::NestedResumeMissingReplace,
        ) {
            owner_consumer_test_support::record_action("nested-resume-fault-hit", 1);
            return Ok(None);
        }
        self.resume_permit(Stage::Ready, root, Some(replacement), span)
            .map(Some)
    }
    pub(super) fn resume_binding_authorization(
        &self,
        call: UnitExpressionId,
        binding: UnitSymbolId,
        value: Slot,
        span: Span,
    ) -> Result<Option<Permit>, LoweringError> {
        let Some(plan) = &self.nested_owner_body else {
            return Ok(None);
        };
        let Some(w) = plan
            .evidence
            .parent_resume_write()
            .filter(|w| w.call() == call || w.binding() == binding)
        else {
            return Ok(None);
        };
        if w.call() != call || w.binding() != binding {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::NestedResumeMissingBinding,
        ) {
            owner_consumer_test_support::record_action("nested-resume-fault-hit", 1);
            return Ok(None);
        }
        self.resume_permit(Stage::Replaced(value), value, None, span)
            .map(Some)
    }
    pub(super) fn resume_drop_authorization(
        &self,
        index: usize,
        target: UnitDropTarget,
        value: Slot,
        span: Span,
    ) -> Result<Option<Permit>, LoweringError> {
        let Some(plan) = &self.nested_owner_body else {
            return Ok(None);
        };
        let Some(w) = plan
            .evidence
            .parent_resume_write()
            .filter(|w| w.returned_value_drop().target() == target)
        else {
            return Ok(None);
        };
        if self.inputs.owned.drops().get(index) != Some(&w.returned_value_drop()) {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::NestedResumeMissingDrop,
        ) {
            owner_consumer_test_support::record_action("nested-resume-fault-hit", 1);
            return Ok(None);
        }
        self.resume_permit(Stage::Bound(value), value, None, span)
            .map(Some)
    }
    fn resume_permit(
        &self,
        stage: Stage,
        source: Slot,
        replacement: Option<Slot>,
        span: Span,
    ) -> Result<Permit, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let plan = self.nested_owner_body.as_ref().ok_or_else(fail)?;
        let (blocks, _) = plan.lowered.as_ref().ok_or_else(fail)?;
        if !matches!(self.body.blocks[blocks[2].0].terminator,Some((RTerm::Goto(next),_)) if next==blocks[3])
        {
            return Err(fail());
        }
        if plan.evidence.false_exit_partition().is_none()
            || plan.evidence.parent_resume_roots().is_none()
            || plan.resume_stage != stage
            || self.block != Some(blocks[3])
            || self.loops.len() != 1
            || self.loops.last().map(|f| f.statement) != Some(plan.evidence.parent())
            || !self.pending_call_loans.is_empty()
            || !self.result_source_loans.is_empty()
            || !self.active_views.is_empty()
            || !self.range_views.is_empty()
            || !self.short_ranges.is_empty()
        {
            return Err(fail());
        }
        Ok(Permit {
            index: 0,
            control: plan.evidence.parent(),
            block: blocks[3],
            instruction: self.body.blocks[blocks[3].0].statements.len(),
            span,
            stage,
            source,
            replacement,
            resume: true,
        })
    }
    pub(super) fn resume_emitted(
        &mut self,
        permit: Permit,
        output: Option<Slot>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        #[cfg(test)]
        if permit.stage == Stage::Ready
            && owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::NestedResumeForeignLoan,
            )
        {
            let child = self
                .nested_owner_body
                .as_ref()
                .and_then(|p| p.lowered.as_ref())
                .map(|(b, _)| b[1])
                .ok_or_else(fail)?;
            let loan = self.body.blocks[child.0]
                .statements
                .iter()
                .find_map(|s| {
                    if let ROp::RootReplace { loan, .. } = s.op {
                        Some(loan)
                    } else {
                        None
                    }
                })
                .ok_or_else(fail)?;
            if let ROp::RootReplace { loan: actual, .. } =
                &mut self.body.blocks[permit.block.0].statements[permit.instruction].op
            {
                *actual = loan;
                owner_consumer_test_support::record_action("nested-resume-fault-hit", 1);
            }
        }
        let plan = self.nested_owner_body.as_mut().ok_or_else(fail)?;
        let (blocks, _) = plan.lowered.as_ref().ok_or_else(fail)?;
        let block = self.body.blocks.get(permit.block.0).ok_or_else(fail)?;
        let s = block.statements.get(permit.instruction).ok_or_else(fail)?;
        if permit.control != plan.evidence.parent()
            || permit.block != blocks[3]
            || self.block != Some(permit.block)
            || block.statements.len() != permit.instruction + 1
            || s.span != permit.span
            || plan.resume_stage != permit.stage
        {
            return Err(fail());
        }
        let next = match (permit.stage, output) {
            (Stage::Ready, Some(old))
                if old != permit.source
                    && s.results == [permit.source, old]
                    && matches!(s.op,ROp::RootReplace {owner,replacement,..} if owner==permit.source && Some(replacement)==permit.replacement) =>
            {
                Stage::Replaced(old)
            }
            (Stage::Replaced(source), Some(bound))
                if source == permit.source
                    && bound != source
                    && s.results == [bound]
                    && matches!(s.op,ROp::Alias {source:s} if s==source) =>
            {
                Stage::Bound(bound)
            }
            (Stage::Bound(source), None)
                if source == permit.source
                    && s.results.is_empty()
                    && matches!(s.op,ROp::Drop {owner} if owner==source) =>
            {
                Stage::Dropped
            }
            _ => return Err(fail()),
        };
        plan.resume_stage = next;
        #[cfg(test)]
        owner_consumer_test_support::record_action(
            "nested-resume-stage",
            match next {
                Stage::Ready => 0,
                Stage::Replaced(_) => 1,
                Stage::Bound(_) => 2,
                Stage::Dropped => 3,
            },
        );
        Ok(())
    }
    pub(super) fn finish_nested_resume(&mut self) -> Result<(), LoweringError> {
        let Some(plan) = &self.nested_owner_body else {
            return Ok(());
        };
        if plan.evidence.parent_resume_write().is_some() {
            if plan.resume_stage != Stage::Dropped {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            let mut receipt = self.recheck_nested_resume()?;
            receipt.carry = self.nested_resume_carry(&receipt);
            self.body.nested_resume_plans =
                super::super::super::nested_resume_receipt::Plans(Some(receipt));
            #[cfg(test)]
            owner_consumer_test_support::record_action("nested-parent-resume-consumed", 1);
        }
        Ok(())
    }
    fn recheck_nested_resume(
        &self,
    ) -> Result<super::super::super::nested_resume_receipt::Plan, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let plan = self.nested_owner_body.as_ref().ok_or_else(fail)?;
        let w = plan.evidence.parent_resume_write().ok_or_else(fail)?;
        let (blocks, roots) = plan.lowered.as_ref().ok_or_else(fail)?;
        let body = blocks[3];
        let statements = &self.body.blocks[body.0].statements;
        let index = plan
            .evidence
            .normal()
            .header_roots()
            .iter()
            .position(|r| *r == w.root())
            .ok_or_else(fail)?;
        let root = *roots.get(index).ok_or_else(fail)?;
        if !matches!(self.body.blocks[blocks[2].0].terminator,Some((RTerm::Goto(next),_)) if next==body)
            || self
                .body
                .blocks
                .iter()
                .filter(|b| {
                    b.terminator
                        .as_ref()
                        .is_some_and(|(t, _)| t.successors().contains(&body))
                })
                .count()
                != 1
            || self.body.blocks[blocks[2].0]
                .statements
                .iter()
                .any(|s| s.results.contains(&root) || s.op.operands().contains(&root))
        {
            return Err(fail());
        }
        let replacement = self.multi_root_literal(
            lang_frontend::ownership_checking::UnitOwnerOrigin::String(w.producer()),
            body,
        )?;
        let matches=statements.iter().enumerate().filter(|(_,s)|s.span==self.span(w.call().expression()) && matches!(s.op,ROp::RootReplace {owner,replacement:r,..} if owner==root && r==replacement)).collect::<Vec<_>>();
        let [(replace, statement)] = matches.as_slice() else {
            return Err(fail());
        };
        let [new, old] = statement.results.as_slice() else {
            return Err(fail());
        };
        self.recheck_resume_preparation(body, root, *replace)?;

        let bound = *self.bindings.get(&w.binding()).ok_or_else(fail)?;
        let alias = self.counter_alias(body, bound, *old, None)?;
        let drop = self.multi_root_drop(w.returned_value_drop(), body, bound)?;
        if *new != root
            || roots.contains(old)
            || roots.contains(&bound)
            || *old == bound
            || *replace >= alias
            || alias >= drop
        {
            return Err(fail());
        }
        for (b, block) in self.body.blocks.iter().enumerate() {
            for (i, s) in block.statements.iter().enumerate() {
                let at = |n| b == body.0 && i == n;
                if s.results.contains(old) != at(*replace)
                    || s.results.contains(&bound) != at(alias)
                    || s.op.operands().contains(old) != at(alias)
                    || s.op.operands().contains(&bound) != at(drop)
                {
                    return Err(fail());
                }
            }
            if block
                .terminator
                .as_ref()
                .is_some_and(|(t, _)| t.operands().iter().any(|s| s == old || *s == bound))
            {
                return Err(fail());
            }
        }
        Ok(super::super::super::nested_resume_receipt::Plan {
            carry: None,
            parent: plan.evidence.parent(),
            child: plan.evidence.control(),
            header: blocks[0],
            finished: blocks[2],
            resume: body,
            root,
            replacement,
            old: *old,
            bound,
            positions: [*replace, alias, drop],
            spans: [
                statement.span,
                statements[alias].span,
                statements[drop].span,
            ],
        })
    }
    fn recheck_resume_preparation(
        &self,
        body: BlockRef,
        root: Slot,
        replace: usize,
    ) -> Result<(), LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let statements = &self.body.blocks[body.0].statements;
        let ROp::RootReplace { loan, .. } = statements.get(replace).ok_or_else(fail)?.op else {
            return Err(fail());
        };
        let loans = statements[..replace]
            .iter()
            .enumerate()
            .filter_map(|(i, s)| match s.op {
                ROp::BorrowBegin {
                    place,
                    kind: crate::ssa::model::LoanKind::Exclusive,
                } if s.results == [loan] => Some((i, place)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(borrow, place)] = loans.as_slice() else {
            return Err(fail());
        };
        let places = statements[..*borrow]
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                (s.results == [*place] && matches!(s.op,ROp::RootPlace {owner} if owner==root))
                    .then_some(i)
            })
            .collect::<Vec<_>>();
        let [prepare] = places.as_slice() else {
            return Err(fail());
        };
        if statements[..replace].iter().enumerate().any(|(i, s)| {
            s.results.contains(&root) || (s.op.operands().contains(&root) && i != *prepare)
        }) {
            return Err(fail());
        }

        for (b, block) in self.body.blocks.iter().enumerate() {
            for (i, s) in block.statements.iter().enumerate() {
                let at = |n| b == body.0 && i == n;
                if s.results.contains(place) != at(*prepare)
                    || s.op.operands().contains(place) != at(*borrow)
                    || s.results.contains(&loan) != at(*borrow)
                    || s.op.operands().contains(&loan) != at(replace)
                {
                    return Err(fail());
                }
            }
            if block
                .terminator
                .as_ref()
                .is_some_and(|(t, _)| t.operands().iter().any(|s| s == place || *s == loan))
            {
                return Err(fail());
            }
        }
        Ok(())
    }
}
