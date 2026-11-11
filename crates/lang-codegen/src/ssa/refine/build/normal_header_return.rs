//! A changing header is a slot equation, never a fixed P/Q producer identity.
use super::*;
use lang_frontend::ownership_checking::{
    UnitOwnerCallable, UnitOwnerNormalBackedgeV1, UnitOwnerOpenLoopObservationV1,
    UnitOwnerTransportEvent,
};

pub(super) struct Plan<'a> {
    boundary: &'a UnitOwnerNormalBackedgeV1,
    previous: UnitSymbolId,
    drop_index: usize,
    produced: [Option<Slot>; 2],
    root: Option<Slot>,
    detached: Option<Slot>,
    old: Option<Slot>,
    geometry: Option<[BlockRef; 5]>, // actual entry, header, body, false, exit
    taken: bool,
    completed: bool,
}
#[derive(Clone, Copy)]
pub(super) struct Receipt {
    block: BlockRef,
    slot: Slot,
}
impl Receipt {
    #[cfg(test)]
    pub(super) fn slot(self) -> Slot {
        self.slot
    }
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
            .filter_map(|o| o.open_loop_observation_v1());
        let Some(product) = products.next() else {
            return Ok(None);
        };
        let UnitOwnerOpenLoopObservationV1::NormalBackedge(boundary) = product else {
            return Ok(None);
        };
        let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
        if products.next().is_some()
            || !boundary.is_consistent()
            || boundary.callable() != callable
            || inputs.borrow_return.is_some()
            || inputs.carrier_return.is_some()
        {
            return Err(fail());
        }
        let Some(UnitOwnerTransportEvent::Drop {
            point,
            symbol: previous,
            ..
        }) = boundary.body_events().last()
        else {
            return Err(fail());
        };
        let drops = inputs
            .owned
            .drops()
            .iter()
            .enumerate()
            .filter(|(_, d)| d.target() == UnitDropTarget::Named(*previous))
            .collect::<Vec<_>>();
        let [(drop_index, drop)] = drops.as_slice() else {
            return Err(fail());
        };
        let typed = inputs
            .typed
            .ownership_primitive(boundary.call())
            .ok_or_else(fail)?;
        let owned = inputs
            .owned
            .ownership_primitive(boundary.call())
            .ok_or_else(fail)?;
        if drop.point() != *point
            || inputs
                .owned
                .drops()
                .iter()
                .any(|d| d.target() == UnitDropTarget::Named(boundary.root()))
            || typed.kind() != lang_frontend::type_checking::OwnershipPrimitiveKind::Replace
            || typed.operands().len() != 2
            || typed.operands()[1] != boundary.argument()
            || *owned.descriptor() != typed
            || owned.places().len() != 1
            || !owned.places()[0].is_root()
            || owned.places()[0].root() != boundary.root()
            || inputs.typed.types().get(typed.value_type())
                != Some(&UnitTypeKind::Builtin(
                    lang_frontend::type_checking::BuiltinType::String,
                ))
        {
            return Err(fail());
        }
        Ok(Some(Self {
            boundary,
            previous: *previous,
            drop_index: *drop_index,
            produced: [None; 2],
            root: None,
            detached: None,
            old: None,
            geometry: None,
            taken: false,
            completed: false,
        }))
    }
}
impl Builder<'_, '_> {
    pub(super) fn header_result(
        &mut self,
        expression: UnitExpressionId,
        slot: Slot,
    ) -> Result<(), LoweringError> {
        if let Some(p) = &mut self.header_return_plan {
            for (i, id) in [p.boundary.initial(), p.boundary.argument()]
                .iter()
                .enumerate()
            {
                if *id == expression && p.produced[i].replace(slot).is_some() {
                    return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
                }
            }
        }
        Ok(())
    }
    pub(super) fn header_binding(
        &self,
        initializer: UnitExpressionId,
        symbol: UnitSymbolId,
        value: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &self.header_return_plan else {
            return Ok(());
        };
        let invalid = if symbol == p.boundary.root() {
            initializer != p.boundary.initial() || p.produced[0] != Some(value) || p.root.is_some()
        } else if symbol == p.previous {
            initializer != p.boundary.call() || p.old != Some(value) || p.detached.is_some()
        } else {
            false
        };
        if invalid {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(())
    }
    pub(super) fn header_binding_emitted(&mut self, symbol: UnitSymbolId, slot: Slot) {
        if let Some(p) = &mut self.header_return_plan {
            if symbol == p.boundary.root() {
                p.root = Some(slot);
            }
            if symbol == p.previous {
                p.detached = Some(slot);
            }
        }
    }
    pub(super) fn header_while(
        &mut self,
        statement: UnitStatementId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        geometry: [BlockRef; 4],
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &mut self.header_return_plan else {
            return Ok(());
        };
        if p.boundary.statement() != statement {
            return Ok(());
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let entry = self.block.ok_or_else(fail)?;
        if condition != p.boundary.condition()
            || body != p.boundary.body()
            || p.root.is_none()
            || self.bindings.get(&p.boundary.root()).copied() != p.root
            || p.geometry
                .replace([entry, geometry[0], geometry[1], geometry[2], geometry[3]])
                .is_some()
        {
            return Err(fail());
        }
        Ok(())
    }
    pub(super) fn header_replace(
        &self,
        call: UnitExpressionId,
        root: Slot,
        replacement: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &self.header_return_plan else {
            return Ok(());
        };
        if call != p.boundary.call() {
            return Ok(());
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let [_, header, body, finished, _] = p.geometry.ok_or_else(fail)?;
        if self.block != Some(body)
            || p.root != Some(root)
            || p.produced[1] != Some(replacement)
            || p.old.is_some()
            || self.dead.contains(&p.boundary.root())
            || !matches!(self.body.blocks[header.0].terminator, Some((RTerm::Branch { when_true, when_false, .. }, _)) if when_true == body && when_false == finished)
        {
            return Err(fail());
        }
        Ok(())
    }
    pub(super) fn header_replace_emitted(
        &mut self,
        call: UnitExpressionId,
        root: Slot,
        replacement: Slot,
        old: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &mut self.header_return_plan else {
            return Ok(());
        };
        if call != p.boundary.call() {
            return Ok(());
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let block = self.block.ok_or_else(fail)?;
        let s = self.body.blocks[block.0]
            .statements
            .last()
            .ok_or_else(fail)?;
        if p.root != Some(root)
            || p.produced[1] != Some(replacement)
            || old == root
            || old == replacement
            || s.results != [root, old]
            || !matches!(s.op, ROp::RootReplace { owner, replacement: actual, .. } if owner == root && actual == replacement)
            || p.old.replace(old).is_some()
        {
            return Err(fail());
        }
        Ok(())
    }
    pub(super) fn header_name(
        &mut self,
        expression: UnitExpressionId,
        span: Span,
    ) -> Result<Option<Slot>, LoweringError> {
        self.sync_dead();
        let Some(p) = &self.header_return_plan else {
            return Ok(None);
        };
        if expression != p.boundary.return_payload() {
            return Ok(None);
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let [_, _, _, _, exit] = p.geometry.ok_or_else(fail)?;
        let slot = p.root.ok_or_else(fail)?;
        if self.block != Some(exit)
            || p.taken
            || p.completed
            || p.old.is_none()
            || self.reference(span)? != p.boundary.root()
            || self.bindings.get(&p.boundary.root()) != Some(&slot)
            || self.dead.contains(&p.boundary.root())
        {
            return Err(fail());
        }
        #[cfg(test)]
        if super::owner_consumer_test_support::take_fault(
            super::owner_consumer_test_support::OwnerFault::ExtraHeaderAlias,
        ) {
            let [entry, _, _, _, _] = p.geometry.ok_or_else(fail)?;
            let alias = self.body.blocks[entry.0]
                .statements
                .iter()
                .find(|s| s.results == [slot] && matches!(s.op, ROp::Alias { .. }))
                .ok_or_else(fail)?
                .clone();
            self.body.blocks[exit.0].statements.push(alias);
        }
        self.header_return_plan.as_mut().ok_or_else(fail)?.taken = true;
        Ok(Some(slot))
    }
    pub(super) fn header_return(
        &self,
        expression: UnitExpressionId,
        result: Option<Slot>,
        unit: bool,
        span: Span,
    ) -> Result<Option<Receipt>, LoweringError> {
        let Some(p) = &self.header_return_plan else {
            return Ok(None);
        };
        if expression != p.boundary.return_expression() {
            return Ok(None);
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let [entry, header, body, finished, exit] = p.geometry.ok_or_else(fail)?;
        let slot = p.root.ok_or_else(fail)?;
        let old = p.old.ok_or_else(fail)?;
        let detached = p.detached.ok_or_else(fail)?;
        let initial = p.produced[0].ok_or_else(fail)?;
        let replacement = p.produced[1].ok_or_else(fail)?;
        if unit
            || result != Some(slot)
            || !p.taken
            || p.completed
            || self.block != Some(exit)
            || !self.dead.contains(&p.boundary.root())
            || !self.consumed_drops.contains(&p.drop_index)
        {
            return Err(fail());
        }
        let incoming = |target| {
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
        // This finite CFG equation closes under any number of traversals: each body
        // replaces the current slot with fresh Q, drops its old value once, and goes
        // back to the same header. SSA subsequently versions that slot into phi inputs.
        if incoming(header) != [entry, body]
            || incoming(body) != [header]
            || incoming(finished) != [header]
            || incoming(exit) != [finished]
            || !matches!(self.body.blocks[entry.0].terminator, Some((RTerm::Goto(b),_)) if b == header)
            || !matches!(self.body.blocks[body.0].terminator, Some((RTerm::Goto(b),_)) if b == header)
            || !matches!(self.body.blocks[finished.0].terminator, Some((RTerm::Goto(b),_)) if b == exit)
            || !matches!(self.body.blocks[header.0].terminator, Some((RTerm::Branch {when_true,when_false,..},_)) if when_true == body && when_false == finished)
        {
            return Err(fail());
        }
        let entry_statements = &self.body.blocks[entry.0].statements;
        let statements = &self.body.blocks[body.0].statements;
        if !entry_statements.iter().any(|s| {
            s.results == [slot] && matches!(s.op, ROp::Alias {source} if source == initial)
        }) {
            return Err(fail());
        }
        let replaces = statements
            .iter()
            .enumerate()
            .filter(|(_, s)| matches!(s.op, ROp::RootReplace { .. }))
            .collect::<Vec<_>>();
        let [(replace_index, replace)] = replaces.as_slice() else {
            return Err(fail());
        };
        let aliases = statements
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.results == [detached] && matches!(s.op, ROp::Alias {source} if source == old)
            })
            .collect::<Vec<_>>();
        let [(alias_index, _)] = aliases.as_slice() else {
            return Err(fail());
        };
        let drops = statements
            .iter()
            .enumerate()
            .filter(|(_, s)| matches!(s.op, ROp::Drop { .. }))
            .collect::<Vec<_>>();
        let [(drop_index, drop)] = drops.as_slice() else {
            return Err(fail());
        };
        let valid_replace_drop = replace.results == [slot, old]
            && matches!(replace.op, ROp::RootReplace {owner,replacement: q,..} if owner == slot && q == replacement)
            && matches!(drop.op, ROp::Drop {owner} if owner == detached)
            && replace_index < alias_index
            && alias_index < drop_index;
        let outside_drop = self.body.blocks.iter().enumerate().filter(|(i,_)| *i != body.0)
            .any(|(_,b)| b.statements.iter().any(|s| matches!(s.op, ROp::Drop {owner} if owner == slot || owner == old || owner == detached)));
        if !valid_replace_drop || detached == slot || outside_drop {
            return Err(fail());
        }
        // Exhaustive definition census: extra aliases/redefinitions cannot silently
        // change H, P, Q or the detached value between the observed hooks.
        let definitions = |target| {
            self.body
                .blocks
                .iter()
                .enumerate()
                .flat_map(|(block, b)| {
                    b.statements
                        .iter()
                        .enumerate()
                        .filter_map(move |(index, s)| {
                            s.results
                                .contains(&target)
                                .then_some((BlockRef(block), index))
                        })
                })
                .collect::<Vec<_>>()
        };
        let initial_defs = definitions(initial);
        let q_defs = definitions(replacement);
        let root_defs = definitions(slot);
        let [(p_block, p_index)] = initial_defs.as_slice() else {
            return Err(fail());
        };
        let [(q_block, q_index)] = q_defs.as_slice() else {
            return Err(fail());
        };
        let entry_alias = entry_statements
            .iter()
            .position(|s| {
                s.results == [slot] && matches!(s.op, ROp::Alias {source} if source == initial)
            })
            .ok_or_else(fail)?;
        if *p_block != entry
            || *q_block != body
            || *p_index >= entry_alias
            || q_index >= replace_index
            || !matches!(entry_statements[*p_index].op, ROp::StringLiteral { .. })
            || !matches!(statements[*q_index].op, ROp::StringLiteral { .. })
            || root_defs != [(entry, entry_alias), (body, *replace_index)]
            || definitions(old) != [(body, *replace_index)]
            || definitions(detached) != [(body, *alias_index)]
            || [slot, initial, replacement, old, detached]
                .iter()
                .enumerate()
                .any(|(i, value)| [slot, initial, replacement, old, detached][..i].contains(value))
        {
            return Err(fail());
        }
        Ok(Some(Receipt { block: exit, slot }))
    }
    pub(super) fn header_return_emitted(
        &mut self,
        receipt: Option<Receipt>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(r) = receipt else {
            return Ok(());
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        if !matches!(self.body.blocks[r.block.0].terminator, Some((RTerm::Return(Some(slot)),_)) if slot == r.slot)
        {
            return Err(fail());
        }
        let p = self.header_return_plan.as_mut().ok_or_else(fail)?;
        if std::mem::replace(&mut p.completed, true) {
            return Err(fail());
        }
        Ok(())
    }
    pub(super) fn finish_header_return(&self) -> Result<(), LoweringError> {
        if self
            .header_return_plan
            .as_ref()
            .is_some_and(|p| !p.taken || !p.completed)
        {
            return Err(error(LoweringErrorKind::MissingFact, self.inputs.span));
        }
        Ok(())
    }
}
