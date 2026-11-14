//! Exact nested source envelope only: no CFG, generation or final-owned authority.
use super::*;
use crate::parser::{
    AssignmentOperator, BinaryOperator, FunctionForm, IntegerLiteralKind, Item, LiteralKind,
    NameMarker, Statement, StringPart, VariableKind,
};

pub(super) struct DualCoverage {
    pub(super) callable: crate::type_checking::UnitItemId,
    pub(super) parent_counter: UnitSymbolId,
    pub(super) child_counter: UnitSymbolId,
    pub(super) parameter: UnitSymbolId,
    pub(super) parent_condition: UnitExpressionId,
    pub(super) child_condition: UnitExpressionId,
    pub(super) parent_step: UnitExpressionId,
    pub(super) child_step: UnitExpressionId,
}
impl DropPlanner<'_, '_> {
    pub(super) fn dual_generation_coverage(
        &self,
        controls: (UnitStatementId, UnitStatementId),
        root: UnitSymbolId,
        old_roots: [UnitSymbolId; 2],
        sites: [UnitExpressionId; 5],
        retained: &Retained,
    ) -> Option<DualCoverage> {
        let (parent, child) = controls;
        let [initial, parent_incoming, parent_call, incoming, call] = sites;
        let [parent_old_root, old_root] = old_roots;
        let c = self.checker;
        let ast = c.parsed.ast();
        if !c.parsed.diagnostics().is_empty()
            || parent.source_unit() != c.source_unit
            || child.source_unit() != c.source_unit
            || parent == child
            || [
                initial,
                parent_incoming,
                parent_call,
                incoming,
                call,
                retained.initial,
                retained.call,
            ]
            .iter()
            .any(|id| id.source_unit() != c.source_unit)
        {
            return None;
        }
        use crate::ownership_checking::{LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget};
        if retained.loan.call() != retained.call
            || retained.loan.kind() != LoanKind::Shared
            || !matches!(retained.loan.target(), UnitLoanTarget::Place(p) if p.is_root() && p.root() == retained.root)
            || retained.cleanup.target() != UnitDropTarget::Named(retained.root)
            || retained.cleanup.point() != UnitDropPoint::CallReturn(retained.call)
        {
            return None;
        }
        if c.ownership_primitives.len() != 2 || parent_call == call {
            return None;
        }
        for (expected_call, expected_incoming) in [(parent_call, parent_incoming), (call, incoming)]
        {
            let mut plans = c
                .ownership_primitives
                .iter()
                .filter(|p| p.descriptor().expression() == expected_call);
            let plan = plans.next()?;
            if plans.next().is_some()
                || plan.descriptor().kind() != OwnershipPrimitiveKind::Replace
                || plan.places().len() != 1
                || !plan.places()[0].is_root()
                || plan.places()[0].root() != root
                || plan.descriptor().operands().get(1) != Some(&expected_incoming)
            {
                return None;
            }
        }
        let retained = Some(retained);
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
        let elements = block_elements(*block)?;
        let [
            root_statement,
            kept_statement,
            parent_counter_statement,
            parent_loop,
            suffix,
        ] = elements
        else {
            return None;
        };
        let retained_statement = Some(kept_statement);
        let suffix_statement = Some(suffix);
        if UnitStatementId::new(c.source_unit, *parent_loop) != parent {
            return None;
        }
        let Statement::While {
            condition: parent_condition,
            body: parent_body,
            ..
        } = ast.statements().get(*parent_loop).ok()?.payload()
        else {
            return None;
        };
        let [
            parent_old_statement,
            counter_statement,
            actual_loop,
            parent_step_statement,
        ] = block_elements(*parent_body)?
        else {
            return None;
        };
        if UnitStatementId::new(c.source_unit, *actual_loop) != child {
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
        let outer = binding(*parent_counter_statement, VariableKind::Var)?;
        let old = binding(*old_statement, VariableKind::Val)?;
        let parent_old = binding(*parent_old_statement, VariableKind::Val)?;
        if parent_old.1 != parent_old_root
            || c.unit_expression(parent_old.2) != parent_call
            || !symbol_type(parent_old_root, BuiltinType::String)
            || a.1 != root
            || old.1 != old_root
            || c.unit_expression(a.2) != initial
            || c.unit_expression(old.2) != call
            || !symbol_type(root, BuiltinType::String)
            || !symbol_type(old_root, BuiltinType::String)
            || !symbol_type(n.1, BuiltinType::Int)
            || !symbol_type(outer.1, BuiltinType::Int)
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
        if name(n.2)? != parameter || name(outer.2)? != parameter {
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
        let Expression::Binary {
            left: parent_left,
            operator: BinaryOperator::Greater,
            right: parent_right,
            ..
        } = ast.expressions().get(*parent_condition).ok()?.payload()
        else {
            return None;
        };
        if name(*parent_left)? != outer.1
            || !expression_type(*parent_condition, BuiltinType::Boolean)
        {
            return None;
        }
        integer(*parent_right, "0")?;
        let Statement::Expression {
            expression: parent_step,
        } = ast.statements().get(*parent_step_statement).ok()?.payload()
        else {
            return None;
        };
        let Expression::Assignment {
            target: parent_target,
            operator: AssignmentOperator::SubtractAssign,
            value: parent_value,
            ..
        } = ast.expressions().get(*parent_step).ok()?.payload()
        else {
            return None;
        };
        if name(*parent_target)? != outer.1 {
            return None;
        }
        integer(*parent_value, "1")?;
        let roots = [
            root,
            parent_old_root,
            old_root,
            n.1,
            outer.1,
            parameter,
            retained?.root,
        ];
        if roots
            .iter()
            .enumerate()
            .any(|(i, r)| roots[..i].contains(r))
        {
            return None;
        }
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
        let Expression::Call {
            callee: parent_callee,
            arguments: parent_arguments,
            type_arguments: parent_type_arguments,
            ..
        } = ast
            .expressions()
            .get(parent_call.expression())
            .ok()?
            .payload()
        else {
            return None;
        };
        let [parent_argument, parent_next] = parent_arguments.as_slice() else {
            return None;
        };
        if !parent_type_arguments.is_empty()
            || name(parent_argument.value)? != root
            || c.unit_expression(parent_next.value) != parent_incoming
            || !matches!(
                ast.expressions().get(*parent_callee).ok()?.payload(),
                Expression::Name
            )
        {
            return None;
        }
        for expression in [
            initial.expression(),
            parent_incoming.expression(),
            incoming.expression(),
        ] {
            if !expression_type(expression, BuiltinType::String)
                || !matches!(ast.expressions().get(expression).ok()?.payload(), Expression::String { parts }
                    if parts.iter().all(|p| matches!(p, StringPart::Text(_))))
            {
                return None;
            }
        }
        // Every source arena node participates exactly once in the closed envelope.
        let mut items = vec![*function_id, a.0, outer.0, n.0, parent_old.0, old.0];
        let mut statements = vec![
            *block,
            *root_statement,
            *parent_counter_statement,
            *parent_loop,
            *parent_body,
            *parent_old_statement,
            *parent_step_statement,
            *counter_statement,
            *actual_loop,
            *body,
            *old_statement,
            *step_statement,
        ];
        let mut expressions = vec![
            initial.expression(),
            parent_call.expression(),
            *parent_callee,
            parent_argument.value,
            parent_incoming.expression(),
            outer.2,
            *parent_condition,
            *parent_left,
            *parent_right,
            *parent_step,
            *parent_target,
            *parent_value,
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
        if let (Some(kept), Some(binding_statement), Some(suffix_statement)) =
            (retained, retained_statement, suffix_statement)
        {
            let b = binding(*binding_statement, VariableKind::Var)?;
            if b.1 != kept.root
                || c.unit_expression(b.2) != kept.initial
                || !symbol_type(b.1, BuiltinType::String)
                || [root, old_root, n.1, parameter].contains(&b.1)
            {
                return None;
            }
            if !expression_type(b.2, BuiltinType::String)
                || !matches!(ast.expressions().get(b.2).ok()?.payload(), Expression::String { parts } if parts.iter().all(|p| matches!(p, StringPart::Text(_))))
            {
                return None;
            }
            let Statement::Expression { expression: suffix } =
                ast.statements().get(*suffix_statement).ok()?.payload()
            else {
                return None;
            };
            if c.unit_expression(*suffix) != kept.call {
                return None;
            }
            let Expression::Call {
                callee,
                arguments,
                type_arguments,
                ..
            } = ast.expressions().get(*suffix).ok()?.payload()
            else {
                return None;
            };
            let [arg] = arguments.as_slice() else {
                return None;
            };
            let Item::Variable {
                name: NameMarker::Present(kept_span),
                ..
            } = ast.items().get(b.0).ok()?.payload()
            else {
                return None;
            };
            if kept.cleanup.value_origin() != *kept_span
                || kept.loan.begin_span() != ast.expressions().get(arg.value).ok()?.span()
                || kept.loan.end_span() != ast.expressions().get(*suffix).ok()?.span()
            {
                return None;
            }
            let typed_call = c.typed.call(kept.call)?;
            if !type_arguments.is_empty()
                || name(arg.value)? != kept.root
                || !matches!(
                    ast.expressions().get(*callee).ok()?.payload(),
                    Expression::Name
                )
                || !typed_call.prints_line()
                || typed_call.aborts()
                || typed_call.receiver().is_some()
                || c.typed.types().get(typed_call.return_type())
                    != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                || kept.loan.argument() != c.unit_expression(arg.value)
            {
                return None;
            }
            items.push(b.0);
            statements.extend([*binding_statement, *suffix_statement]);
            expressions.extend([b.2, *suffix, *callee, arg.value]);
        }
        if items.len() != 7
            || statements.len() != 14
            || expressions.len() != 27
            || ast.items().iter().count() != items.len()
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
        Some(DualCoverage {
            callable: crate::type_checking::UnitItemId::new(c.source_unit, *function_id),
            parent_counter: outer.1,
            child_counter: n.1,
            parameter,
            parent_condition: c.unit_expression(*parent_condition),
            child_condition: c.unit_expression(*condition),
            parent_step: c.unit_expression(*parent_step),
            child_step: c.unit_expression(*step),
        })
    }
}
