//! Slot-CFG bridge audit with an optional independent normal-body consumption rule.
use super::*;
mod body_group;
mod consumption;
pub(super) use body_group::BodyGroup;
mod counter;
mod entry;
mod exit;
mod suffix;
use lang_frontend::ownership_checking::{UnitOwnerCallable, UnitOwnerControlBridgeEvidenceV1};

pub(super) struct Plan<'a> {
    evidence: &'a UnitOwnerControlBridgeEvidenceV1,
    chain: Vec<consumption::Stage>,
    scalar: Option<[Slot; 2]>,
    initial: [Option<entry::Binding>; 2],
    exits: Option<Vec<bool>>,
    suffix: Option<Vec<suffix::Stage>>,
    prior: Option<([BlockRef; 4], Vec<Slot>, Slot)>,
    next: Option<(BlockRef, [BlockRef; 4], Vec<Slot>)>,
}
impl<'a> Plan<'a> {
    pub(super) fn new(inputs: &RefineInputs<'a>) -> Result<Option<Self>, LoweringError> {
        let Some(item) = inputs.function_item else {
            return Ok(None);
        };
        let callable = UnitOwnerCallable::Function(UnitItemId::new(inputs.source_unit, item));
        let mut selected = inputs
            .owned
            .owner_observations()
            .iter()
            .filter(|o| o.callable() == callable)
            .filter_map(|o| o.control_bridge_evidence_v1());
        let Some(evidence) = selected.next() else {
            return Ok(None);
        };
        if selected.next().is_some() || !evidence.is_consistent() {
            return Err(error(LoweringErrorKind::InvalidModel, inputs.span));
        }
        Ok(Some(Self {
            chain: vec![
                consumption::Stage::Ready;
                evidence
                    .next_normal_transport()
                    .map_or(0, |rule| rule.steps().len())
            ],
            evidence,
            exits: evidence
                .next_false_exit_partition()
                .map(|(_, drops)| vec![false; drops.len()]),
            suffix: evidence
                .next_false_exit_suffix()
                .map(|items| vec![suffix::Stage::Ready; items.len()]),
            scalar: None,
            initial: [None, None],
            prior: None,
            next: None,
        }))
    }
}
impl Builder<'_, '_> {
    pub(super) fn control_bridge_while(
        &mut self,
        control: UnitStatementId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        blocks: [BlockRef; 4],
    ) -> Result<(), LoweringError> {
        self.control_chain_body_while(control, condition, body, blocks)?;
        if self
            .control_bridge_plan
            .as_ref()
            .is_some_and(|p| p.evidence.next_control() == control)
        {
            self.check_bridge_entry()?;
        }
        let Some(plan) = &mut self.control_bridge_plan else {
            return Ok(());
        };
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        if control == plan.evidence.prior().statement() {
            let prior = plan.evidence.prior();
            if plan.prior.is_some()
                || plan.next.is_some()
                || condition != prior.condition()
                || body != prior.body()
            {
                return Err(fail());
            }
            let roots = plan
                .evidence
                .sources()
                .iter()
                .map(|symbol| self.bindings.get(symbol).copied().ok_or_else(fail))
                .collect::<Result<Vec<_>, _>>()?;
            let guard = self
                .bindings
                .get(&prior.guard())
                .copied()
                .ok_or_else(fail)?;
            plan.prior = Some((blocks, roots, guard));
        } else if control == plan.evidence.next_control() {
            if plan.prior.is_none()
                || plan.next.is_some()
                || condition != plan.evidence.next_condition()
                || body != plan.evidence.next_body()
            {
                return Err(fail());
            }
            let roots = plan
                .evidence
                .targets()
                .iter()
                .map(|s| self.bindings.get(s).copied().ok_or_else(fail))
                .collect::<Result<Vec<_>, _>>()?;
            if let Some(tail) = plan.evidence.next_counter_tail() {
                plan.scalar = Some([
                    self.bindings
                        .get(&tail.counter())
                        .copied()
                        .ok_or_else(fail)?,
                    self.bindings.get(&tail.guard()).copied().ok_or_else(fail)?,
                ]);
            }
            plan.next = Some((self.block.ok_or_else(fail)?, blocks, roots));
        }
        Ok(())
    }
    pub(super) fn finish_control_bridge(&mut self) -> Result<(), LoweringError> {
        self.finish_control_chain_bodies()?;
        #[cfg(test)]
        if let Some(plan) = &mut self.control_bridge_plan {
            use owner_consumer_test_support::{OwnerFault, take_fault};
            if take_fault(OwnerFault::BridgeEntryLateWrongLiteral)
                && let (Some(entry), Some((block, _, _))) =
                    (plan.evidence.next_counter_entry(), &plan.next)
            {
                for statement in &mut self.body.blocks[block.0].statements {
                    if statement.span
                        == self
                            .inputs
                            .parsed
                            .ast()
                            .expressions()
                            .get(entry.counter_initializer().expression())
                            .map(|n| n.span())
                            .unwrap_or(self.inputs.span)
                        && let ROp::Constant(crate::ssa::model::ScalarConstant::Integer(value)) =
                            &mut statement.op
                    {
                        *value = 0;
                    }
                }
            }
            if take_fault(OwnerFault::BridgeEntryLateWrongComparison)
                && let (Some(entry), Some((block, _, _))) =
                    (plan.evidence.next_counter_entry(), &plan.next)
            {
                for statement in &mut self.body.blocks[block.0].statements {
                    if statement.span
                        == self
                            .inputs
                            .parsed
                            .ast()
                            .expressions()
                            .get(entry.guard_initializer().expression())
                            .map(|n| n.span())
                            .unwrap_or(self.inputs.span)
                        && let ROp::Compare { left, right, .. } = &mut statement.op
                    {
                        *left = *right;
                    }
                }
            }
            if take_fault(OwnerFault::ControlBridgeWrongRoot)
                && let Some((_, _, roots)) = &mut plan.next
            {
                roots.reverse();
            }
            if take_fault(OwnerFault::ControlBridgeMissingNext) {
                plan.next = None;
            }
            if take_fault(OwnerFault::ControlBridgeLateSourceDrop)
                && let (Some((_, roots, _)), Some((entry, _, _))) = (&plan.prior, &plan.next)
            {
                let statements = &mut self.body.blocks[entry.0].statements;
                let mut late = statements[0].clone();
                late.op = ROp::Drop { owner: roots[0] };
                late.results.clear();
                statements.push(late);
            }
            if take_fault(OwnerFault::ControlBridgeHeaderRootDrop)
                && let Some((entry, [header, ..], roots)) = &plan.next
            {
                let mut injected = self.body.blocks[entry.0].statements[0].clone();
                injected.op = ROp::Drop { owner: roots[0] };
                injected.results.clear();
                self.body.blocks[header.0].statements.push(injected);
            }
            if take_fault(OwnerFault::BridgeExitWrongExitEdge)
                && let Some((_, [header, _, finished, _], _)) = &plan.next
            {
                self.body.blocks[finished.0].terminator =
                    Some((RTerm::Goto(*header), self.inputs.span));
            }
            if take_fault(OwnerFault::BridgeExitLateWrongOwner)
                && let Some((_, [_, _, finished, _], roots)) = &plan.next
                && let Some(statement) = self.body.blocks[finished.0].statements.first_mut()
            {
                let replacement = if matches!(statement.op, ROp::Drop { owner } if owner == roots[0])
                {
                    roots[1]
                } else {
                    roots[0]
                };
                statement.op = ROp::Drop { owner: replacement };
            }
            if take_fault(OwnerFault::ControlBridgeWrongBodyEdge)
                && let Some((_, [header, _, finished, _], _)) = &plan.next
                && let Some((RTerm::Branch { when_true, .. }, _)) =
                    &mut self.body.blocks[header.0].terminator
            {
                *when_true = *finished;
            }
        }
        self.check_bridge_entry()?;
        self.check_bridge_counter_tail()?;
        self.finish_bridge_suffix()?;
        let Some(plan) = &self.control_bridge_plan else {
            return Ok(());
        };
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let ([header, body, finished, exit], roots, guard) =
            plan.prior.as_ref().ok_or_else(fail)?;
        let (entry, [next_header, next_body, next_finished, next_exit], targets) =
            plan.next.as_ref().ok_or_else(fail)?;
        let predecessors = |target| {
            self.body
                .blocks
                .iter()
                .enumerate()
                .filter_map(|(i, b)| {
                    b.terminator
                        .as_ref()
                        .filter(|(t, _)| t.successors().contains(&target))
                        .map(|_| BlockRef(i))
                })
                .collect::<Vec<_>>()
        };
        if exit != entry
            || header == next_header
            || roots.len() != targets.len()
            || !matches!(self.body.blocks[header.0].terminator, Some((RTerm::Branch {condition,when_true,when_false},_)) if condition == *guard && when_true == *body && when_false == *finished)
            || !matches!(self.body.blocks[finished.0].terminator, Some((RTerm::Goto(t),_)) if t == *exit)
            || !matches!(self.body.blocks[entry.0].terminator, Some((RTerm::Goto(t),_)) if t == *next_header)
            || !matches!(self.body.blocks[next_header.0].terminator, Some((RTerm::Branch {condition,when_true,when_false},_)) if when_true == *next_body && when_false == *next_finished && !targets.contains(&condition))
            || self.body.blocks[next_header.0].statements.iter().any(|s| {
                targets
                    .iter()
                    .any(|root| s.results.contains(root) || s.op.operands().contains(root))
            })
            || predecessors(*next_body) != [*next_header]
            || predecessors(*finished) != [*header]
            || predecessors(*exit) != [*finished]
            || self.body.blocks[finished.0].statements.iter().any(|s| {
                roots
                    .iter()
                    .any(|root| s.results.contains(root) || s.op.operands().contains(root))
            })
            || roots
                .iter()
                .enumerate()
                .any(|(i, r)| roots[..i].contains(r))
            || targets
                .iter()
                .enumerate()
                .any(|(i, r)| targets[..i].contains(r))
        {
            return Err(fail());
        }
        for (root, target) in roots.iter().zip(targets) {
            if !alias_prefix(&self.body, *entry, *target, *root) {
                return Err(fail());
            }
        }
        if let Some((live, drops)) = plan.evidence.next_false_exit_partition() {
            let consumed = plan.exits.as_ref().ok_or_else(fail)?;
            if consumed.len() != drops.len()
                || consumed.iter().any(|done| !done)
                || predecessors(*next_finished) != [*next_header]
                || predecessors(*next_exit) != [*next_finished]
                || !matches!(self.body.blocks[next_finished.0].terminator, Some((RTerm::Goto(t),_)) if t == *next_exit)
            {
                return Err(fail());
            }
            let statements = &self.body.blocks[next_finished.0].statements;
            if statements.len() != drops.len()
                || statements
                    .iter()
                    .any(|s| !s.results.is_empty() || !matches!(s.op, ROp::Drop { .. }))
                || drops.iter().zip(statements).any(|(fact, statement)| {
                    let UnitDropTarget::Named(root) = fact.target() else {
                        return true;
                    };
                    plan.evidence
                        .targets()
                        .iter()
                        .position(|r| *r == root)
                        .is_none_or(
                            |i| !matches!(statement.op, ROp::Drop { owner } if owner == targets[i]),
                        )
                })
                || live.iter().any(|symbol| {
                    plan.evidence
                        .targets()
                        .iter()
                        .position(|r| r == symbol)
                        .is_none_or(|i| {
                            statements
                                .iter()
                                .any(|s| s.op.operands().contains(&targets[i]))
                        })
                })
            {
                return Err(fail());
            }
        }
        if let Some(rule) = plan.evidence.next_normal_transport() {
            if plan.chain.len() != rule.steps().len()
                || plan
                    .chain
                    .iter()
                    .any(|stage| *stage != consumption::Stage::Dropped)
                || self.body.blocks[next_body.0]
                    .statements
                    .iter()
                    .filter(|s| matches!(s.op, ROp::RootReplace { .. }))
                    .count()
                    != rule.steps().len()
            {
                return Err(fail());
            }
            #[cfg(test)]
            {
                if plan.evidence.next_counter_tail().is_some() {
                    owner_consumer_test_support::record_action(
                        "bridge-counter-tail-complete",
                        rule.steps().len(),
                    );
                }
                if let Some((live, drops)) = plan.evidence.next_false_exit_partition() {
                    owner_consumer_test_support::record_action(
                        "bridge-false-exit-partition-consumed",
                        drops.len(),
                    );
                    if live.is_empty() {
                        owner_consumer_test_support::record_action(
                            "bridge-false-exit-all-dead",
                            drops.len(),
                        );
                    }
                }
                if plan.evidence.next_counter_entry().is_some() {
                    owner_consumer_test_support::record_action("bridge-counter-entry-complete", 2);
                }
                if let Some(items) = plan.evidence.next_false_exit_suffix() {
                    owner_consumer_test_support::record_action(
                        "bridge-suffix-complete",
                        items.len(),
                    );
                }
                owner_consumer_test_support::record_action(
                    "bridge-body-complete",
                    rule.steps().len(),
                );
            }
        }
        // The entry bridge audits actual slot transport, not runtime generations.
        // Optional body permits above do not authorize the whole loop. SSA verification follows.
        #[cfg(test)]
        owner_consumer_test_support::record_action("control-bridge-slot-audit", roots.len());
        if let Some(tail) = plan.evidence.next_counter_tail() {
            let arithmetic = tail.assignments()[1];
            let mut edges = self
                .checked_edge_receipts
                .iter()
                .filter(|r| r.expression == arithmetic);
            let edge = edges.next().ok_or_else(fail)?;
            if edges.next().is_some() {
                return Err(fail());
            }
            self.body.owner_bridge_plans = super::super::control_bridge_receipt::Plans::seal(
                [*header, *body, *finished, *exit],
                [*next_header, *next_body, *next_finished, *next_exit],
                [edge.blocks[1], edge.blocks[2]],
                roots.clone(),
                targets.clone(),
                self.inputs.span,
            )?;
        }
        Ok(())
    }
}
fn alias_prefix(body: &RefinedBody, block: BlockRef, mut value: Slot, source: Slot) -> bool {
    let statements = &body.blocks[block.0].statements;
    let mut before = statements.len();
    let mut path = Vec::new();
    while value != source {
        let Some((i, statement)) = statements[..before]
            .iter()
            .enumerate()
            .rev()
            .find(|(_, statement)| statement.results.contains(&value))
        else {
            return false;
        };
        let ROp::Alias { source: prior } = statement.op else {
            return false;
        };
        if statement.results != [value] || path.iter().any(|(slot, _)| *slot == value) {
            return false;
        }
        path.push((value, i));
        value = prior;
        before = i;
    }
    let uses = |slot: Slot, definition: Option<usize>, consumption: Option<usize>| {
        statements.iter().enumerate().all(|(i, statement)| {
            statement.results.contains(&slot) == (definition == Some(i))
                && statement.op.operands().contains(&slot) == (consumption == Some(i))
        })
    };
    // Check the ENTIRE block. A source use after its move is not hidden by the
    // backwards search cutoff, and intermediate aliases have the same obligation.
    uses(source, None, path.last().map(|(_, i)| *i))
        && path
            .iter()
            .enumerate()
            .all(|(position, (slot, definition))| {
                uses(
                    *slot,
                    Some(*definition),
                    position.checked_sub(1).map(|i| path[i].1),
                )
            })
}
