//! Non-authorizing slot-CFG correspondence for a real cross-control alias bridge.
use super::*;
use lang_frontend::ownership_checking::{UnitOwnerCallable, UnitOwnerControlBridgeEvidenceV1};

pub(super) struct Plan<'a> {
    evidence: &'a UnitOwnerControlBridgeEvidenceV1,
    prior: Option<([BlockRef; 4], Vec<Slot>, Slot)>,
    next: Option<(BlockRef, BlockRef, Vec<Slot>)>,
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
            evidence,
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
            if plan.prior.is_none() || plan.next.is_some() {
                return Err(fail());
            }
            let roots = plan
                .evidence
                .targets()
                .iter()
                .map(|s| self.bindings.get(s).copied().ok_or_else(fail))
                .collect::<Result<Vec<_>, _>>()?;
            plan.next = Some((self.block.ok_or_else(fail)?, blocks[0], roots));
        }
        Ok(())
    }
    pub(super) fn finish_control_bridge(&mut self) -> Result<(), LoweringError> {
        #[cfg(test)]
        if let Some(plan) = &mut self.control_bridge_plan {
            use owner_consumer_test_support::{OwnerFault, take_fault};
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
        }
        let Some(plan) = &self.control_bridge_plan else {
            return Ok(());
        };
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let ([header, body, finished, exit], roots, guard) =
            plan.prior.as_ref().ok_or_else(fail)?;
        let (entry, next_header, targets) = plan.next.as_ref().ok_or_else(fail)?;
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
        // This audits actual slot transport, not runtime generation identities or
        // a permission to consume the next body's roots. SSA verification still follows.
        #[cfg(test)]
        owner_consumer_test_support::record_action("control-bridge-slot-audit", roots.len());
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
