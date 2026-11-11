//! Additional production audit from new FE products to actual slot CFG.
//! This receipt does NOT replace legacy lowering permissions or certify runtime instances.
use super::*;
mod entry;
mod guard;
mod incoming;
use lang_frontend::ownership_checking::{
    UnitDropFact, UnitOwnerCallable, UnitOwnerHeaderSourceV1, UnitOwnerMultiRootBoundaryV1,
    UnitOwnerOrigin,
};

#[derive(Clone, Copy)]
enum Guard {
    Borrowed(Slot),
    Value(Slot),
}

pub(super) struct Plan<'a> {
    boundary: &'a UnitOwnerMultiRootBoundaryV1,
    steps: &'a [lang_frontend::ownership_checking::UnitOwnerRootReplaceStepV1],
    geometry: Option<[BlockRef; 5]>,
    guard: Option<Guard>,
    roots: Vec<Slot>,
}
impl<'a> Plan<'a> {
    pub(super) fn new(inputs: &RefineInputs<'a>) -> Result<Option<Self>, LoweringError> {
        let Some(item) = inputs.function_item else {
            return Ok(None);
        };
        let callable = UnitOwnerCallable::Function(UnitItemId::new(inputs.source_unit, item));
        let mut products = inputs
            .owned
            .owner_observations()
            .iter()
            .filter(|o| o.callable() == callable)
            .filter_map(|o| {
                Some((
                    o.multi_root_boundary_v1()?,
                    o.multi_root_normal_transport_v1()?,
                ))
            });
        let Some((boundary, rule)) = products.next() else {
            return Ok(None);
        };
        let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
        if products.next().is_some() || !boundary.is_consistent() {
            return Err(fail());
        }
        let incoming: Vec<_> = boundary
            .slots()
            .iter()
            .map(|slot| {
                (
                    slot.root(),
                    UnitOwnerHeaderSourceV1::Backedge(slot.first_body_output()),
                )
            })
            .collect();
        let Some(partition) = boundary.condition_false_partition(boundary.statement(), &incoming)
        else {
            return Ok(None);
        };
        if !partition.live_roots().is_empty()
            && boundary
                .condition_false_suffix_retirements(boundary.statement(), &incoming)
                .is_none()
        {
            return Ok(None);
        }
        Ok(Some(Self {
            boundary,
            steps: rule.steps(),
            geometry: None,
            guard: None,
            roots: Vec::new(),
        }))
    }
}
impl Builder<'_, '_> {
    /// Before emitting effects, reject mixing distinct sealed owner contracts.
    /// This does not replace generic admission, drop facts or the SSA verifier.
    pub(super) fn check_multi_root_plan_exclusivity(&self) -> Result<(), LoweringError> {
        let Some(plan) = &self.multi_root_plan else {
            return Ok(());
        };
        let conflict = self.owner_plan.is_some()
            || self.open_return_plan.is_some()
            || self.header_return_plan.is_some();
        #[cfg(test)]
        let conflict = conflict
            || owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::ConflictingMultiRootAuthority,
            );
        if conflict {
            return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
        }
        // The plan is selected independently of the older single-root plans.
        // Optional actual-CFG eligibility is still checked later at the while.
        let _ = plan;
        #[cfg(test)]
        owner_consumer_test_support::record_action(
            "multi-root-exclusive-plan",
            plan.boundary.slots().len(),
        );
        Ok(())
    }

    pub(super) fn multi_root_while(
        &mut self,
        statement: UnitStatementId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        blocks: [BlockRef; 4],
        span: Span,
    ) -> Result<(), LoweringError> {
        self.counter_tail_while(statement, condition, body, blocks)?;
        let Some(plan) = &mut self.multi_root_plan else {
            return Ok(());
        };
        if plan.boundary.statement() != statement {
            return Ok(());
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        if plan.boundary.condition() != condition
            || plan.boundary.body() != body
            || plan.geometry.is_some()
        {
            return Err(fail());
        }
        plan.roots = plan
            .boundary
            .slots()
            .iter()
            .map(|slot| self.bindings.get(&slot.root()).copied().ok_or_else(fail))
            .collect::<Result<_, _>>()?;
        if plan
            .roots
            .iter()
            .enumerate()
            .any(|(i, root)| plan.roots[..i].contains(root))
        {
            return Err(fail());
        }
        let entry = self.block.ok_or_else(fail)?;
        // Optional audit eligibility, not language admission: this first receipt only
        // covers single-use entry moves followed by acyclic root-transparent paths.
        let entry_chains = plan
            .boundary
            .slots()
            .iter()
            .zip(&plan.roots)
            .all(|(slot, root)| {
                let Some(definition) = plan.boundary.input_catalog().definition(slot.entry())
                else {
                    return false;
                };
                let UnitOwnerOrigin::String(id) = definition.origin() else {
                    return false;
                };
                let Ok(node) = self.inputs.parsed.ast().expressions().get(id.expression()) else {
                    return false;
                };
                entry::prefix(&self.body, entry, *root, node.span()).is_some()
            });
        if !entry_chains {
            self.multi_root_plan = None;
            return Ok(());
        }
        plan.guard = self
            .borrowed
            .get(&plan.boundary.guard())
            .copied()
            .map(Guard::Borrowed)
            .or_else(|| {
                self.bindings
                    .get(&plan.boundary.guard())
                    .copied()
                    .map(Guard::Value)
            });
        if plan.guard.is_none()
            || (plan.boundary.guard_reset().is_some()
                && matches!(plan.guard, Some(Guard::Borrowed(_))))
        {
            self.multi_root_plan = None;
            return Ok(());
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::MissingMultiRootGuard,
        ) {
            plan.guard = None;
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::MissingMultiRootGeometry,
        ) {
            return Ok(());
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::WrongMultiRootRoot,
        ) {
            plan.roots.swap(0, 1);
        }
        plan.geometry = Some([
            self.block.ok_or_else(fail)?,
            blocks[0],
            blocks[1],
            blocks[2],
            blocks[3],
        ]);
        Ok(())
    }
    pub(super) fn finish_multi_root_receipt(&self) -> Result<(), LoweringError> {
        let Some(plan) = &self.multi_root_plan else {
            return Ok(());
        };
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let [entry, header, body, finished, exit] = plan.geometry.ok_or_else(fail)?;
        let incoming = |target| {
            self.body
                .blocks
                .iter()
                .enumerate()
                .filter_map(|(i, block)| {
                    block
                        .terminator
                        .as_ref()
                        .filter(|(term, _)| term.successors().contains(&target))
                        .map(|_| BlockRef(i))
                })
                .collect::<Vec<_>>()
        };
        if !matches!(self.body.blocks[entry.0].terminator, Some((RTerm::Goto(target), _)) if target == header)
            || incoming(header) != [entry, body]
            || incoming(body) != [header]
            || incoming(finished) != [header]
            || incoming(exit) != [finished]
            || !matches!(self.body.blocks[body.0].terminator, Some((RTerm::Goto(target), _)) if target == header)
            || !matches!(self.body.blocks[header.0].terminator, Some((RTerm::Branch { when_true, when_false, .. }, _)) if when_true == body && when_false == finished)
        {
            return Err(fail());
        }
        let Some((RTerm::Branch { condition, .. }, _)) = self.body.blocks[header.0].terminator
        else {
            return Err(fail());
        };
        match plan.guard.ok_or_else(fail)? {
            Guard::Borrowed(loan) => {
                let reads: Vec<_> = self.body.blocks[header.0]
                    .statements
                    .iter()
                    .filter(|s| {
                        s.results == [condition]
                            && s.span == self.span(plan.boundary.condition().expression())
                            && matches!(s.op, ROp::ReadLoan {loan: actual} if actual == loan)
                    })
                    .collect();
                if reads.len() != 1 || self.multi_root_definitions(condition) != 1 {
                    return Err(fail());
                }
            }
            Guard::Value(value) => {
                if condition != value
                    || self.body.blocks[header.0]
                        .statements
                        .iter()
                        .any(|s| s.results.contains(&value))
                {
                    return Err(fail());
                }
                if let Some(reset) = plan.boundary.guard_reset() {
                    self.multi_root_guard_reset(body, value, reset)?;
                } else if self.body.blocks[body.0]
                    .statements
                    .iter()
                    .any(|s| s.results.contains(&value))
                {
                    return Err(fail());
                }
            }
        }

        let boundary = plan.boundary;
        incoming::audit(boundary).ok_or_else(fail)?;
        let sources: Vec<_> = boundary
            .slots()
            .iter()
            .map(|slot| {
                (
                    slot.root(),
                    UnitOwnerHeaderSourceV1::Backedge(slot.first_body_output()),
                )
            })
            .collect();
        let normal = boundary
            .normal_header_retirements(boundary.statement(), &sources)
            .ok_or_else(fail)?;
        let partition = boundary
            .condition_false_partition(boundary.statement(), &sources)
            .ok_or_else(fail)?;
        let suffix = boundary
            .condition_false_suffix_retirements(boundary.statement(), &sources)
            .unwrap_or_default();
        let statements = &self.body.blocks[body.0].statements;
        let replacements: Vec<_> = statements
            .iter()
            .enumerate()
            .filter(|(_, s)| matches!(s.op, ROp::RootReplace { .. }))
            .collect();
        if replacements.len() != boundary.slots().len() {
            return Err(fail());
        }
        if replacements.len() != plan.steps.len()
            || replacements
                .iter()
                .zip(plan.steps)
                .any(|((_, statement), step)| statement.span != self.span(step.call().expression()))
        {
            return Err(fail());
        }
        for (root_index, slot) in boundary.slots().iter().enumerate() {
            let root = plan.roots[root_index];
            let origin = boundary
                .input_catalog()
                .definition(slot.entry())
                .ok_or_else(fail)?
                .origin();
            let UnitOwnerOrigin::String(id) = origin else {
                return Err(fail());
            };
            let initial = entry::prefix(&self.body, entry, root, self.span(id.expression()))
                .ok_or_else(fail)?;
            if self.multi_root_literal(origin, initial.producer_block)? != initial.producer {
                return Err(fail());
            }
            #[cfg(test)]
            if initial.root_block != entry || initial.producer_block != initial.root_block {
                owner_consumer_test_support::record_action(
                    "multi-root-cross-block-entry",
                    root_index,
                );
            }
            let produced = self.multi_root_literal(
                boundary
                    .body_catalog()
                    .definition(slot.first_body_output())
                    .ok_or_else(fail)?
                    .origin(),
                body,
            )?;
            let step = plan
                .steps
                .iter()
                .find(|step| step.header_root() == slot.root())
                .ok_or_else(fail)?;
            let call_span = self.span(step.call().expression());
            let matching: Vec<_> = replacements.iter().filter(|(_, s)| s.span == call_span && matches!(s.op, ROp::RootReplace {owner, replacement, ..} if owner == root && replacement == produced)).collect();
            let [(replace_index, replace)] = matching.as_slice() else {
                return Err(fail());
            };
            if replace.results.len() != 2 || replace.results[0] != root {
                return Err(fail());
            }
            let old = replace.results[1];
            let old_fact = slot.returned_value_drop();
            let UnitDropTarget::Named(binding) = old_fact.target() else {
                return Err(fail());
            };
            let detached = *self.bindings.get(&binding).ok_or_else(fail)?;
            let aliases: Vec<_> = statements
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    s.results == [detached] && matches!(s.op, ROp::Alias {source} if source == old)
                })
                .collect();
            let [(alias_index, _)] = aliases.as_slice() else {
                return Err(fail());
            };
            let drop_index = self.multi_root_drop(old_fact, body, detached)?;
            if *replace_index >= *alias_index
                || *alias_index >= drop_index
                || old == root
                || detached == root
                || self.body.blocks[initial.root_block.0]
                    .statements
                    .iter()
                    .filter(|s| s.results.contains(&root))
                    .count()
                    != 1
                || statements
                    .iter()
                    .filter(|s| s.results.contains(&root))
                    .count()
                    != 1
                || self.multi_root_definitions(old) != 1
                || self.multi_root_definitions(detached) != 1
                || self.body.blocks[header.0]
                    .statements
                    .iter()
                    .any(|s| s.results.contains(&root) || s.op.operands().contains(&root))
            {
                return Err(fail());
            }
            let source = UnitOwnerHeaderSourceV1::Backedge(slot.first_body_output());
            if !normal
                .iter()
                .any(|(known, fact)| *known == source && *fact == old_fact)
            {
                return Err(fail());
            }
            if let Some((_, fact)) = partition
                .retirements()
                .iter()
                .find(|(known, _)| *known == source)
            {
                let drop_index = self.multi_root_drop(*fact, finished, root)?;
                if self.body.blocks[finished.0].statements[..=drop_index]
                    .iter()
                    .any(|s| s.results.contains(&root))
                {
                    return Err(fail());
                }
            } else {
                let (_, fact) = suffix
                    .iter()
                    .find(|(known, _)| *known == source)
                    .ok_or_else(fail)?;
                let UnitDropTarget::Named(binding) = fact.target() else {
                    return Err(fail());
                };
                let target = *self.bindings.get(&binding).ok_or_else(fail)?;
                let aliases: Vec<_> = self.body.blocks[exit.0]
                    .statements
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| {
                        s.results == [target]
                            && matches!(s.op, ROp::Alias {source} if source == root)
                    })
                    .collect();
                let [(alias_index, _)] = aliases.as_slice() else {
                    return Err(fail());
                };
                let drop_index = self.multi_root_drop(*fact, exit, target)?;
                if *alias_index >= drop_index
                    || self.body.blocks[finished.0]
                        .statements
                        .iter()
                        .any(|s| s.results.contains(&root))
                    || self.body.blocks[exit.0].statements[..=drop_index]
                        .iter()
                        .any(|s| s.results.contains(&root))
                    || self.multi_root_definitions(target) != 1
                {
                    return Err(fail());
                }
            }
        }
        #[cfg(test)]
        owner_consumer_test_support::record_action("multi-root-header-audit", plan.roots.len());
        Ok(())
    }
    pub(super) fn multi_root_definitions(&self, slot: Slot) -> usize {
        self.body
            .blocks
            .iter()
            .flat_map(|b| &b.statements)
            .filter(|s| s.results.contains(&slot))
            .count()
    }
    pub(super) fn multi_root_literal(
        &self,
        origin: UnitOwnerOrigin,
        block: BlockRef,
    ) -> Result<Slot, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let UnitOwnerOrigin::String(id) = origin else {
            return Err(fail());
        };
        if id.source_unit() != self.inputs.source_unit {
            return Err(fail());
        }
        let matches: Vec<_> = self.body.blocks[block.0]
            .statements
            .iter()
            .filter(|s| {
                s.span == self.span(id.expression()) && matches!(s.op, ROp::StringLiteral { .. })
            })
            .collect();
        let [statement] = matches.as_slice() else {
            return Err(fail());
        };
        let [slot] = statement.results.as_slice() else {
            return Err(fail());
        };
        if self.multi_root_definitions(*slot) != 1 {
            return Err(fail());
        }
        Ok(*slot)
    }
    pub(super) fn multi_root_drop(
        &self,
        fact: UnitDropFact,
        block: BlockRef,
        owner: Slot,
    ) -> Result<usize, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::MissingMultiRootDropReceipt,
        ) {
            return Err(fail());
        }
        let index = self
            .inputs
            .owned
            .drops()
            .iter()
            .position(|original| *original == fact)
            .ok_or_else(fail)?;
        if !self.consumed_drops.contains(&index) {
            return Err(fail());
        }
        let matches: Vec<_> = self.body.blocks[block.0]
            .statements
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.span == fact.value_origin()
                    && matches!(s.op, ROp::Drop {owner: actual} if actual == owner)
            })
            .collect();
        let [(index, _)] = matches.as_slice() else {
            return Err(fail());
        };
        // Later source-level reassignment may reuse a slot after this finite prefix.
        // Global SSA verification still rejects duplicate consumption of one definition.
        Ok(*index)
    }
}
