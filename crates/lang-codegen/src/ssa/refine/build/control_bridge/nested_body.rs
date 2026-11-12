//! One direct nested normal body. No entry, exit, outer-resume or induction permission.
use super::*;
use lang_frontend::ownership_checking::{
    UnitOwnerMultiRootNormalTransportV1, UnitOwnerNestedNormalBodyV1,
};

pub(in super::super) struct NestedBody<'a> {
    pub(super) evidence: &'a UnitOwnerNestedNormalBodyV1,
    stages: Vec<consumption::Stage>,
    pub(super) lowered: Option<([BlockRef; 4], Vec<Slot>)>,
    pub(super) resume_stage: consumption::Stage,
}
impl<'a> NestedBody<'a> {
    pub(in super::super) fn new(inputs: &RefineInputs<'a>) -> Result<Option<Self>, LoweringError> {
        let Some(item) = inputs.function_item else {
            return Ok(None);
        };
        let callable = UnitOwnerCallable::Function(UnitItemId::new(inputs.source_unit, item));
        let observations = inputs.owned.owner_observations();
        let mut selected = observations
            .iter()
            .filter(|o| o.callable() == callable)
            .filter_map(|o| o.nested_normal_body_v1());
        let Some(evidence) = selected.next() else {
            return Ok(None);
        };
        if selected.next().is_some()
            || observations
                .iter()
                .filter(|o| o.callable() == callable)
                .any(|o| {
                    o.control_chain_bodies_v1().is_some()
                        || o.control_bridge_evidence_v1().is_some()
                        || o.multi_root_boundary_v1().is_some()
                        || o.counter_batch_evidence_v1().is_some()
                        || o.counter_tail_evidence_v1().is_some()
                })
        {
            return Err(error(LoweringErrorKind::InvalidModel, inputs.span));
        }
        Ok(Some(Self {
            evidence,
            stages: vec![consumption::Stage::Ready; evidence.normal().steps().len()],
            lowered: None,
            resume_stage: consumption::Stage::Ready,
        }))
    }
    pub(super) fn select_body(
        &self,
        matches: &impl Fn(&UnitOwnerMultiRootNormalTransportV1) -> bool,
        span: Span,
    ) -> Result<Option<body_group::Selected<'_>>, LoweringError> {
        if !matches(self.evidence.normal()) {
            return Ok(None);
        }
        let (blocks, roots) = self
            .lowered
            .as_ref()
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
        Ok(Some(body_group::Selected {
            control: self.evidence.control(),
            rule: self.evidence.normal(),
            stages: &self.stages,
            block: blocks[1],
            roots,
        }))
    }
    pub(super) fn stages_mut(
        &mut self,
        control: UnitStatementId,
        span: Span,
    ) -> Result<&mut Vec<consumption::Stage>, LoweringError> {
        if self.evidence.control() != control {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(&mut self.stages)
    }
}
impl Builder<'_, '_> {
    pub(super) fn nested_owner_while(
        &mut self,
        control: UnitStatementId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        blocks: [BlockRef; 4],
    ) -> Result<(), LoweringError> {
        let Some(plan) = &mut self.nested_owner_body else {
            return Ok(());
        };
        if plan.evidence.control() != control {
            return Ok(());
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::NestedBodyWrongParent,
        ) {
            // Corrupt the actual enclosing lowering frame, not a local expected value.
            if let Some(parent) = self.loops.last_mut() {
                parent.statement = control;
                owner_consumer_test_support::record_action("nested-parent-fault-hit", 1);
            }
        }
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        if plan.lowered.is_some()
            || plan.evidence.condition() != condition
            || plan.evidence.body() != body
            || self.loops.len() != 1
            || self.loops.last().map(|f| f.statement) != Some(plan.evidence.parent())
            || !self.pending_call_loans.is_empty()
            || !self.result_source_loans.is_empty()
            || !self.active_views.is_empty()
            || !self.range_views.is_empty()
            || !self.short_ranges.is_empty()
            || self.control_bridge_plan.is_some()
            || self.control_chain_bodies.is_some()
        {
            return Err(fail());
        }
        let roots = plan
            .evidence
            .normal()
            .header_roots()
            .iter()
            .map(|s| {
                self.bindings
                    .get(s)
                    .copied()
                    .filter(|_| !self.dead.contains(s))
                    .ok_or_else(fail)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if roots.is_empty()
            || roots
                .iter()
                .enumerate()
                .any(|(i, r)| roots[..i].contains(r))
        {
            return Err(fail());
        }
        plan.lowered = Some((blocks, roots));
        Ok(())
    }
    pub(super) fn finish_nested_owner_body(&mut self) -> Result<(), LoweringError> {
        let Some(plan) = &self.nested_owner_body else {
            return Ok(());
        };
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let (blocks, roots) = plan.lowered.as_ref().ok_or_else(fail)?;
        #[cfg(test)]
        {
            use owner_consumer_test_support::{OwnerFault, record_action, take_fault};
            let statements = &mut self.body.blocks[blocks[1].0].statements;
            if take_fault(OwnerFault::NestedBodyLateRootPlace)
                && let Some(extra) = statements
                    .iter()
                    .find(|s| matches!(s.op, ROp::RootPlace { .. }))
                    .cloned()
            {
                statements.push(extra);
                record_action("nested-preparation-fault-hit", 1);
            }
            if take_fault(OwnerFault::NestedBodyLateMissingDrop)
                && let Some(index) = statements
                    .iter()
                    .position(|s| matches!(s.op, ROp::Drop { .. }))
            {
                statements.remove(index);
                record_action("nested-retirement-fault-hit", 1);
            }
            if take_fault(OwnerFault::NestedBodyLateDuplicateDrop)
                && let Some(extra) = statements
                    .iter()
                    .find(|s| matches!(s.op, ROp::Drop { .. }))
                    .cloned()
            {
                statements.push(extra);
                record_action("nested-retirement-fault-hit", 1);
            }
            if take_fault(OwnerFault::NestedBodyForeignLoan) {
                let other = statements
                    .iter()
                    .filter_map(|s| match s.op {
                        ROp::RootReplace { loan, .. } => Some(loan),
                        _ => None,
                    })
                    .nth(1);
                if let Some(other) = other
                    && let Some(first) = statements
                        .iter_mut()
                        .find(|s| matches!(s.op, ROp::RootReplace { .. }))
                    && let ROp::RootReplace { loan, .. } = &mut first.op
                {
                    *loan = other;
                    record_action("nested-preparation-fault-hit", 1);
                }
            }
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::NestedBodyEscapedPreparation,
        ) && let Some(extra) = self.body.blocks[blocks[1].0]
            .statements
            .iter()
            .find(|s| matches!(s.op, ROp::BorrowBegin { .. }))
            .cloned()
        {
            self.body.blocks[blocks[2].0].statements.push(extra);
            owner_consumer_test_support::record_action("nested-preparation-fault-hit", 1);
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::NestedBodyEscapedLoanTerminator,
        ) && let Some(loan) = self.body.blocks[blocks[1].0]
            .statements
            .iter()
            .find_map(|s| {
                if let ROp::RootReplace { loan, .. } = s.op {
                    Some(loan)
                } else {
                    None
                }
            })
        {
            self.body.blocks[blocks[2].0].terminator =
                Some((RTerm::BorrowReturn { loan }, self.inputs.span));
            owner_consumer_test_support::record_action("nested-preparation-fault-hit", 1);
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::NestedFalseMissingDrop,
        ) && let Some(index) = self.body.blocks[blocks[2].0]
            .statements
            .iter()
            .position(|s| matches!(s.op, ROp::Drop { .. }))
        {
            self.body.blocks[blocks[2].0].statements.remove(index);
            owner_consumer_test_support::record_action("nested-false-fault-hit", 1);
        }
        let statements = &self.body.blocks[blocks[1].0].statements;
        if plan.stages.is_empty()
            || plan
                .stages
                .iter()
                .any(|s| *s != consumption::Stage::Dropped)
            || statements
                .iter()
                .filter(|s| matches!(s.op, ROp::RootReplace { .. }))
                .count()
                != plan.stages.len()
            || !matches!(self.body.blocks[blocks[0].0].terminator, Some((RTerm::Branch { when_true, when_false, .. },_)) if when_true == blocks[1] && when_false == blocks[2])
        {
            return Err(fail());
        }
        // Admit only each Replace's actual exclusive RootPlace/BorrowBegin preparation.
        // No extra root read, owner drop or loan use is authorized by this product.
        for root in roots {
            let replaces = statements
                .iter()
                .enumerate()
                .filter_map(|(i, s)| match s.op {
                    ROp::RootReplace { owner, loan, .. } if owner == *root => Some((i, loan)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [(replace, loan)] = replaces.as_slice() else {
                return Err(fail());
            };
            let loans = statements[..*replace]
                .iter()
                .enumerate()
                .filter_map(|(i, s)| match s.op {
                    ROp::BorrowBegin {
                        place,
                        kind: crate::ssa::model::LoanKind::Exclusive,
                    } if s.results == [*loan] => Some((i, place)),
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
                    (s.results == [*place]
                        && matches!(s.op, ROp::RootPlace { owner } if owner == *root))
                    .then_some(i)
                })
                .collect::<Vec<_>>();
            let [prepare] = places.as_slice() else {
                return Err(fail());
            };
            for (i, statement) in statements.iter().enumerate() {
                if (statement.op.operands().contains(root) && i != *prepare && i != *replace)
                    || (statement.results.contains(root) && i != *replace)
                    || (statement.op.operands().contains(place) && i != *borrow)
                    || (statement.results.contains(place) && i != *prepare)
                    || (statement.op.operands().contains(loan) && i != *replace)
                    || (statement.results.contains(loan) && i != *borrow)
                {
                    return Err(fail());
                }
            }
            // Preparation capabilities belong only to this normal body. Root owners
            // may legitimately cross its boundary; ephemeral places and loans may not.
            for (block_index, block) in self.body.blocks.iter().enumerate() {
                for (index, statement) in block.statements.iter().enumerate() {
                    let at = |position| block_index == blocks[1].0 && index == position;
                    if statement.results.contains(place) != at(*prepare)
                        || statement.op.operands().contains(place) != at(*borrow)
                        || statement.results.contains(loan) != at(*borrow)
                        || statement.op.operands().contains(loan) != at(*replace)
                    {
                        return Err(fail());
                    }
                }
                if block
                    .terminator
                    .as_ref()
                    .is_some_and(|(t, _)| t.operands().iter().any(|s| s == place || s == loan))
                {
                    return Err(fail());
                }
            }
        }
        self.check_nested_retirement(plan.evidence.normal(), blocks[1], roots)?;
        if let Some((_live, drops)) = plan.evidence.false_exit_partition() {
            let mut previous = None;
            let mut sites = Vec::new();
            for fact in drops {
                let UnitDropTarget::Named(root) = fact.target() else {
                    return Err(fail());
                };
                if fact.point() != UnitDropPoint::LoopExit(plan.evidence.control()) {
                    return Err(fail());
                }
                let index = plan
                    .evidence
                    .normal()
                    .header_roots()
                    .iter()
                    .position(|r| *r == root)
                    .ok_or_else(fail)?;
                let actual =
                    self.multi_root_drop(*fact, blocks[2], *roots.get(index).ok_or_else(fail)?)?;
                if previous.is_some_and(|p| p >= actual) {
                    return Err(fail());
                }
                previous = Some(actual);
                sites.push(super::super::super::nested_exit_receipt::DropSite {
                    root: roots[index],
                    instruction: actual,
                    span: fact.value_origin(),
                });
            }
            self.body.nested_exit_plans = super::super::super::nested_exit_receipt::Plans(Some(
                super::super::super::nested_exit_receipt::Plan {
                    control: plan.evidence.control(),
                    header: blocks[0],
                    exit: blocks[2],
                    drops: sites,
                    roots: roots.clone(),
                },
            ));
            #[cfg(test)]
            owner_consumer_test_support::record_action(
                "nested-false-cleanup-rechecked",
                drops.len(),
            );
        }

        #[cfg(test)]
        owner_consumer_test_support::record_action(
            "nested-owner-normal-body-complete",
            plan.stages.len(),
        );
        Ok(())
    }
}

impl Builder<'_, '_> {
    // Re-read completed emission, not just the mutable one-shot stage flags.
    fn check_nested_retirement(
        &self,
        rule: &UnitOwnerMultiRootNormalTransportV1,
        body: BlockRef,
        roots: &[Slot],
    ) -> Result<(), LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let statements = &self.body.blocks[body.0].statements;
        let mut previous = None;
        for step in rule.steps() {
            let root_index = rule
                .header_roots()
                .iter()
                .position(|r| *r == step.header_root())
                .ok_or_else(fail)?;
            let root = *roots.get(root_index).ok_or_else(fail)?;
            let producer = self.multi_root_literal(
                rule.body_catalog()
                    .definition(step.producer())
                    .ok_or_else(fail)?
                    .origin(),
                body,
            )?;
            let matching = statements.iter().enumerate().filter(|(_, s)| s.span == self.span(step.call().expression())
                && matches!(s.op, ROp::RootReplace {owner,replacement,..} if owner==root && replacement==producer)).collect::<Vec<_>>();
            let [(replace_index, replace)] = matching.as_slice() else {
                return Err(fail());
            };
            let [actual_root, old] = replace.results.as_slice() else {
                return Err(fail());
            };
            let fact = step.returned_value_drop();
            let UnitDropTarget::Named(symbol) = fact.target() else {
                return Err(fail());
            };
            let bound = *self.bindings.get(&symbol).ok_or_else(fail)?;
            let alias = self.counter_alias(body, bound, *old, None)?;
            let drop = self.multi_root_drop(fact, body, bound)?;
            if *actual_root != root
                || roots.contains(old)
                || roots.contains(&bound)
                || *old == bound
                || previous.is_some_and(|p| p >= *replace_index)
                || *replace_index >= alias
                || alias >= drop
            {
                return Err(fail());
            }
            for (block_index, block) in self.body.blocks.iter().enumerate() {
                for (index, statement) in block.statements.iter().enumerate() {
                    let at = |position| block_index == body.0 && index == position;
                    if statement.results.contains(old) != at(*replace_index)
                        || statement.results.contains(&bound) != at(alias)
                        || statement.op.operands().contains(old) != at(alias)
                        || statement.op.operands().contains(&bound) != at(drop)
                    {
                        return Err(fail());
                    }
                }
                if block
                    .terminator
                    .as_ref()
                    .is_some_and(|(t, _)| t.operands().iter().any(|s| *s == *old || *s == bound))
                {
                    return Err(fail());
                }
            }
            previous = Some(drop);
        }
        #[cfg(test)]
        owner_consumer_test_support::record_action(
            "nested-owner-retirement-rechecked",
            rule.steps().len(),
        );
        Ok(())
    }
}
