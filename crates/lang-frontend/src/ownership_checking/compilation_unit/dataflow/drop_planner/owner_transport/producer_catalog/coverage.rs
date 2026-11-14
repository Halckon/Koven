use super::*;
use crate::parser::{
    Expression, FunctionForm, Item, NameMarker, Statement, StringPart, VariableKind,
};
use crate::type_checking::{BuiltinType, UnitTypeKind};

pub(super) struct Coverage {
    pub(super) callable: crate::type_checking::UnitItemId,
    pub(super) literals: [UnitExpressionId; 2],
    pub(super) bindings: [(UnitSymbolId, UnitExpressionId); 2],
    pub(super) call: UnitExpressionId,
    pub(super) old_statement: UnitStatementId,
}
impl DropPlanner<'_, '_> {
    pub(super) fn catalog_coverage(&self) -> Option<Coverage> {
        let ast = self.checker.parsed.ast();
        if ast.items().iter().count() != 3
            || ast.statements().iter().count() != 3
            || ast.expressions().iter().count() != 5
        {
            return None;
        }
        let (function_id, function) = ast
            .items()
            .iter()
            .find(|(_, n)| matches!(n.payload(), Item::Function { .. }))?;
        let Item::Function {
            extension_receiver: None,
            type_parameters,
            parameters,
            form: FunctionForm::ImplicitUnitBlock(body),
            ..
        } = function.payload()
        else {
            return None;
        };
        if !type_parameters.is_empty() || !parameters.is_empty() {
            return None;
        }
        let Statement::Block { elements } = ast.statements().get(*body).ok()?.payload() else {
            return None;
        };
        let [first, second] = elements.as_slice() else {
            return None;
        };
        let binding = |statement, expected| {
            let Statement::LocalVariable { declaration } =
                ast.statements().get(statement).ok()?.payload()
            else {
                return None;
            };
            let Item::Variable {
                name: NameMarker::Present(span),
                kind,
                initializer,
                ..
            } = ast.items().get(*declaration).ok()?.payload()
            else {
                return None;
            };
            if *kind != expected {
                return None;
            }
            let root = *self.checker.symbols_by_span.get(&span_key(*span))?;
            if self
                .checker
                .typed
                .types()
                .get(self.checker.typed.symbol_type(root)?)
                != Some(&UnitTypeKind::Builtin(BuiltinType::String))
            {
                return None;
            }
            Some((root, self.checker.unit_expression(*initializer)))
        };
        let a = binding(*first, VariableKind::Var)?;
        let old = binding(*second, VariableKind::Val)?;
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = ast.expressions().get(old.1.expression()).ok()?.payload()
        else {
            return None;
        };
        let [root_arg, incoming] = arguments.as_slice() else {
            return None;
        };
        if !type_arguments.is_empty()
            || !matches!(
                ast.expressions().get(*callee).ok()?.payload(),
                Expression::Name
            )
            || !matches!(
                ast.expressions().get(root_arg.value).ok()?.payload(),
                Expression::Name
            )
        {
            return None;
        }
        let plan = self
            .checker
            .ownership_primitives
            .iter()
            .find(|p| p.descriptor().expression() == old.1)?;
        if plan.descriptor().kind() != OwnershipPrimitiveKind::Replace
            || plan.places().len() != 1
            || !plan.places()[0].is_root()
            || plan.places()[0].root() != a.0
        {
            return None;
        }
        let literals = [a.1, self.checker.unit_expression(incoming.value)];
        for literal in literals {
            if !matches!(ast.expressions().get(literal.expression()).ok()?.payload(), Expression::String { parts } if parts.iter().all(|p| matches!(p, StringPart::Text(_))))
            {
                return None;
            }
        }
        let ids = [
            a.1.expression(),
            old.1.expression(),
            *callee,
            root_arg.value,
            incoming.value,
        ];
        if ast
            .expressions()
            .iter()
            .any(|(id, _)| ids.iter().filter(|candidate| **candidate == id).count() != 1)
        {
            return None;
        }
        Some(Coverage {
            callable: crate::type_checking::UnitItemId::new(self.checker.source_unit, function_id),
            literals,
            bindings: [a, old],
            call: old.1,
            old_statement: UnitStatementId::new(self.checker.source_unit, *second),
        })
    }
}
