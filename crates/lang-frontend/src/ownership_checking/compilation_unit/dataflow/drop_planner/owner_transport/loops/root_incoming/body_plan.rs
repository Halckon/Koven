//! Shared exact syntax contract; actual events are required separately.
use super::*;
pub(in super::super) struct BodyPlan {
    pub(in super::super) condition: UnitExpressionId,
    pub(in super::super) body: UnitStatementId,
    pub(in super::super) condition_binding: UnitSymbolId,
    pub(super) tail: super::guard_reset::Tail,
    pub(in super::super) planned: Vec<(UnitExpressionId, UnitSymbolId, UnitExpressionId)>,
    pub(in super::super) bindings: Vec<(UnitSymbolId, UnitStatementId)>,
}
impl BodyPlan {
    pub(in super::super) fn complete_tail(&mut self, id: UnitExpressionId, ready: bool) -> bool {
        if let Some(counter) = &mut self.tail.counter
            && !counter.complete(id, ready)
        {
            return false;
        }
        if self.tail.reset == Some(id) {
            return ready && self.tail.counter.as_ref().is_none_or(|c| c.is_complete());
        }
        true
    }
    pub(in super::super) fn completed_counter(
        &self,
    ) -> Option<(UnitSymbolId, UnitSymbolId, [UnitExpressionId; 4])> {
        let counter = self.tail.counter.as_ref()?;
        if !counter.is_complete() {
            return None;
        }
        let (symbol, sites) = counter.sites();
        Some((self.condition_binding, symbol, sites))
    }
    pub(in super::super) fn reset(&self) -> Option<UnitExpressionId> {
        self.tail.reset
    }
    pub(in super::super) fn admit(
        checker: &Checker<'_>,
        control: UnitStatementId,
        roots: &[UnitSymbolId],
        entered_body: Option<(UnitExpressionId, UnitStatementId)>,
    ) -> Option<Self> {
        if roots.len() < 2
            || roots
                .iter()
                .enumerate()
                .any(|(i, r)| roots[..i].contains(r))
        {
            return None;
        }
        let ast = checker.parsed.ast();
        let Statement::While {
            condition, body, ..
        } = ast.statements().get(control.statement()).ok()?.payload()
        else {
            return None;
        };
        if entered_body
            != Some((
                checker.unit_expression(*condition),
                UnitStatementId::new(control.source_unit(), *body),
            ))
        {
            return None;
        }
        if !matches!(
            ast.expressions().get(*condition).ok()?.payload(),
            Expression::Name
        ) || !exact_type(checker, *condition, BuiltinType::Boolean)
        {
            return None;
        }
        let condition_binding =
            checker.reference_symbol(ast.expressions().get(*condition).ok()?.span())?;
        if condition_binding.source_unit() != control.source_unit()
            || roots.contains(&condition_binding)
            || checker
                .typed
                .symbol_type(condition_binding)
                .and_then(|ty| checker.typed.types().get(ty))
                != Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
        {
            return None;
        }
        let Statement::Block { elements } = ast.statements().get(*body).ok()?.payload() else {
            return None;
        };
        let (elements, tail) =
            super::guard_reset::body_parts(checker, elements, roots.len(), condition_binding)?;
        let mut planned = Vec::new();
        let mut bindings = Vec::new();
        for statement in elements {
            let (binding, call) = replace_break::variable(checker, *statement, VariableKind::Val)?;
            bindings.push((
                binding,
                UnitStatementId::new(control.source_unit(), *statement),
            ));
            let call = checker.unit_expression(call);
            let plan = checker
                .ownership_primitives
                .iter()
                .find(|plan| plan.descriptor().expression() == call)?;
            if plan.descriptor().kind() != OwnershipPrimitiveKind::Replace
                || plan.places().len() != 1
                || !plan.places()[0].is_root()
                || plan.descriptor().operands().len() != 2
            {
                return None;
            }
            let root = plan.places()[0].root();
            let producer = plan.descriptor().operands()[1];
            if !replace_break::literal(checker, producer.expression())
                || !roots.contains(&root)
                || planned.iter().any(|(_, known, _)| *known == root)
            {
                return None;
            }
            planned.push((call, root, producer));
        }
        Some(Self {
            condition: checker.unit_expression(*condition),
            body: UnitStatementId::new(control.source_unit(), *body),
            condition_binding,
            tail,
            planned,
            bindings,
        })
    }
}
