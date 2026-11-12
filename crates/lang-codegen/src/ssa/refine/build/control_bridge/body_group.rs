//! One group of normal-body consumers, keyed by actual control, never entry tokens.
use super::*;
use lang_frontend::ownership_checking::{
    UnitDropFact, UnitOwnerControlChainBodyV1, UnitOwnerMultiRootNormalTransportV1,
};

pub(in super::super) struct BodyGroup<'a> {
    members: Vec<Member<'a>>,
    link: (&'a [UnitSymbolId], &'a [UnitSymbolId]),
}
struct Member<'a> {
    evidence: UnitOwnerControlChainBodyV1<'a>,
    stages: Vec<consumption::Stage>,
    lowered: Option<([BlockRef; 4], Vec<Slot>)>,
    exits: Vec<bool>,
    counter: Option<(BlockRef, [Slot; 2])>,
    suffix: Option<Vec<suffix::Stage>>,
    prior_exit: Option<BlockRef>,
    initial: [Option<entry::Binding>; 2],
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
            link: group.inter_member_roots(),
            members: group
                .members()
                .iter()
                .map(|e| Member {
                    evidence: *e,
                    stages: vec![consumption::Stage::Ready; e.normal().steps().len()],
                    lowered: None,
                    counter: None,
                    prior_exit: None,
                    initial: [None, None],
                    suffix: e
                        .false_exit_suffix()
                        .map(|s| vec![suffix::Stage::Ready; s.len()]),
                    exits: vec![false; e.false_exit_partition().1.len()],
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
        if self
            .control_chain_bodies
            .as_ref()
            .is_some_and(|g| g.members.iter().any(|m| m.evidence.control() == control))
        {
            self.check_control_entry(control)?;
        }
        let Some(group) = &mut self.control_chain_bodies else {
            return Ok(());
        };
        for member in &mut group.members {
            if member.evidence.prior_control() == control
                && member.prior_exit.replace(blocks[3]).is_some()
            {
                return Err(fail());
            }
        }
        let Some(index) = group
            .members
            .iter()
            .position(|m| m.evidence.control() == control)
        else {
            return Ok(());
        };
        if group.members[..index].iter().any(|m| {
            m.lowered.is_none()
                || m.stages.iter().any(|s| *s != consumption::Stage::Dropped)
                || m.exits.iter().any(|done| !done)
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
        let (counter, guard, _) = member.evidence.checked_tail();
        member.counter = Some((
            self.block.ok_or_else(fail)?,
            [
                self.bindings.get(&counter).copied().ok_or_else(fail)?,
                self.bindings.get(&guard).copied().ok_or_else(fail)?,
            ],
        ));
        member.lowered = Some((blocks, roots));
        Ok(())
    }
    pub(super) fn finish_control_chain_bodies(&mut self) -> Result<(), LoweringError> {
        let Some(group) = &self.control_chain_bodies else {
            return Ok(());
        };
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        for member in &group.members {
            let (blocks, roots) = member.lowered.as_ref().ok_or_else(fail)?;
            let block = blocks[1];
            self.check_control_entry(member.evidence.control())?;
            self.check_chain_false_exit(member, *blocks, roots)?;
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
        self.finish_bridge_suffix()?;
        #[cfg(test)]
        let count = group.members.len();
        let tails = group
            .members
            .iter()
            .map(|m| {
                let (blocks, roots) = m.lowered.as_ref().ok_or_else(fail)?;
                let (entry, scalar) = m.counter.ok_or_else(fail)?;
                Ok((
                    m.evidence.checked_tail().2,
                    entry,
                    *blocks,
                    roots.clone(),
                    scalar,
                ))
            })
            .collect::<Result<Vec<_>, LoweringError>>()?;
        for (index, (sites, entry, blocks, roots, scalar)) in tails.into_iter().enumerate() {
            #[cfg(test)]
            if index == 1
                && owner_consumer_test_support::take_fault(
                    owner_consumer_test_support::OwnerFault::ChainLastTailMissingReceipt,
                )
            {
                self.checked_edge_receipts
                    .retain(|r| r.expression != sites[1]);
                owner_consumer_test_support::record_action("control-chain-tail-fault-hit", index);
            }
            let _ = index;
            self.check_control_counter_tail(sites, entry, blocks, &roots, scalar)?;
        }
        let members = self
            .control_chain_bodies
            .as_ref()
            .ok_or_else(fail)?
            .members
            .iter()
            .map(|m| {
                let (blocks, roots) = m.lowered.as_ref().ok_or_else(fail)?;
                let arithmetic = m.evidence.checked_tail().2[1];
                let mut edges = self
                    .checked_edge_receipts
                    .iter()
                    .filter(|r| r.expression == arithmetic);
                let edge = edges.next().ok_or_else(fail)?;
                if edges.next().is_some() || edge.blocks[0] != blocks[1] {
                    return Err(fail());
                }
                Ok(super::super::super::control_bridge_receipt::Member {
                    control: m.evidence.control(),
                    blocks: *blocks,
                    checked: [edge.blocks[1], edge.blocks[2]],
                    roots: roots.clone(),
                })
            })
            .collect::<Result<Vec<_>, LoweringError>>()?;
        let group = self.control_chain_bodies.as_ref().ok_or_else(fail)?;
        let first = group.members.first().ok_or_else(fail)?;
        let last = group.members.last().ok_or_else(fail)?;
        let (prior_blocks, prior_roots) = first.lowered.as_ref().ok_or_else(fail)?;
        let (next_blocks, next_roots) = last.lowered.as_ref().ok_or_else(fail)?;
        let exit = prior_blocks[3];
        if last.counter.map(|c| c.0) != Some(exit)
            || group.link.0.len() != group.link.1.len()
            || !matches!(self.body.blocks[exit.0].terminator, Some((RTerm::Goto(t), _)) if t == next_blocks[0])
        {
            return Err(fail());
        }
        let pairs = group
            .link
            .0
            .iter()
            .zip(group.link.1)
            .map(|(source, target)| {
                let a = first
                    .evidence
                    .roots()
                    .iter()
                    .position(|s| s == source)
                    .ok_or_else(fail)?;
                let b = last
                    .evidence
                    .roots()
                    .iter()
                    .position(|s| s == target)
                    .ok_or_else(fail)?;
                let pair = (prior_roots[a], next_roots[b]);
                if !super::alias_prefix(&self.body, exit, pair.1, pair.0) {
                    return Err(fail());
                }
                Ok(pair)
            })
            .collect::<Result<Vec<_>, LoweringError>>()?;
        let link = super::super::super::control_bridge_receipt::Link {
            from_control: first.evidence.control(),
            to_control: last.evidence.control(),
            block: exit,
            pairs,
        };
        self.body.owner_bridge_plans =
            super::super::super::control_bridge_receipt::Plans::seal_group(
                members,
                link,
                self.inputs.span,
            )?;
        #[cfg(test)]
        owner_consumer_test_support::record_action("control-chain-tails-complete", count);
        #[cfg(test)]
        owner_consumer_test_support::record_action("control-chain-exits-complete", count);
        #[cfg(test)]
        owner_consumer_test_support::record_action("control-chain-bodies-complete", count);
        Ok(())
    }
}

pub(super) struct SelectedExit<'a> {
    pub control: UnitStatementId,
    pub symbols: &'a [UnitSymbolId],
    pub roots: &'a [Slot],
    pub finished: BlockRef,
    pub stages: &'a [consumption::Stage],
    pub drops: &'a [UnitDropFact],
    pub consumed: &'a [bool],
}
impl Builder<'_, '_> {
    pub(super) fn bridge_select_exit(
        &self,
        matches: impl Fn(UnitStatementId, &[UnitDropFact], &[bool]) -> bool,
    ) -> Result<Option<SelectedExit<'_>>, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        if let Some(group) = &self.control_chain_bodies {
            if self.control_bridge_plan.is_some() {
                return Err(fail());
            }
            let mut members = group.members.iter().filter(|m| {
                matches(
                    m.evidence.control(),
                    m.evidence.false_exit_partition().1,
                    &m.exits,
                )
            });
            let Some(m) = members.next() else {
                return Ok(None);
            };
            if members.next().is_some() {
                return Err(fail());
            }
            let (blocks, roots) = m.lowered.as_ref().ok_or_else(fail)?;
            return Ok(Some(SelectedExit {
                control: m.evidence.control(),
                symbols: m.evidence.roots(),
                roots,
                finished: blocks[2],
                stages: &m.stages,
                drops: m.evidence.false_exit_partition().1,
                consumed: &m.exits,
            }));
        }
        let Some(p) = &self.control_bridge_plan else {
            return Ok(None);
        };
        let Some((_, drops)) = p.evidence.next_false_exit_partition() else {
            return Ok(None);
        };
        let consumed = p.exits.as_ref().ok_or_else(fail)?;
        if !matches(p.evidence.next_control(), drops, consumed) {
            return Ok(None);
        }
        let (_, blocks, roots) = p.next.as_ref().ok_or_else(fail)?;
        Ok(Some(SelectedExit {
            control: p.evidence.next_control(),
            symbols: p.evidence.targets(),
            roots,
            finished: blocks[2],
            stages: &p.chain,
            drops,
            consumed,
        }))
    }
    pub(super) fn bridge_exit_states_mut(
        &mut self,
        control: UnitStatementId,
    ) -> Result<&mut Vec<bool>, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        if let Some(group) = &mut self.control_chain_bodies {
            if self.control_bridge_plan.is_some() {
                return Err(fail());
            }
            return group
                .members
                .iter_mut()
                .find(|m| m.evidence.control() == control)
                .map(|m| &mut m.exits)
                .ok_or_else(fail);
        }
        self.control_bridge_plan
            .as_mut()
            .filter(|p| p.evidence.next_control() == control)
            .and_then(|p| p.exits.as_mut())
            .ok_or_else(fail)
    }
    fn check_chain_false_exit(
        &self,
        m: &Member<'_>,
        blocks: [BlockRef; 4],
        roots: &[Slot],
    ) -> Result<(), LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let [header, body, finished, exit] = blocks;
        let (live, drops) = m.evidence.false_exit_partition();
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
        let statements = &self.body.blocks[finished.0].statements;
        if m.exits.len() != drops.len()
            || m.exits.iter().any(|d| !d)
            || predecessors(finished) != [header]
            || predecessors(exit) != [finished]
            || !matches!(self.body.blocks[header.0].terminator, Some((RTerm::Branch {when_true,when_false,..},_)) if when_true==body && when_false==finished)
            || !matches!(self.body.blocks[finished.0].terminator, Some((RTerm::Goto(t),_)) if t==exit)
            || statements.len() != drops.len()
            || drops.iter().zip(statements).any(|(fact, s)| {
                let UnitDropTarget::Named(root) = fact.target() else {
                    return true;
                };
                m.evidence
                    .roots()
                    .iter()
                    .position(|r| *r == root)
                    .is_none_or(|i| {
                        !s.results.is_empty()
                            || !matches!(s.op, ROp::Drop {owner} if owner==roots[i])
                    })
            })
            || live.iter().any(|r| {
                m.evidence
                    .roots()
                    .iter()
                    .position(|x| x == r)
                    .is_none_or(|i| {
                        statements
                            .iter()
                            .any(|s| s.op.operands().contains(&roots[i]))
                    })
            })
        {
            return Err(fail());
        }
        Ok(())
    }
}

