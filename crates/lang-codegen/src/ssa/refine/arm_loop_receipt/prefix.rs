//! A separately owned scalar prefix, captured from actual CFG and replayed at the boundary.
use super::prefix_fact::PrefixFact;
use super::*;
use crate::ssa::model::{Operation, Origin, PlaceAccess, ScalarConstant, TerminatorKind};
use lang_frontend::{ownership_checking::UnitLoopBreakPrefixFact, parser::LiteralKind};
#[derive(Clone, Debug)]
pub(super) struct Prefix {
    pub(super) fact: PrefixFact,
    role: usize,
    span: Span,
    calls: [(FunctionId, Span, i128, Span); 2],
    pub(super) split: Option<[BlockRef; 3]>,
    pub(super) join: Option<BlockRef>,
    used: Option<(BlockRef, Slot)>,
}
impl Plans {
    pub(in crate::ssa::refine) fn required_prefixes(
        inputs: &RefineInputs<'_>,
    ) -> Vec<UnitLoopBreakPrefixFact> {
        inputs
            .owned
            .no_carry_break_prefixes()
            .iter()
            .filter(|f| {
                Some(f.pair().pair().arms()[0].callable().item()) == inputs.function_item
                    && f.prefix().source_unit() == inputs.source_unit
            })
            .cloned()
            .collect()
    }
    pub(super) fn prefix_plan(
        inputs: &RefineInputs<'_>,
        pair: &UnitLoopBranchNoCarryFact,
    ) -> Result<Option<Prefix>, LoweringError> {
        let mut found = inputs
            .owned
            .no_carry_break_prefixes()
            .iter()
            .cloned()
            .map(PrefixFact::Tail)
            .chain(
                inputs
                    .owned
                    .no_carry_prefix_counter_breaks()
                    .iter()
                    .cloned()
                    .map(PrefixFact::Counter),
            )
            .filter(|p| p.pair().same_identity(pair));
        let Some(fact) = found.next() else {
            return Ok(None);
        };
        let fail = || invalid(inputs.span);
        if found.next().is_some() || !fact.owns(inputs) {
            return Err(fail());
        }
        let node = inputs
            .parsed
            .ast()
            .expressions()
            .get(fact.condition().expression())
            .map_err(|_| fail())?;
        if !matches!(node.payload(), Expression::Name) {
            return Err(fail());
        }
        let symbol = inputs
            .references
            .get(&(node.span().start(), node.span().end()))
            .ok_or_else(fail)?;
        let role = inputs
            .parameters
            .iter()
            .position(|s| s == symbol)
            .ok_or_else(fail)?;
        let mut calls = Vec::new();
        for id in fact.calls() {
            let call = inputs
                .parsed
                .ast()
                .expressions()
                .get(id.expression())
                .map_err(|_| fail())?;
            let Expression::Call { arguments, .. } = call.payload() else {
                return Err(fail());
            };
            if arguments.len() != 1 {
                return Err(fail());
            }
            let arg = inputs
                .parsed
                .ast()
                .expressions()
                .get(arguments[0].value)
                .map_err(|_| fail())?;
            let Expression::Literal(LiteralKind::Integer(kind)) = arg.payload() else {
                return Err(fail());
            };
            let value = lang_frontend::type_checking::integer_literal_magnitude(
                inputs.sources.slice(arg.span()).map_err(|_| fail())?,
                *kind,
            )
            .and_then(|v| i128::try_from(v).ok())
            .ok_or_else(fail)?;
            calls.push((
                super::super::fresh_loop_receipt::callee(inputs, id)?,
                call.span(),
                value,
                arg.span(),
            ));
        }
        Ok(Some(Prefix {
            fact: fact.clone(),
            role,
            span: node.span(),
            calls: calls.try_into().map_err(|_| fail())?,
            split: None,
            join: None,
            used: None,
        }))
    }
}
impl Prefix {
    pub(super) fn is_counter_composition(&self) -> bool {
        matches!(self.fact, PrefixFact::Counter(_))
    }
    pub(super) fn split(
        &mut self,
        id: UnitExpressionId,
        blocks: [BlockRef; 3],
        s: Span,
    ) -> Result<(), LoweringError> {
        if self.fact.prefix() == id
            && (self.split.replace(blocks).is_some() || blocks[1] == blocks[2])
        {
            return Err(invalid(s));
        }
        Ok(())
    }
    pub(super) fn join(
        &mut self,
        id: UnitExpressionId,
        b: BlockRef,
        preds: &[BlockRef],
        s: Span,
    ) -> Result<(), LoweringError> {
        if self.fact.prefix() == id {
            let split = self.split.ok_or_else(|| invalid(s))?;
            if preds != [split[1], split[2]] || self.join.replace(b).is_some() {
                return Err(invalid(s));
            }
        }
        Ok(())
    }
    pub(super) fn condition(
        &mut self,
        id: UnitExpressionId,
        b: BlockRef,
        v: Slot,
        s: Span,
    ) -> Result<(), LoweringError> {
        #[cfg(test)]
        if self.fact.condition() == id && super::publication::skip_prefix_capture() {
            return Ok(());
        }
        if self.fact.condition() == id
            && (self.split.map(|x| x[0]) != Some(b) || self.used.replace((b, v)).is_some())
        {
            return Err(invalid(s));
        }
        Ok(())
    }
    pub(super) fn fold(&mut self, b: BlockRef, next: BlockRef) {
        if let Some(bs) = &mut self.split {
            for x in bs {
                if *x == next {
                    *x = b;
                }
            }
        }
        if self.join == Some(next) {
            self.join = Some(b);
        }
        if let Some((x, _)) = &mut self.used
            && *x == next
        {
            *x = b;
        }
    }
}
impl Emitted {
    pub(super) fn check_prefix_required(
        &self,
        body: &RefinedBody,
        s: Span,
    ) -> Result<(), LoweringError> {
        let actual: Vec<_> = body
            .arm_loop_plans
            .loops
            .iter()
            .filter_map(|p| {
                p.prefix.as_ref().and_then(|p| match &p.fact {
                    PrefixFact::Tail(f) => Some(f),
                    _ => None,
                })
            })
            .collect();
        if actual.len() != body.arm_prefix_required.len()
            || actual
                .iter()
                .zip(&body.arm_prefix_required)
                .any(|(a, b)| !a.same_identity(b))
        {
            return Err(invalid(s));
        }
        Ok(())
    }
    pub(super) fn validate_prefix(
        &self,
        parameters: &[Slot],
        f: &Function,
        p: &Plan,
        s: Span,
    ) -> Result<(), LoweringError> {
        let Some(prefix) = &p.prefix else {
            return Ok(());
        };
        if !prefix.fact.matches_plan(p) {
            return Err(invalid(s));
        }
        let [head, a, b] = prefix.split.ok_or_else(|| invalid(s))?;
        let join = prefix.join.ok_or_else(|| invalid(s))?;
        if p.blocks.map(|x| x[0]) != Some(join)
            || [head, a, b].contains(&join)
            || head == a
            || head == b
            || a == b
        {
            return Err(invalid(s));
        }
        if f.entry_block() != Some(self.block(head, s)?) {
            return Err(invalid(s));
        }
        self.incoming(f, head, &[], s)?;
        self.links(f, head, &[a, b], s)?;
        self.incoming(f, a, &[head], s)?;
        self.incoming(f, b, &[head], s)?;
        self.links(f, a, &[join], s)?;
        self.links(f, b, &[join], s)?;
        self.incoming(f, join, &[a, b], s)?;
        // An independent Boolean prefix parameter can die after its Read. Only
        // that scalar slot may be trimmed; every other input (including all
        // loop roles and resource parameters) must still be transported.
        let scalar = *parameters.get(prefix.role).ok_or_else(|| invalid(s))?;
        for block in [head, a, b, join] {
            let actual = self.parameters.get(&block).ok_or_else(|| invalid(s))?;
            let expected = parameters
                .iter()
                .copied()
                .filter(|slot| {
                    *slot != scalar
                        || p.roles.contains(&prefix.role)
                        || p.counter_break
                            .as_ref()
                            .is_some_and(|b| b.role == prefix.role)
                        || actual.contains(slot)
                })
                .collect::<Vec<_>>();
            if *actual != expected {
                return Err(invalid(s));
            }
        }
        let (used_block, slot) = prefix.used.ok_or_else(|| invalid(s))?;
        if used_block != head {
            return Err(invalid(s));
        }
        let source = self.entity(
            head,
            *parameters.get(prefix.role).ok_or_else(|| invalid(s))?,
            s,
        )?;
        let actual = self
            .ends
            .get(&self.block(head, s)?)
            .and_then(|m| m.get(&slot))
            .copied()
            .ok_or_else(|| invalid(s))?;
        let ids = self.ids(f, head, s)?;
        match source {
            EntityId::Value(_) if ids.is_empty() && actual == source => {}
            EntityId::Loan(loan) if ids.len() == 1 => {
                let i = f.instruction(ids[0]).ok_or_else(|| invalid(s))?;
                if i.block != self.block(head, s)?
                    || i.origin != Origin::Source(prefix.span)
                    || i.operation
                        != (Operation::Read {
                            source: PlaceAccess::Loan(loan),
                        })
                    || i.results != [actual]
                {
                    return Err(invalid(s));
                }
            }
            _ => return Err(invalid(s)),
        }
        if !matches!(&f.block(self.block(head,s)?).and_then(|b|b.terminator.as_ref()).ok_or_else(||invalid(s))?.kind,TerminatorKind::Conditional{condition,..} if EntityId::Value(*condition)==actual)
        {
            return Err(invalid(s));
        }
        for (index, block) in [a, b].into_iter().enumerate() {
            let (callee, call_span, value, arg_span) = prefix.calls[index];
            let ids = self.ids(f, block, s)?;
            if ids.len() != 2 {
                return Err(invalid(s));
            }
            let constant = f.instruction(ids[0]).ok_or_else(|| invalid(s))?;
            if constant.block != self.block(block, s)?
                || constant.operation != Operation::Constant(ScalarConstant::Integer(value))
                || constant.origin != Origin::Source(arg_span)
                || constant.results.len() != 1
                || !matches!(constant.results[0], EntityId::Value(_))
            {
                return Err(invalid(s));
            }
            let call = f.instruction(ids[1]).ok_or_else(|| invalid(s))?;
            if call.block != self.block(block, s)?
                || call.operation
                    != (Operation::DirectCall {
                        callee,
                        receiver: None,
                        arguments: constant.results.clone(),
                    })
                || call.origin != Origin::Source(call_span)
                || !call.results.is_empty()
            {
                return Err(invalid(s));
            }
            // The entire resource state is forwarded unchanged; no slot substitution is allowed.
            for param in self.parameters.get(&block).ok_or_else(|| invalid(s))? {
                if self
                    .ends
                    .get(&self.block(block, s)?)
                    .and_then(|m| m.get(param))
                    .copied()
                    != Some(self.entity(block, *param, s)?)
                {
                    return Err(invalid(s));
                }
            }
        }
        Ok(())
    }
}
