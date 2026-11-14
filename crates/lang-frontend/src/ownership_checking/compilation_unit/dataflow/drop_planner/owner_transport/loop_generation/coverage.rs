//! Closed source envelope for the private generation candidate, not a CFG proof.
use super::*;
use crate::parser::{
    AssignmentOperator, BinaryOperator, FunctionForm, IntegerLiteralKind, Item, LiteralKind,
    NameMarker, Statement, StringPart, VariableKind,
};

pub(in super::super) struct Coverage {
    pub(in super::super) callable: crate::type_checking::UnitItemId,
    pub(in super::super) counter: UnitSymbolId,
    pub(in super::super) parameter: UnitSymbolId,
    pub(in super::super) condition: UnitExpressionId,
    pub(in super::super) step: UnitExpressionId,
}
impl DropPlanner<'_, '_> {
    pub(in super::super) fn loop_generation_coverage(
        &self,
        control: UnitStatementId,
        root: UnitSymbolId,
        old_root: UnitSymbolId,
        initial: UnitExpressionId,
        incoming: UnitExpressionId,
        call: UnitExpressionId,
    ) -> Option<Coverage> {
        let c = self.checker;
        let ast = c.parsed.ast();
        let [function_id] = c.parsed.roots() else {
            return None;
        };
        let Item::Function {
            extension_receiver: None,
            type_parameters,
            parameters,
            form: FunctionForm::ImplicitUnitBlock(block),
            ..
        } = ast.items().get(*function_id).ok()?.payload()
        else {
            return None;
        };
        let [parameter] = parameters.as_slice() else {
            return None;
        };
        if !type_parameters.is_empty() || parameter.mode_marker.is_some() {
            return None;
        }
        let NameMarker::Present(parameter_span) = parameter.name else {
            return None;
        };
        let parameter = *c.symbols_by_span.get(&span_key(parameter_span))?;
        let symbol_type = |symbol, expected| {
            c.typed
                .symbol_type(symbol)
                .and_then(|t| c.typed.types().get(t))
                == Some(&UnitTypeKind::Builtin(expected))
        };
        let expression_type = |id, expected| {
            c.typed
                .expression_type(c.unit_expression(id))
                .and_then(|t| c.typed.types().get(t))
                == Some(&UnitTypeKind::Builtin(expected))
        };
        if !symbol_type(parameter, BuiltinType::Int) {
            return None;
        }
        let block_elements = |id| match ast.statements().get(id).ok()?.payload() {
            Statement::Block { elements } | Statement::ControlBody { elements } => {
                Some(elements.as_slice())
            }
            _ => None,
        };
        let [root_statement, counter_statement, actual_loop] = block_elements(*block)? else {
            return None;
        };
        if UnitStatementId::new(c.source_unit, *actual_loop) != control {
            return None;
        }
        let Statement::While {
            condition, body, ..
        } = ast.statements().get(*actual_loop).ok()?.payload()
        else {
            return None;
        };
        let [old_statement, step_statement] = block_elements(*body)? else {
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
                type_ref: None,
                ..
            } = ast.items().get(*declaration).ok()?.payload()
            else {
                return None;
            };
            if *kind != expected {
                return None;
            }
            Some((
                *declaration,
                *c.symbols_by_span.get(&span_key(*span))?,
                *initializer,
            ))
        };
        let a = binding(*root_statement, VariableKind::Var)?;
        let n = binding(*counter_statement, VariableKind::Var)?;
        let old = binding(*old_statement, VariableKind::Val)?;
        if a.1 != root
            || old.1 != old_root
            || c.unit_expression(a.2) != initial
            || c.unit_expression(old.2) != call
            || !symbol_type(root, BuiltinType::String)
            || !symbol_type(old_root, BuiltinType::String)
            || !symbol_type(n.1, BuiltinType::Int)
        {
            return None;
        }
        let name = |id| {
            let node = ast.expressions().get(id).ok()?;
            if !matches!(node.payload(), Expression::Name) {
                return None;
            }
            c.reference_symbol(node.span())
        };
        if name(n.2)? != parameter {
            return None;
        }
        let integer = |id, text| {
            let node = ast.expressions().get(id).ok()?;
            (matches!(
                node.payload(),
                Expression::Literal(LiteralKind::Integer(IntegerLiteralKind::Unsuffixed))
            ) && c.sources.slice(node.span()).ok()? == text
                && expression_type(id, BuiltinType::Int))
            .then_some(())
        };
        let Expression::Binary {
            left,
            operator: BinaryOperator::Greater,
            right,
            ..
        } = ast.expressions().get(*condition).ok()?.payload()
        else {
            return None;
        };
        if name(*left)? != n.1 || !expression_type(*condition, BuiltinType::Boolean) {
            return None;
        }
        integer(*right, "0")?;
        let Statement::Expression { expression: step } =
            ast.statements().get(*step_statement).ok()?.payload()
        else {
            return None;
        };
        let Expression::Assignment {
            target,
            operator: AssignmentOperator::SubtractAssign,
            value,
            ..
        } = ast.expressions().get(*step).ok()?.payload()
        else {
            return None;
        };
        if name(*target)? != n.1 {
            return None;
        }
        integer(*value, "1")?;
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = ast.expressions().get(call.expression()).ok()?.payload()
        else {
            return None;
        };
        let [argument, next] = arguments.as_slice() else {
            return None;
        };
        if !type_arguments.is_empty()
            || name(argument.value)? != root
            || c.unit_expression(next.value) != incoming
            || !matches!(
                ast.expressions().get(*callee).ok()?.payload(),
                Expression::Name
            )
        {
            return None;
        }
        for expression in [initial.expression(), incoming.expression()] {
            if !expression_type(expression, BuiltinType::String)
                || !matches!(ast.expressions().get(expression).ok()?.payload(), Expression::String { parts }
                    if parts.iter().all(|p| matches!(p, StringPart::Text(_))))
            {
                return None;
            }
        }
        // Every source arena node participates exactly once in the closed envelope.
        let items = [*function_id, a.0, n.0, old.0];
        let statements = [
            *block,
            *root_statement,
            *counter_statement,
            *actual_loop,
            *body,
            *old_statement,
            *step_statement,
        ];
        let expressions = [
            initial.expression(),
            n.2,
            *condition,
            *left,
            *right,
            call.expression(),
            *callee,
            argument.value,
            incoming.expression(),
            *step,
            *target,
            *value,
        ];
        if ast.items().iter().count() != items.len()
            || ast.statements().iter().count() != statements.len()
            || ast.expressions().iter().count() != expressions.len()
            || ast
                .items()
                .iter()
                .any(|(id, _)| items.iter().filter(|i| **i == id).count() != 1)
            || ast
                .statements()
                .iter()
                .any(|(id, _)| statements.iter().filter(|i| **i == id).count() != 1)
            || ast
                .expressions()
                .iter()
                .any(|(id, _)| expressions.iter().filter(|i| **i == id).count() != 1)
        {
            return None;
        }
        Some(Coverage {
            callable: crate::type_checking::UnitItemId::new(c.source_unit, *function_id),
            counter: n.1,
            parameter,
            condition: c.unit_expression(*condition),
            step: c.unit_expression(*step),
        })
    }
}