pub(super) struct SelectedSuffix<'a> {
    pub control: UnitStatementId,
    pub items: &'a [(UnitSymbolId, UnitDropFact)],
    pub symbols: &'a [UnitSymbolId],
    pub roots: &'a [Slot],
    pub exit: BlockRef,
    pub stages: &'a [suffix::Stage],
    pub complete: bool,
}
impl Builder<'_, '_> {
    pub(super) fn bridge_select_suffix(
        &self,
        matches: impl Fn(UnitStatementId, &[(UnitSymbolId, UnitDropFact)]) -> bool,
    ) -> Result<Option<SelectedSuffix<'_>>, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        if let Some(group) = &self.control_chain_bodies {
            if self.control_bridge_plan.is_some() {
                return Err(fail());
            }
            let mut found = group.members.iter().filter_map(|m| {
                let items = m.evidence.false_exit_suffix()?;
                matches(m.evidence.control(), items).then_some((m, items))
            });
            let Some((m, items)) = found.next() else {
                return Ok(None);
            };
            if found.next().is_some() {
                return Err(fail());
            }
            let (blocks, roots) = m.lowered.as_ref().ok_or_else(fail)?;
            return Ok(Some(SelectedSuffix {
                control: m.evidence.control(),
                items,
                symbols: m.evidence.roots(),
                roots,
                exit: blocks[3],
                stages: m.suffix.as_deref().ok_or_else(fail)?,
                complete: m.exits.iter().all(|d| *d)
                    && m.stages.iter().all(|s| *s == consumption::Stage::Dropped),
            }));
        }
        let Some(p) = &self.control_bridge_plan else {
            return Ok(None);
        };
        let Some(items) = p.evidence.next_false_exit_suffix() else {
            return Ok(None);
        };
        if !matches(p.evidence.next_control(), items) {
            return Ok(None);
        }
        let (_, blocks, roots) = p.next.as_ref().ok_or_else(fail)?;
        Ok(Some(SelectedSuffix {
            control: p.evidence.next_control(),
            items,
            symbols: p.evidence.targets(),
            roots,
            exit: blocks[3],
            stages: p.suffix.as_deref().ok_or_else(fail)?,
            complete: p.exits.as_ref().is_some_and(|e| e.iter().all(|d| *d))
                && p.chain.iter().all(|s| *s == consumption::Stage::Dropped),
        }))
    }
    pub(super) fn bridge_suffix_states_mut(
        &mut self,
        control: UnitStatementId,
    ) -> Result<&mut Vec<suffix::Stage>, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        if let Some(group) = &mut self.control_chain_bodies {
            if self.control_bridge_plan.is_some() {
                return Err(fail());
            }
            return group
                .members
                .iter_mut()
                .find(|m| m.evidence.control() == control)
                .and_then(|m| m.suffix.as_mut())
                .ok_or_else(fail);
        }
        self.control_bridge_plan
            .as_mut()
            .filter(|p| p.evidence.next_control() == control)
            .and_then(|p| p.suffix.as_mut())
            .ok_or_else(fail)
    }
}

