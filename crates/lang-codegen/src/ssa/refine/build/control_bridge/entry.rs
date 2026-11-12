//! Required original scalar bindings keyed by control, not an owner token source.
use super::*;
use crate::ssa::model::ScalarConstant;

pub(super) struct Binding {
    block: BlockRef,
    source: Slot,
    target: Slot,
    instruction: usize,
    span: Span,
    literal: Option<(usize, i128)>,
    read_loan: Option<(usize, Slot)>,
}
impl Binding {
    pub(super) fn block(&self) -> BlockRef {
        self.block
    }
}
pub(in super::super) struct Permit {
    index: usize,
    control: UnitStatementId,
    block: BlockRef,
    source: Slot,
    instruction: usize,
    span: Span,
    literal: Option<(usize, i128)>,
    read_loan: Option<(usize, Slot)>,
}
impl Builder<'_, '_> {
    pub(in super::super) fn bridge_entry_binding(
        &self,
        initializer: UnitExpressionId,
        symbol: UnitSymbolId,
        source: Slot,
        span: Span,
    ) -> Result<Option<Permit>, LoweringError> {
        let Some(selected) = self.bridge_select_entry(|_, symbols, initializers| {
            symbols.contains(&symbol) || initializers.contains(&initializer)
        })?
        else {
            return Ok(None);
        };
        #[cfg(test)]
        if self
            .control_chain_bodies
            .as_ref()
            .is_some_and(|g| g.is_leading(selected.control))
            && owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::ChainLeadingEntryMissingBinding,
            )
        {
            owner_consumer_test_support::record_action("control-chain-leading-entry-fault-hit", 1);
            return Ok(None);
        }
        let symbols = selected.symbols;
        let initializers = selected.initializers;
        let Some(index) = (0..2).find(|&i| symbols[i] == symbol || initializers[i] == initializer)
        else {
            return Ok(None);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let block = selected.block;
        if symbols[index] != symbol
            || initializers[index] != initializer
            || selected.entered
            || self.block != Some(block)
            || selected.initial[index].is_some()
            || selected.initial[..index].iter().any(Option::is_none)
        {
            return Err(fail());
        }
        let mut literal = None;
        let mut read_loan = None;
        if index == 0 {
            match self.node(initializer.expression())?.0 {
                lang_frontend::parser::Expression::Name => {
                    let symbol = self.pending_symbol(initializer).ok_or_else(fail)?;
                    if let Some(&loan) = self.borrowed.get(&symbol) {
                        let position = self.body.blocks[block.0].statements.iter().position(|s|
                            s.span == self.span(initializer.expression()) && s.results == [source]
                            && matches!(s.op, ROp::ReadLoan { loan: actual } if actual == loan)
                        ).ok_or_else(fail)?;
                        if self.multi_root_definitions(source) != 1 {
                            return Err(fail());
                        }
                        read_loan = Some((position, loan));
                    } else if self.bindings.get(&symbol) != Some(&source) {
                        return Err(fail());
                    }
                }
                lang_frontend::parser::Expression::Literal(
                    lang_frontend::parser::LiteralKind::Integer(kind),
                ) => {
                    let origin_span = self.span(initializer.expression());
                    let text = self.inputs.sources.slice(origin_span).map_err(|_| fail())?;
                    let expected =
                        lang_frontend::type_checking::integer_literal_magnitude(text, kind)
                            .and_then(|n| i128::try_from(n).ok())
                            .ok_or_else(fail)?;
                    let position = self.body.blocks[block.0].statements.iter().position(|s|
                        s.span == origin_span && s.results == [source]
                        && matches!(s.op, ROp::Constant(ScalarConstant::Integer(value)) if value == expected)
                    ).ok_or_else(fail)?;
                    if self.multi_root_definitions(source) != 1 {
                        return Err(fail());
                    }
                    literal = Some((position, expected));
                }
                _ => return Err(fail()),
            }
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::MissingBridgeEntryBinding,
        ) {
            if self.control_chain_bodies.is_some() {
                owner_consumer_test_support::record_action("control-chain-entry-fault-hit", 1);
            }
            return Ok(None);
        }
        #[cfg(test)]
        if self
            .control_chain_bodies
            .as_ref()
            .is_some_and(|g| g.is_last(selected.control))
            && owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::ChainLastEntryMissingBinding,
            )
        {
            owner_consumer_test_support::record_action("control-chain-entry-fault-hit", 1);
            return Ok(None);
        }
        Ok(Some(Permit {
            index,
            control: selected.control,
            block,
            source,
            instruction: self.body.blocks[block.0].statements.len(),
            span,
            literal,
            read_loan,
        }))
    }
    pub(in super::super) fn bridge_entry_emitted(
        &mut self,
        permit: Option<Permit>,
        target: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(permit) = permit else { return Ok(()) };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        #[cfg(test)]
        let permit = {
            let mut p = permit;
            if owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::BridgeEntryWrongSource,
            ) {
                if self.control_chain_bodies.is_some() {
                    owner_consumer_test_support::record_action("control-chain-entry-fault-hit", 1);
                }
                p.source = target;
            }
            p
        };
        let block = &self.body.blocks[permit.block.0];
        let statement = block.statements.get(permit.instruction).ok_or_else(fail)?;
        if self.block != Some(permit.block)
            || block.statements.len() != permit.instruction + 1
            || statement.span != permit.span
            || target == permit.source
            || statement.results != [target]
            || !matches!(statement.op, ROp::Alias { source } if source == permit.source)
        {
            return Err(fail());
        }
        let selected = self
            .bridge_select_entry(|control, _, _| control == permit.control)?
            .ok_or_else(fail)?;
        if selected.block != permit.block
            || selected.entered
            || selected
                .initial
                .get(permit.index)
                .is_none_or(Option::is_some)
            || selected.initial[..permit.index].iter().any(Option::is_none)
        {
            return Err(fail());
        }
        if permit.index == 1 {
            let counter = selected.initial[0].as_ref().ok_or_else(fail)?;
            self.counter_guard_comparison(
                permit.block,
                counter.target,
                target,
                selected.initializers[1],
                None,
            )?;
            let compare = block
                .statements
                .iter()
                .position(|s| s.results == [permit.source])
                .ok_or_else(fail)?;
            if counter.instruction >= compare {
                return Err(fail());
            }
        }
        let state = self
            .bridge_initial_states_mut(permit.control)?
            .get_mut(permit.index)
            .ok_or_else(fail)?;
        if state.is_some() {
            return Err(fail());
        }
        *state = Some(Binding {
            block: permit.block,
            source: permit.source,
            target,
            instruction: permit.instruction,
            span: permit.span,
            literal: permit.literal,
            read_loan: permit.read_loan,
        });
        #[cfg(test)]
        owner_consumer_test_support::record_action("bridge-entry-binding-consumed", permit.index);
        Ok(())
    }
    pub(super) fn check_bridge_entry(&self) -> Result<(), LoweringError> {
        let Some(plan) = &self.control_bridge_plan else {
            return Ok(());
        };
        if plan.evidence.next_counter_entry().is_none() {
            return Ok(());
        }
        self.check_control_entry(plan.evidence.next_control())
    }
    pub(super) fn check_control_entry(
        &self,
        control: UnitStatementId,
    ) -> Result<(), LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let selected = self
            .bridge_select_entry(|candidate, _, _| candidate == control)?
            .ok_or_else(fail)?;
        let block = selected.block;
        let [Some(counter), Some(guard)] = selected.initial else {
            return Err(fail());
        };
        for item in [counter, guard] {
            let statement = self.body.blocks[block.0]
                .statements
                .get(item.instruction)
                .ok_or_else(fail)?;
            if item.block != block
                || statement.span != item.span
                || statement.results != [item.target]
                || !matches!(statement.op, ROp::Alias { source } if source == item.source)
                || self.counter_alias(block, item.target, item.source, None)? != item.instruction
            {
                return Err(fail());
            }
        }
        if let Some((position, loan)) = counter.read_loan {
            let producer = self.body.blocks[block.0]
                .statements
                .get(position)
                .ok_or_else(fail)?;
            if position >= counter.instruction
                || producer.span != self.span(selected.initializers[0].expression())
                || producer.results != [counter.source]
                || self.multi_root_definitions(counter.source) != 1
                || !matches!(producer.op, ROp::ReadLoan { loan: actual } if actual == loan)
            {
                return Err(fail());
            }
        }
        if let Some((position, expected)) = counter.literal {
            let producer = self.body.blocks[block.0]
                .statements
                .get(position)
                .ok_or_else(fail)?;
            if position >= counter.instruction
                || producer.span != self.span(selected.initializers[0].expression())
                || producer.results != [counter.source]
                || self.multi_root_definitions(counter.source) != 1
                || !matches!(producer.op, ROp::Constant(ScalarConstant::Integer(value)) if value == expected)
            {
                return Err(fail());
            }
        }
        self.counter_guard_comparison(
            block,
            counter.target,
            guard.target,
            selected.initializers[1],
            None,
        )?;
        let compare = self.body.blocks[block.0]
            .statements
            .iter()
            .position(|s| s.results == [guard.source])
            .ok_or_else(fail)?;
        if counter.instruction >= compare
            || compare >= guard.instruction
            || selected
                .scalar
                .is_some_and(|actual| actual != [counter.target, guard.target])
        {
            return Err(fail());
        }
        Ok(())
    }
}
