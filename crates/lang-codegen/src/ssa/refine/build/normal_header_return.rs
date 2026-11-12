//! A changing header is a slot equation, never a fixed P/Q producer identity.
use super::*;
mod bind_drop;
mod conditional_break;
mod conditional_return;
mod mixed;
mod parameter;
mod transport;
use lang_frontend::ownership_checking::{
    UnitOwnerCallable, UnitOwnerConditionalBreakBackedgeV1, UnitOwnerConditionalReturnBackedgeV1,
    UnitOwnerContinueBackedgeV1, UnitOwnerMixedBackedgeV1, UnitOwnerNormalBackedgeV1,
    UnitOwnerNormalBindDropV1, UnitOwnerOpenLoopObservationV1, UnitOwnerTransportEvent,
};

// Private adapter keeps the two public edge roles distinct. It never exposes a
// Continue product as the Normal wrapper and does not manufacture FE facts.
#[derive(Clone, Copy)]
enum Boundary<'a> {
    Normal(&'a UnitOwnerNormalBackedgeV1),
    BindDrop(&'a UnitOwnerNormalBindDropV1),
    Continue(&'a UnitOwnerContinueBackedgeV1),
    Mixed(&'a UnitOwnerMixedBackedgeV1),
    ConditionalBreak(&'a UnitOwnerConditionalBreakBackedgeV1),
    ConditionalReturn(&'a UnitOwnerConditionalReturnBackedgeV1),
}
macro_rules! boundary_getters {
    ($($name:ident -> $ty:ty),* $(,)?) => {$ (
        fn $name(self) -> $ty {
            match self { Self::Normal(v) => v.$name(), Self::BindDrop(v) => v.$name(), Self::Continue(v) => v.$name(), Self::Mixed(v) => v.$name(), Self::ConditionalBreak(v) => v.$name(), Self::ConditionalReturn(v) => v.$name() }
        }
    )*};
}
impl<'a> Boundary<'a> {
    boundary_getters! {
        callable -> UnitOwnerCallable,
        statement -> UnitStatementId,
        condition -> UnitExpressionId,
        body -> UnitStatementId,
        root -> UnitSymbolId,
        initial -> UnitExpressionId,
        argument -> UnitExpressionId,
        call -> UnitExpressionId,
        increment -> Option<UnitExpressionId>,
        body_events -> &'a [UnitOwnerTransportEvent],
        is_consistent -> bool,
    }
    fn return_payload(self) -> Option<UnitExpressionId> {
        match self {
            Self::BindDrop(_) => None,
            Self::Normal(v) => Some(v.return_payload()),
            Self::Continue(v) => Some(v.return_payload()),
            Self::Mixed(v) => Some(v.return_payload()),
            Self::ConditionalBreak(v) => Some(v.return_payload()),
            Self::ConditionalReturn(v) => Some(v.return_payload()),
        }
    }
    fn return_expression(self) -> Option<UnitExpressionId> {
        match self {
            Self::BindDrop(_) => None,
            Self::Normal(v) => Some(v.return_expression()),
            Self::Continue(v) => Some(v.return_expression()),
            Self::Mixed(v) => Some(v.return_expression()),
            Self::ConditionalBreak(v) => Some(v.return_expression()),
            Self::ConditionalReturn(v) => Some(v.return_expression()),
        }
    }
    fn jump(self) -> Option<UnitExpressionId> {
        match self {
            Self::Normal(_)
            | Self::BindDrop(_)
            | Self::Mixed(_)
            | Self::ConditionalBreak(_)
            | Self::ConditionalReturn(_) => None,
            Self::Continue(v) => Some(v.jump()),
        }
    }
}

// Produced only around the actual Continue terminator, after the original cleanup.
#[derive(Clone, Copy)]
pub(super) struct ContinueReceipt {
    expression: UnitExpressionId,
    source: BlockRef,
    target: BlockRef,
    statements: usize,
}

pub(super) struct Plan<'a> {
    boundary: Boundary<'a>,
    parameter: Option<(UnitSymbolId, usize)>,
    entry_read: bool,
    bind_drop: bind_drop::State,
    previous: UnitSymbolId,
    drop_index: usize,
    produced: [Option<Slot>; 2],
    root: Option<Slot>,
    detached: Option<Slot>,
    old: Option<Slot>,
    geometry: Option<[BlockRef; 5]>, // actual entry, header, body, false, exit
    increment: Option<([BlockRef; 3], [Slot; 4], usize)>,
    continued: Option<ContinueReceipt>,
    mixed: mixed::State,
    conditional_break: conditional_break::State,
    conditional_return: conditional_return::State,
    taken: bool,
    completed: bool,
}
#[derive(Clone, Copy)]
pub(super) struct Receipt {
    block: BlockRef,
    slot: Slot,
    body_return: bool,
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
        let boundary = match product {
            UnitOwnerOpenLoopObservationV1::NormalBackedge(v) => Boundary::Normal(v),
            UnitOwnerOpenLoopObservationV1::NormalBindDrop(v) => Boundary::BindDrop(v),
            UnitOwnerOpenLoopObservationV1::ContinueBackedge(v) => Boundary::Continue(v),
            UnitOwnerOpenLoopObservationV1::MixedBackedge(v) => Boundary::Mixed(v),
            UnitOwnerOpenLoopObservationV1::ConditionalBreakBackedge(v) => {
                Boundary::ConditionalBreak(v)
            }
            UnitOwnerOpenLoopObservationV1::ConditionalReturnBackedge(v) => {
                Boundary::ConditionalReturn(v)
            }
            _ => return Ok(None),
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
        let parameter = match boundary {
            Boundary::Normal(v) => v
                .input_parameter()
                .map(|symbol| {
                    let indices = inputs
                        .parameters
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| **s == symbol)
                        .map(|(i, _)| i)
                        .collect::<Vec<_>>();
                    let [index] = indices.as_slice() else {
                        return Err(fail());
                    };
                    if inputs.modes.get(*index) != Some(&ParameterMode::Value) {
                        return Err(fail());
                    }
                    Ok((symbol, *index))
                })
                .transpose()?,
            _ => None,
        };
        Ok(Some(Self {
            parameter,
            entry_read: false,
            bind_drop: bind_drop::State::default(),
            boundary,
            previous: *previous,
            drop_index: *drop_index,
            produced: [None; 2],
            root: None,
            detached: None,
            old: None,
            geometry: None,
            increment: None,
            continued: None,
            mixed: mixed::State::default(),
            conditional_break: conditional_break::State::default(),
            conditional_return: conditional_return::State::default(),
            taken: false,
            completed: false,
        }))
    }
}
impl Builder<'_, '_> {
    pub(super) fn header_continue_edge(
        &mut self,
        expression: UnitExpressionId,
        is_break: bool,
        frame: (UnitStatementId, BlockRef, bool),
        span: Span,
    ) -> Result<Option<ContinueReceipt>, LoweringError> {
        #[cfg(test)]
        if self
            .header_return_plan
            .as_ref()
            .is_some_and(|p| p.boundary.jump().is_some())
            && super::owner_consumer_test_support::take_fault(
                super::owner_consumer_test_support::OwnerFault::MissingHeaderContinueDrop,
            )
            && let Some(p) = &self.header_return_plan
        {
            self.consumed_drops.remove(&p.drop_index);
        }
        let Some(p) = &self.header_return_plan else {
            return Ok(None);
        };
        if matches!(
            p.boundary,
            Boundary::Mixed(_) | Boundary::ConditionalBreak(_)
        ) {
            return Ok(None);
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let Some(jump) = p.boundary.jump() else {
            return Err(fail());
        };
        let [_, header, body, _, _] = p.geometry.ok_or_else(fail)?;
        let source = self.block.ok_or_else(fail)?;
        let expected_source = match (p.boundary.increment(), p.increment) {
            (None, None) => body,
            (Some(_), Some(([_, success, _], _, _))) => success,
            _ => return Err(fail()),
        };
        let (statement, target, plain_while) = frame;
        if expression != jump
            || is_break
            || !plain_while
            || statement != p.boundary.statement()
            || target != header
            || source != expected_source
            || p.continued.is_some()
            || p.taken
            || p.completed
            || p.old.is_none()
            || p.root.is_none()
            || self.bindings.get(&p.boundary.root()).copied() != p.root
            || self.dead.contains(&p.boundary.root())
            || !self.consumed_drops.contains(&p.drop_index)
            || self.body.blocks[source.0].terminator.is_some()
        {
            return Err(fail());
        }
        Ok(Some(ContinueReceipt {
            expression,
            source,
            target,
            statements: self.body.blocks[source.0].statements.len(),
        }))
    }
    pub(super) fn header_continue_emitted(
        &mut self,
        receipt: Option<ContinueReceipt>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(receipt) = receipt else {
            return Ok(());
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let p = self.header_return_plan.as_mut().ok_or_else(fail)?;
        if p.boundary.jump() != Some(receipt.expression)
            || p.continued.is_some()
            || self.block.is_some()
            || self.body.blocks[receipt.source.0].statements.len() != receipt.statements
            || !matches!(self.body.blocks[receipt.source.0].terminator,
                Some((RTerm::Goto(target),_)) if target == receipt.target)
        {
            return Err(fail());
        }
        #[cfg(test)]
        {
            use super::owner_consumer_test_support::{OwnerFault, take_fault};
            if take_fault(OwnerFault::MissingHeaderContinue) {
                return Ok(());
            }
            if take_fault(OwnerFault::DuplicateHeaderContinue) {
                p.continued = Some(receipt);
            }
            if take_fault(OwnerFault::WrongHeaderContinueIdentity) {
                p.continued = Some(ContinueReceipt {
                    expression: p.boundary.return_payload().ok_or_else(fail)?,
                    ..receipt
                });
                return Ok(());
            }
            if take_fault(OwnerFault::WrongHeaderContinueTarget) {
                let [_, _, _, _, exit] = p.geometry.ok_or_else(fail)?;
                p.continued = Some(ContinueReceipt {
                    target: exit,
                    ..receipt
                });
                return Ok(());
            }
            if take_fault(OwnerFault::WrongHeaderContinueSource) {
                let [entry, _, _, _, _] = p.geometry.ok_or_else(fail)?;
                p.continued = Some(ContinueReceipt {
                    source: entry,
                    ..receipt
                });
                return Ok(());
            }
        }
        if p.continued.replace(receipt).is_some() {
            return Err(fail());
        }
        Ok(())
    }

    // Called at the actual checked-arithmetic emission, not by searching a finished CFG.
    pub(super) fn header_increment_emitted(
        &mut self,
        expression: UnitExpressionId,
        blocks: [BlockRef; 3],
        values: [Slot; 4],
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &mut self.header_return_plan else {
            return Ok(());
        };
        if p.boundary.increment() != Some(expression) {
            return Ok(());
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let [body, success, abort] = blocks;
        let [result, failed, left, right] = values;
        let Some([_, _, expected_body, _, _]) = p.geometry else {
            return Err(fail());
        };
        let statement = self.body.blocks[body.0]
            .statements
            .last()
            .ok_or_else(fail)?;
        if body != expected_body
            || self.block != Some(success)
            || p.increment.is_some()
            || statement.results != [result, failed]
            || !matches!(statement.op, ROp::CheckedArithmetic {operator: crate::ssa::model::CheckedArithmeticOperator::Add, left: l, right: r} if l == left && r == right)
            || !matches!(self.body.blocks[body.0].terminator, Some((RTerm::Branch {condition,when_true,when_false},_)) if condition == failed && when_true == abort && when_false == success)
            || !matches!(self.body.blocks[abort.0].terminator, Some((RTerm::Abort(super::super::body::RAbort::CheckedArithmetic {operator: crate::ssa::model::CheckedArithmeticOperator::Add, divisor}),_)) if divisor == right)
            || !self.body.blocks[abort.0].statements.is_empty()
        {
            return Err(fail());
        }
        p.increment = Some((
            blocks,
            values,
            self.body.blocks[body.0].statements.len() - 1,
        ));
        Ok(())
    }
    pub(super) fn header_result(
        &mut self,
        expression: UnitExpressionId,
        slot: Slot,
    ) -> Result<(), LoweringError> {
        if self.header_parameter_result(expression, slot)? {
            return Ok(());
        }
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
        self.multi_root_while(statement, condition, body, geometry, span)?;
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
        if let Some(slot) = self.header_bind_name(expression, span)? {
            return Ok(Some(slot));
        }
        if let Some(slot) = self.cr_name(expression, span)? {
            return Ok(Some(slot));
        }
        let Some(p) = &self.header_return_plan else {
            return Ok(None);
        };
        if Some(expression) != p.boundary.return_payload() {
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
        if let Some(receipt) = self.cr_return(expression, result, unit, span)? {
            return Ok(Some(receipt));
        }
        let Some(p) = &self.header_return_plan else {
            return Ok(None);
        };
        if Some(expression) != p.boundary.return_expression() {
            return Ok(None);
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let [_, _, _, _, exit] = p.geometry.ok_or_else(fail)?;
        let slot = p.root.ok_or_else(fail)?;
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
        self.validate_header_transport(span)?;
        Ok(Some(Receipt {
            block: exit,
            slot,
            body_return: false,
        }))
    }
    pub(super) fn header_return_emitted(
        &mut self,
        receipt: Option<Receipt>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(r) = receipt else {
            return Ok(());
        };
        if r.body_return {
            return self.cr_return_emitted(r, span);
        }
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
