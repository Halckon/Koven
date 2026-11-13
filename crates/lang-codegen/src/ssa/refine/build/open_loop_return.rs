//! Two terminal owner paths consume the unified open-loop contract, never a Break join.
use super::*;
use lang_frontend::ownership_checking::{
    UnitOwnerCallable, UnitOwnerOpenLoopObservationV1, UnitOwnerOpenLoopReturnV1, UnitOwnerOrigin,
};

pub(super) struct Plan<'a> {
    boundary: &'a UnitOwnerOpenLoopReturnV1,
    produced: [Option<Slot>; 2],
    parameter: Option<(UnitSymbolId, usize)>,
    entry_read: bool,
    root: Option<Slot>,
    previous: Option<Slot>,
    old: Option<Slot>,
    geometry: Option<[BlockRef; 4]>, // header, body, condition-false, exit
    drop_index: usize,
    taken: [bool; 2],
    completed: [bool; 2],
}
#[derive(Clone, Copy)]
pub(super) struct Receipt {
    role: usize,
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
        let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
        if products.next().is_some() {
            return Err(fail());
        }
        let UnitOwnerOpenLoopObservationV1::ReturnDelivered(boundary) = product else {
            return Ok(None);
        };
        if !boundary.is_consistent()
            || boundary.callable() != callable
            || inputs.borrow_return.is_some()
            || inputs.carrier_return.is_some()
        {
            return Err(fail());
        }
        let parameter = match boundary
            .input_producers()
            .definition(boundary.entry().owner())
            .ok_or_else(fail)?
            .origin()
        {
            UnitOwnerOrigin::String(expression) if expression == boundary.initial() => None,
            UnitOwnerOrigin::Parameter(symbol) => {
                if inputs.parameters.len() != 2 || inputs.modes.len() != 2 {
                    return Err(fail());
                }
                let index = inputs
                    .parameters
                    .iter()
                    .position(|p| *p == symbol)
                    .ok_or_else(fail)?;
                for (i, parameter) in inputs.parameters.iter().enumerate() {
                    let (mode, builtin) = if i == index {
                        (
                            ParameterMode::Value,
                            lang_frontend::type_checking::BuiltinType::String,
                        )
                    } else {
                        (
                            ParameterMode::Borrow,
                            lang_frontend::type_checking::BuiltinType::Boolean,
                        )
                    };
                    let ty = inputs.typed.symbol_type(*parameter).ok_or_else(fail)?;
                    if inputs.modes[i] != mode
                        || inputs.typed.types().get(ty) != Some(&UnitTypeKind::Builtin(builtin))
                    {
                        return Err(fail());
                    }
                }
                Some((symbol, index))
            }
            _ => return Err(fail()),
        };
        let typed = inputs
            .typed
            .ownership_primitive(boundary.call())
            .ok_or_else(fail)?;
        let owned = inputs
            .owned
            .ownership_primitive(boundary.call())
            .ok_or_else(fail)?;
        if typed.kind() != lang_frontend::type_checking::OwnershipPrimitiveKind::Replace
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
        let drops = inputs
            .owned
            .drops()
            .iter()
            .enumerate()
            .filter(|(_, d)| d.target() == UnitDropTarget::Named(boundary.previous()))
            .collect::<Vec<_>>();
        let [(index, drop)] = drops.as_slice() else {
            return Err(fail());
        };
        if drop.point() != UnitDropPoint::AfterStatement(boundary.previous_statement())
            || inputs
                .owned
                .drops()
                .iter()
                .any(|d| d.target() == UnitDropTarget::Named(boundary.root()))
        {
            return Err(fail());
        }
        Ok(Some(Self {
            boundary,
            produced: [None; 2],
            parameter,
            entry_read: false,
            root: None,
            previous: None,
            old: None,
            geometry: None,
            drop_index: *index,
            taken: [false; 2],
            completed: [false; 2],
        }))
    }
    fn role(&self, expression: UnitExpressionId, payload: bool) -> Option<usize> {
        [
            self.boundary.body_return(),
            self.boundary.condition_false_return(),
        ]
        .iter()
        .position(|e| {
            if payload {
                e.payload() == expression
            } else {
                e.return_expression() == expression
            }
        })
    }
}
impl Builder<'_, '_> {
    // Called only after the original Value parameter slot and binding were installed.
    pub(super) fn open_loop_parameter_bound(
        &mut self,
        symbol: UnitSymbolId,
        mode: ParameterMode,
        slot: Slot,
    ) -> Result<(), LoweringError> {
        let Some(plan) = &mut self.open_return_plan else {
            return Ok(());
        };
        let Some((parameter, index)) = plan.parameter else {
            return Ok(());
        };
        if symbol != parameter {
            return Ok(());
        }
        if mode != ParameterMode::Value
            || self.block != Some(BlockRef(0))
            || self.body.parameters.get(index) != Some(&slot)
            || self.bindings.get(&parameter) != Some(&slot)
            || !matches!(self.body.slots[slot.0], EntityType::Value(_))
            || plan.root.is_some()
            || plan.produced[0].replace(slot).is_some()
        {
            return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
        }
        #[cfg(test)]
        super::owner_consumer_test_support::record_action("parameter_entry", slot.0);
        Ok(())
    }

