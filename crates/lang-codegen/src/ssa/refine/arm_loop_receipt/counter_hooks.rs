//! Mechanical capture from real binding, checked-expression and alias emission sites.
use super::*;
use lang_frontend::{name_resolution::UnitSymbolId, parser::BinaryOperator};
impl Plans {
    pub(in crate::ssa::refine) fn required(
        inputs: &RefineInputs<'_>,
    ) -> Vec<UnitLoopBranchNoCarryFact> {
        crate::ssa::borrow_result_support::branch_no_carry::pairs(inputs.owned)
            .filter(|p| {
                Some(p.arms()[0].callable().item()) == inputs.function_item
                    && p.branch().source_unit() == inputs.source_unit
            })
            .cloned()
            .collect()
    }
    pub(in crate::ssa::refine) fn required_counters(
        inputs: &RefineInputs<'_>,
    ) -> Vec<UnitLoopCounterBranchNoCarryFact> {
        inputs
            .owned
            .no_carry_counter_branches()
            .iter()
            .filter(|c| {
                Some(c.pair().arms()[0].callable().item()) == inputs.function_item
                    && c.pair().branch().source_unit() == inputs.source_unit
            })
            .cloned()
            .collect()
    }

    pub(super) fn counter_spec(
        inputs: &RefineInputs<'_>,
        fact: &UnitLoopCounterBranchNoCarryFact,
    ) -> Result<counter::CounterSpec, LoweringError> {
        let fail = || invalid(inputs.span);
        if !inputs.owned.owns_no_carry_counter_branch(fact) {
            return Err(fail());
        }
        let node = |id: UnitExpressionId| {
            inputs
                .parsed
                .ast()
                .expressions()
                .get(id.expression())
                .map_err(|_| fail())
        };
        let condition = node(fact.condition())?;
        let Expression::Binary {
            operator: BinaryOperator::Greater,
            left,
            right: zero,
            ..
        } = condition.payload()
        else {
            return Err(fail());
        };
        let decrement = node(fact.decrement())?;
        let Expression::Binary {
            operator: BinaryOperator::Subtract,
            left: read,
            right: one,
            ..
        } = decrement.payload()
        else {
            return Err(fail());
        };
        let reference = |id| {
            let n = inputs
                .parsed
                .ast()
                .expressions()
                .get(id)
                .map_err(|_| fail())?;
            if !matches!(n.payload(), Expression::Name) {
                return Err(fail());
            }
            inputs
                .references
                .get(&(n.span().start(), n.span().end()))
                .copied()
                .ok_or_else(fail)
        };
        if reference(*left)? != fact.counter()
            || reference(*read)? != fact.counter()
            || reference(fact.initializer().expression())? != fact.rounds()
            || fact.decrement_operand().expression() != *read
        {
            return Err(fail());
        }
        Ok(counter::CounterSpec {
            counter: fact.counter(),
            rounds: fact.rounds(),
            initializer: fact.initializer(),
            update_rhs: fact.decrement(),
            initializer_span: node(fact.initializer())?.span(),
            condition_span: condition.span(),
            zero_span: inputs
                .parsed
                .ast()
                .expressions()
                .get(*zero)
                .map_err(|_| fail())?
                .span(),
            one_span: inputs
                .parsed
                .ast()
                .expressions()
                .get(*one)
                .map_err(|_| fail())?
                .span(),
            update_span: decrement.span(),
        })
    }
    pub(in crate::ssa::refine) fn bind_counter(
        &mut self,
        symbol: UnitSymbolId,
        id: UnitExpressionId,
        position: (BlockRef, usize),
        slots: (Slot, Slot),
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.loops {
            if let Some(c) = &mut p.counter {
                c.bind(
                    symbol,
                    id,
                    counter::AliasUse {
                        block: position.0,
                        index: position.1,
                        input: slots.0,
                        output: slots.1,
                    },
                    span,
                )?;
            }
        }
        Ok(())
    }
    pub(in crate::ssa::refine) fn assign_counter(
        &mut self,
        symbol: UnitSymbolId,
        id: UnitExpressionId,
        position: (BlockRef, usize),
        slots: (Slot, Slot),
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.loops {
            if let Some(c) = &mut p.counter {
                c.assign(
                    symbol,
                    id,
                    counter::AliasUse {
                        block: position.0,
                        index: position.1,
                        input: slots.0,
                        output: slots.1,
                    },
                    span,
                )?;
            }
        }
        Ok(())
    }
    pub(in crate::ssa::refine) fn checked_counter(
        &mut self,
        id: UnitExpressionId,
        blocks: [BlockRef; 3],
        values: [Slot; 4],
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in &mut self.loops {
            if let Some(c) = &mut p.counter {
                c.checked(id, blocks, values, span)?;
            }
        }
        Ok(())
    }
}
impl Emitted {
    pub(in crate::ssa::refine) fn alias(
        &mut self,
        body: &RefinedBody,
        position: (BlockRef, usize),
        slots: (Slot, Slot),
        entity: EntityId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if body
            .arm_loop_plans
            .loops
            .iter()
            .all(|p| p.counter.is_none())
        {
            return Ok(());
        }
        if self
            .aliases
            .insert(position, (slots.0, slots.1, entity))
            .is_some()
        {
            return Err(invalid(span));
        }
        Ok(())
    }
    pub(super) fn check_counter_required(
        &self,
        body: &RefinedBody,
        span: Span,
    ) -> Result<(), LoweringError> {
        let actual: Vec<_> = body
            .arm_loop_plans
            .loops
            .iter()
            .filter_map(|p| p.counter_fact.as_ref())
            .collect();
        if actual.len() != body.arm_counter_required.len()
            || actual
                .iter()
                .zip(&body.arm_counter_required)
                .any(|(a, b)| !a.same_identity(b))
            || body
                .arm_loop_plans
                .loops
                .iter()
                .any(|p| p.counter.is_some() != p.counter_fact.is_some())
        {
            return Err(invalid(span));
        }
        Ok(())
    }
}
