//! Complete static entry/false-exit consumption; never turns an audit into permission.
use super::*;
use lang_frontend::ownership_checking::UnitOwnerOrigin;
/// Private single-use proof that all entry/exit checks in this module completed.
/// It cannot authorize lowering or be constructed by the scalar/body audit.
pub(super) struct Complete<'a> {
    evidence: &'a UnitOwnerCounterTailEvidenceV1,
    geometry: [BlockRef; 5],
    roots: Vec<Slot>,
}
impl Complete<'_> {
    pub(super) fn validate(&self, plan: &Plan<'_>, span: Span) -> Result<(), LoweringError> {
        if !std::ptr::eq(self.evidence, plan.evidence)
            || Some(self.geometry) != plan.geometry
            || self.roots != plan.roots
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(())
    }
    pub(super) fn commit(self, plan: &Plan<'_>, span: Span) -> Result<(), LoweringError> {
        self.validate(plan, span)?;
        #[cfg(test)]
        owner_consumer_test_support::record_action(
            "counter-tail-complete-boundary-audit",
            self.roots.len(),
        );
        Ok(())
    }
}
impl Builder<'_, '_> {
    pub(super) fn counter_tail_boundary<'a>(
        &self,
        plan: &Plan<'a>,
    ) -> Result<Option<Complete<'a>>, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let evidence = plan.evidence;
        // Partial evidence remains usable by the scalar/body audit, but cannot
        // produce the complete-boundary receipt or borrow old exit permissions.
        let Some((live, drops)) = evidence.false_exit_partition() else {
            return Ok(None);
        };
        let suffix = evidence.false_exit_suffix().unwrap_or(&[]);
        if !live.is_empty() && suffix.len() != live.len() {
            return Ok(None);
        }
        #[cfg(test)]
        let suffix = if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::CounterBoundaryMissingSuffix,
        ) {
            &[]
        } else {
            suffix
        };
        // Only literal and direct function-entry owned parameter origins are covered.
        if evidence.root_slots().iter().any(|slot| {
            !matches!(
                evidence
                    .input_catalog()
                    .definition(slot.entry())
                    .map(|d| d.origin()),
                Some(UnitOwnerOrigin::String(_) | UnitOwnerOrigin::Parameter(_))
            )
        }) {
            return Ok(None);
        }
        let [entry, _, _, finished, exit] = plan.geometry.ok_or_else(fail)?;
        if entry != BlockRef(0)
            && evidence.root_slots().iter().any(|slot| {
                matches!(
                    evidence
                        .input_catalog()
                        .definition(slot.entry())
                        .map(|d| d.origin()),
                    Some(UnitOwnerOrigin::Parameter(_))
                )
            })
        {
            return Ok(None);
        }
        if !matches!(self.body.blocks[finished.0].terminator, Some((RTerm::Goto(t), _)) if t == exit)
        {
            return Err(fail());
        }
        let predecessors: Vec<_> = self
            .body
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| {
                b.terminator
                    .as_ref()
                    .filter(|(t, _)| t.successors().contains(&exit))
                    .map(|_| BlockRef(i))
            })
            .collect();
        if predecessors != [finished] {
            return Err(fail());
        }
        let mut suffix_order = Vec::new();
        let mut drop_order = Vec::new();
        for slot in evidence.root_slots() {
            // Steps are body order while the manifest is entry-root order.
            let index = evidence
                .steps()
                .iter()
                .position(|s| s.header_root() == slot.root())
                .ok_or_else(fail)?;
            let root = plan.roots[index];
            #[cfg(test)]
            let root = if owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::CounterBoundaryWrongEntry,
            ) {
                plan.guard.ok_or_else(fail)?
            } else {
                root
            };
            let origin = evidence
                .input_catalog()
                .definition(slot.entry())
                .ok_or_else(fail)?
                .origin();
            match origin {
                UnitOwnerOrigin::String(id) => {
                    let initial = multi_root_receipt::entry::prefix(
                        &self.body,
                        entry,
                        root,
                        self.span(id.expression()),
                    )
                    .ok_or_else(fail)?;
                    if self.multi_root_literal(origin, initial.producer_block)? != initial.producer
                    {
                        return Err(fail());
                    }
                }
                UnitOwnerOrigin::Parameter(parameter) => {
                    if !self.counter_parameter_root(entry, root, parameter) {
                        return Err(fail());
                    }
                }
                _ => return Err(fail()),
            }
            if let Some(fact) = drops
                .iter()
                .find(|f| f.target() == UnitDropTarget::Named(slot.root()))
            {
                let end = self.multi_root_drop(*fact, finished, root)?;
                drop_order.push((*fact, end));
                if self.body.blocks[finished.0].statements[..=end]
                    .iter()
                    .any(|s| s.results.contains(&root))
                {
                    return Err(fail());
                }
            } else {
                if !live.contains(&slot.root()) {
                    return Err(fail());
                }
                let (_, fact) = suffix
                    .iter()
                    .find(|(r, _)| *r == slot.root())
                    .ok_or_else(fail)?;
                let UnitDropTarget::Named(binding) = fact.target() else {
                    return Err(fail());
                };
                let target = *self.bindings.get(&binding).ok_or_else(fail)?;
                let alias = self.counter_alias(exit, target, root, None)?;
                let end = self.multi_root_drop(*fact, exit, target)?;
                suffix_order.push((slot.root(), alias, end));
                if alias >= end
                    || self.multi_root_definitions(target) != 1
                    || self.body.blocks[finished.0]
                        .statements
                        .iter()
                        .any(|s| s.results.contains(&root))
                    || self.body.blocks[exit.0].statements[..=end]
                        .iter()
                        .any(|s| s.results.contains(&root))
                {
                    return Err(fail());
                }
            }
        }
        let mut prior = None;
        for (root, _) in suffix {
            let (_, alias, end) = suffix_order
                .iter()
                .find(|(r, _, _)| r == root)
                .ok_or_else(fail)?;
            if prior.is_some_and(|p| p >= *alias) {
                return Err(fail());
            }
            prior = Some(*end);
        }
        let mut prior = None;
        for fact in drops {
            let (_, end) = drop_order
                .iter()
                .find(|(f, _)| f == fact)
                .ok_or_else(fail)?;
            if prior.is_some_and(|p| p >= *end) {
                return Err(fail());
            }
            prior = Some(*end);
        }
        Ok(Some(Complete {
            evidence,
            geometry: plan.geometry.ok_or_else(fail)?,
            roots: plan.roots.clone(),
        }))
    }
}
