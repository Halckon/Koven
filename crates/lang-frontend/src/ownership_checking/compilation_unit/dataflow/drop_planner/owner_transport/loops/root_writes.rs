//! Private observed primitive commits, not public owner tokens or cleanup authority.
use super::*;
#[derive(Clone, Debug)]
pub(in super::super::super) struct RootWriteReceipt {
    pub(in super::super::super) control: UnitStatementId,
    pub(in super::super::super) root: UnitSymbolId,
    pub(in super::super::super) call: UnitExpressionId,
    pub(in super::super::super) producer: UnitExpressionId,
}
impl ControlTransactions {
    #[cfg(test)]
    pub(super) fn root_writes(&self, control: UnitStatementId) -> &[RootWriteReceipt] {
        self.root_entries
            .iter()
            .find(|(key, _)| *key == control)
            .map_or(&[], |(_, evidence)| evidence.writes.as_slice())
    }
}
fn direct_string(checker: &Checker<'_>, expression: ExpressionId) -> bool {
    matches!(checker.parsed.ast().expressions().get(expression).ok().map(|node| node.payload()),
        Some(Expression::String { parts }) if parts.iter().all(|part| !matches!(part, StringPart::Interpolation { .. })))
        && exact_type(checker, expression, BuiltinType::String)
}
impl DropPlanner<'_, '_> {
    // Called only after the actual String evaluation has reached its completion hook.
    pub(in super::super::super) fn record_root_literal(&mut self, expression: ExpressionId) {
        let Some(control) = self.owner_controls.current() else {
            return;
        };
        if !direct_string(self.checker, expression) {
            return;
        }
        let expression = self.checker.unit_expression(expression);
        if let Some((_, evidence)) = self
            .owner_loop
            .root_entries
            .iter_mut()
            .find(|(key, _)| *key == control)
            && !evidence.completed_strings.contains(&expression)
        {
            evidence.completed_strings.push(expression);
        }
    }
    // Called at the real checked primitive commit boundary even when legacy observation declined.
    pub(in super::super::super) fn record_root_primitive_write(
        &mut self,
        call: ExpressionId,
        state: &ValueState,
    ) {
        let Some(control) = self.owner_controls.current() else {
            return;
        };
        let call_id = self.checker.unit_expression(call);
        let Some(plan) = self.checker.ownership_primitives.iter().find(|plan| {
            plan.descriptor().expression() == call_id
                && plan.descriptor().kind() == OwnershipPrimitiveKind::Replace
                && plan.places().len() == 1
                && plan.places()[0].is_root()
                && plan.descriptor().operands().len() == 2
                && self
                    .checker
                    .typed
                    .types()
                    .get(plan.descriptor().value_type())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::String))
        }) else {
            return;
        };
        let root = plan.places()[0].root();
        let producer = plan.descriptor().operands()[1];
        if state.position(root).is_none()
            || !direct_string(self.checker, producer.expression())
            || !self.pending_value_registered(call, producer.expression(), state)
        {
            return;
        }
        let Some((_, evidence)) = self
            .owner_loop
            .root_entries
            .iter_mut()
            .find(|(key, _)| *key == control)
        else {
            return;
        };
        if !evidence.roots.iter().any(|(known, _)| *known == root)
            || evidence.writes.iter().any(|write| {
                write.control == control
                    && (write.call == call_id
                        || write.producer == producer
                        || write.root.source_unit() != root.source_unit())
            })
        {
            return;
        }
        let Some(index) = evidence
            .completed_strings
            .iter()
            .position(|completed| *completed == producer)
        else {
            return;
        };
        evidence.completed_strings.remove(index);
        evidence.writes.push(RootWriteReceipt {
            control,
            root,
            call: call_id,
            producer,
        });
    }
}
