//! Verify the ordinary return lowering's actual callee, four live operands and delivered result.
use super::*;
use crate::ssa::unit_plan::UnitFunctionInstanceKey;
use lang_frontend::type_checking::{UnitCallTarget, UnitCallableTarget};

impl UnitExpressionLowerer<'_> {
    pub(super) fn validate_nested_return(
        &self,
        pair: map_iteration::QualifiedNested,
        inner: Event,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let tail = pair.tail.ok_or_else(bad)?;
        let operand = tail.operand.ok_or_else(bad)?;
        let outer_state = self.map_state(pair.outer, span)?;
        let outer = outer_state.nested.event(inner.point).ok_or_else(bad)?;
        if outer.block != inner.block
            || outer.start != inner.start
            || outer.end != inner.end
            || outer.returned.is_some()
            || inner.returned.is_some()
            || !matches!(outer_state.phase, MapPhase::Terminal { point, finished: true, source_ended: true, owner_dropped: false } if point == inner.point)
        {
            return Err(bad());
        }
        let block = self.function.block(inner.block).ok_or_else(bad)?;
        let Some(TerminatorKind::Return { values }) =
            block.terminator.as_ref().map(|term| &term.kind)
        else {
            return Err(bad());
        };
        let [result] = values.as_slice() else {
            return Err(bad());
        };
        let ty = self.expression_ssa_type(operand.expression(), span)?;
        let mut current = *result;
        let mut seen = BTreeSet::new();
        let mut previous_position = inner.start;
        let call = loop {
            if !seen.insert(current)
                || self
                    .function
                    .entity(EntityId::Value(current))
                    .map(|data| data.ty)
                    != Some(EntityType::Value(ty))
            {
                return Err(bad());
            }
            let instruction = carrier::instruction(self.function, EntityId::Value(current), span)?;
            let position = block
                .instructions
                .iter()
                .position(|id| *id == instruction.id)
                .ok_or_else(bad)?;
            if instruction.block != inner.block || position >= previous_position {
                return Err(bad());
            }
            previous_position = position;
            if let Operation::Copy { source } = instruction.operation {
                current = source;
            } else {
                break instruction;
            }
        };
        let descriptor = self.typed.call(operand).ok_or_else(bad)?;
        let UnitCallTarget::Declaration(target) = descriptor.target() else {
            return Err(bad());
        };
        let key = UnitFunctionInstanceKey::new(target, Vec::new());
        let resolved = self
            .source_plan
            .call_site(self.source_token, operand)
            .ok_or_else(bad)?;
        let callee = *self.function_ids.get(&key).ok_or_else(bad)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(operand.expression())
            .map_err(|_| bad())?;
        let arguments = [outer.key, outer.value, inner.key, inner.value]
            .into_iter()
            .map(|loan| loan.map(EntityId::Loan).ok_or_else(bad))
            .collect::<Result<Vec<_>, _>>()?;
        if operand.source_unit() != self.source_unit
            || descriptor.expression() != operand
            || resolved.key() != &key
            || resolved.key().target() != UnitCallableTarget::Declaration(target)
            || !resolved.delegation().is_empty()
            || call.origin != Origin::Source(node.span())
            || call.operation
                != (Operation::DirectCall {
                    callee,
                    receiver: None,
                    arguments,
                })
            || call.results.as_slice() != [EntityId::Value(current)]
        {
            return Err(bad());
        }
        let calls = self
            .function
            .instructions
            .iter()
            .filter(|instruction| {
                instruction.origin == Origin::Source(node.span())
                    && matches!(instruction.operation, Operation::DirectCall { .. })
            })
            .collect::<Vec<_>>();
        if !matches!(calls.as_slice(), [actual] if actual.id == call.id) {
            return Err(bad());
        }
        Ok(*result)
    }
}