pub(super) struct SelectedEntry<'a> {
    pub control: UnitStatementId,
    pub symbols: [UnitSymbolId; 2],
    pub initializers: [UnitExpressionId; 2],
    pub block: BlockRef,
    pub initial: &'a [Option<entry::Binding>; 2],
    pub scalar: Option<[Slot; 2]>,
    pub entered: bool,
}
impl Builder<'_, '_> {
    pub(super) fn bridge_select_entry(
        &self,
        matches: impl Fn(UnitStatementId, [UnitSymbolId; 2], [UnitExpressionId; 2]) -> bool,
    ) -> Result<Option<SelectedEntry<'_>>, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        if let Some(group) = &self.control_chain_bodies {
            if self.control_bridge_plan.is_some() {
                return Err(fail());
            }
            let mut found = group.members.iter().filter(|m| {
                let e = m.evidence.scalar_entry();
                matches(m.evidence.control(), e.0, e.1)
            });
            let Some(m) = found.next() else {
                return Ok(None);
            };
            if found.next().is_some() {
                return Err(fail());
            }
            let e = m.evidence.scalar_entry();
            return Ok(Some(SelectedEntry {
                control: m.evidence.control(),
                symbols: e.0,
                initializers: e.1,
                block: m.prior_exit.ok_or_else(fail)?,
                initial: &m.initial,
                scalar: m.counter.map(|(_, s)| s),
                entered: m.lowered.is_some(),
            }));
        }
        let Some(p) = &self.control_bridge_plan else {
            return Ok(None);
        };
        let Some(e) = p.evidence.next_counter_entry() else {
            return Ok(None);
        };
        let symbols = [e.counter(), e.guard()];
        if !matches(p.evidence.next_control(), symbols, e.initializers()) {
            return Ok(None);
        }
        Ok(Some(SelectedEntry {
            control: p.evidence.next_control(),
            symbols,
            initializers: e.initializers(),
            block: p.prior.as_ref().ok_or_else(fail)?.0[3],
            initial: &p.initial,
            scalar: p.scalar,
            entered: p.next.is_some(),
        }))
    }
    pub(super) fn bridge_initial_states_mut(
        &mut self,
        control: UnitStatementId,
    ) -> Result<&mut [Option<entry::Binding>; 2], LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        if let Some(group) = &mut self.control_chain_bodies {
            if self.control_bridge_plan.is_some() {
                return Err(fail());
            }
            return group
                .members
                .iter_mut()
                .find(|m| m.evidence.control() == control)
                .map(|m| &mut m.initial)
                .ok_or_else(fail);
        }
        self.control_bridge_plan
            .as_mut()
            .filter(|p| p.evidence.next_control() == control)
            .map(|p| &mut p.initial)
            .ok_or_else(fail)
    }
}
