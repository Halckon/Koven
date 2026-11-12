//! One direct nested normal body. No entry, exit, outer-resume or induction permission.
use super::*;
use lang_frontend::ownership_checking::{
    UnitOwnerMultiRootNormalTransportV1, UnitOwnerNestedNormalBodyV1,
};

pub(in super::super) struct NestedBody<'a> {
    evidence: &'a UnitOwnerNestedNormalBodyV1,
    stages: Vec<consumption::Stage>,
    lowered: Option<([BlockRef; 4], Vec<Slot>)>,
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
        }
        #[cfg(test)]
        owner_consumer_test_support::record_action(
            "nested-owner-normal-body-complete",
            plan.stages.len(),
        );
        Ok(())
    }
}