    pub(super) fn open_loop_result(
        &mut self,
        expression: UnitExpressionId,
        slot: Slot,
    ) -> Result<(), LoweringError> {
        let parameter = self.open_return_plan.as_ref().and_then(|plan| {
            (expression == plan.boundary.initial())
                .then_some(plan.parameter)
                .flatten()
        });
        if let Some((parameter, _)) = parameter {
            self.sync_dead();
            let named = self.pending_symbol(expression);
            let plan = self
                .open_return_plan
                .as_mut()
                .expect("active parameter plan");
            if named != Some(parameter)
                || self.block != Some(BlockRef(0))
                || self.bindings.get(&parameter) != Some(&slot)
                || self.dead.contains(&parameter)
                || plan.produced[0] != Some(slot)
                || plan.root.is_some()
                || std::mem::replace(&mut plan.entry_read, true)
            {
                return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
            }
            // A Name result transports P. It never defines a replacement producer.
            #[cfg(test)]
            super::owner_consumer_test_support::record_action("parameter_move", slot.0);
            return Ok(());
        }
        if let Some(plan) = &mut self.open_return_plan {
            for (role, producer) in [plan.boundary.initial(), plan.boundary.argument()]
                .iter()
                .enumerate()
            {
                if *producer == expression && plan.produced[role].replace(slot).is_some() {
                    return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
                }
            }
        }
        Ok(())
    }
    pub(super) fn open_loop_binding(
        &self,
        initializer: UnitExpressionId,
        symbol: UnitSymbolId,
        value: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(plan) = &self.open_return_plan else {
            return Ok(());
        };
        let invalid = if symbol == plan.boundary.root() {
            initializer != plan.boundary.initial()
                || plan.produced[0] != Some(value)
                || plan.root.is_some()
                || plan.parameter.is_some_and(|(parameter, _)| {
                    !plan.entry_read || !self.dead.contains(&parameter)
                })
        } else if symbol == plan.boundary.previous() {
            initializer != plan.boundary.call()
                || plan.old != Some(value)
                || plan.previous.is_some()
        } else {
            false
        };
        if invalid {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(())
    }
    pub(super) fn open_loop_binding_emitted(&mut self, symbol: UnitSymbolId, binding: Slot) {
        if let Some(plan) = &mut self.open_return_plan {
            if symbol == plan.boundary.root() {
                plan.root = Some(binding);
                #[cfg(test)]
                if plan.parameter.is_some() {
                    super::owner_consumer_test_support::record_action("parameter_root", binding.0);
                }
            }
            if symbol == plan.boundary.previous() {
                plan.previous = Some(binding);
            }
        }
    }
    pub(super) fn open_loop_while(
        &mut self,
        statement: UnitStatementId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        geometry: [BlockRef; 4],
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(plan) = &mut self.open_return_plan else {
            return Ok(());
        };
        if plan.boundary.statement() != statement {
            return Ok(());
        }
        if plan.boundary.condition() != condition
            || plan.boundary.body() != body
            || plan.root.is_none()
            || self.bindings.get(&plan.boundary.root()).copied() != plan.root
            || plan.geometry.replace(geometry).is_some()
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(())
    }
    pub(super) fn open_loop_replace(
        &self,
        call: UnitExpressionId,
        root: Slot,
        replacement: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(plan) = &self.open_return_plan else {
            return Ok(());
        };
        if call != plan.boundary.call() {
            return Ok(());
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let [header, body, finished, _] = plan.geometry.ok_or_else(fail)?;
        if self.block != Some(body)
            || plan.root != Some(root)
            || plan.produced[1] != Some(replacement)
            || plan.old.is_some()
            || self.dead.contains(&plan.boundary.root())
            || !matches!(self.body.blocks[header.0].terminator, Some((RTerm::Branch { when_true, when_false, .. }, _)) if when_true == body && when_false == finished)
        {
            return Err(fail());
        }
        Ok(())
    }
    pub(super) fn open_loop_replace_emitted(
        &mut self,
        call: UnitExpressionId,
        root: Slot,
        replacement: Slot,
        old: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(plan) = &mut self.open_return_plan else {
            return Ok(());
        };
        if call != plan.boundary.call() {
            return Ok(());
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let block = self.block.ok_or_else(fail)?;
        let stmt = self.body.blocks[block.0]
            .statements
            .last()
            .ok_or_else(fail)?;
        if plan.root != Some(root)
            || plan.produced[1] != Some(replacement)
            || old == root
            || old == replacement
            || stmt.results != [root, old]
            || !matches!(stmt.op, ROp::RootReplace { owner, replacement: actual, .. } if owner == root && actual == replacement)
            || plan.old.replace(old).is_some()
        {
            return Err(fail());
        }
        Ok(())
    }
    pub(super) fn open_loop_name(
        &mut self,
        expression: UnitExpressionId,
        span: Span,
    ) -> Result<Option<Slot>, LoweringError> {
        self.sync_dead();
        let Some(plan) = &self.open_return_plan else {
            return Ok(None);
        };
        let Some(role) = plan.role(expression, true) else {
            return Ok(None);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let [_, body, _, exit] = plan.geometry.ok_or_else(fail)?;
        let root = plan.root.ok_or_else(fail)?;
        if self.block != Some(if role == 0 { body } else { exit })
            || plan.taken[role]
            || plan.completed[role]
            || self.reference(span)? != plan.boundary.root()
            || self.bindings.get(&plan.boundary.root()) != Some(&root)
            || self.dead.contains(&plan.boundary.root())
            || plan.old.is_none()
        {
            return Err(fail());
        }
        self.open_return_plan.as_mut().ok_or_else(fail)?.taken[role] = true;
        Ok(Some(root))
    }
    pub(super) fn open_loop_return(
        &self,
        expression: UnitExpressionId,
        result: Option<Slot>,
        unit: bool,
        span: Span,
    ) -> Result<Option<Receipt>, LoweringError> {
        let Some(plan) = &self.open_return_plan else {
            return Ok(None);
        };
        let Some(role) = plan.role(expression, false) else {
            return Ok(None);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let [_, body, finished, exit] = plan.geometry.ok_or_else(fail)?;
        let slot = plan.root.ok_or_else(fail)?;
        let block = self.block.ok_or_else(fail)?;
        if unit
            || result != Some(slot)
            || !plan.taken[role]
            || plan.completed[role]
            || block != if role == 0 { body } else { exit }
            || !self.dead.contains(&plan.boundary.root())
            || !self.consumed_drops.contains(&plan.drop_index)
        {
            return Err(fail());
        }
        let previous = plan.previous.ok_or_else(fail)?;
        let old = plan.old.ok_or_else(fail)?;
        let statements = &self.body.blocks[body.0].statements;
        let old_alias = statements.iter().any(|s| {
            s.results == [previous] && matches!(s.op, ROp::Alias { source } if source == old)
        });
        let drops = statements
            .iter()
            .filter_map(|s| match s.op {
                ROp::Drop { owner } => Some(owner),
                _ => None,
            })
            .collect::<Vec<_>>();
        if !old_alias || drops != [previous] || previous == slot {
            return Err(fail());
        }
        if role == 1 {
            let incoming = self
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
                .collect::<Vec<_>>();
            if !plan.completed[0]
                || incoming != [finished]
                || !matches!(self.body.blocks[finished.0].terminator, Some((RTerm::Goto(target), _)) if target == exit)
                || !matches!(self.body.blocks[body.0].terminator, Some((RTerm::Return(Some(actual)), _)) if actual == slot)
            {
                return Err(fail());
            }
        }
        Ok(Some(Receipt { role, block, slot }))
    }
    pub(super) fn open_loop_return_emitted(
        &mut self,
        receipt: Option<Receipt>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(receipt) = receipt else {
            return Ok(());
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        if !matches!(self.body.blocks[receipt.block.0].terminator, Some((RTerm::Return(Some(actual)), _)) if actual == receipt.slot)
        {
            return Err(fail());
        }
        let plan = self.open_return_plan.as_mut().ok_or_else(fail)?;
        if std::mem::replace(&mut plan.completed[receipt.role], true) {
            return Err(fail());
        }
        Ok(())
    }
    pub(super) fn finish_open_loop_return(&self) -> Result<(), LoweringError> {
        if let Some(plan) = &self.open_return_plan
            && (plan.completed != [true; 2] || plan.taken != [true; 2])
        {
            return Err(error(LoweringErrorKind::MissingFact, self.inputs.span));
        }
        Ok(())
    }
}
