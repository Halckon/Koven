//! Independent child-then-parent envelope. Only two distinct counters and two suffix uses.
use super::*;
use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::UnitSymbolId,
    parser::{
        AssignmentOperator, BinaryOperator, Expression, FunctionForm, IntegerLiteralKind, Item,
        LiteralKind, NameMarker, ParameterModeMarker, Statement, StringPart, VariableKind,
    },
    source::Span,
    type_checking::{BuiltinType, UnitTypeKind},
};
#[derive(Clone, Copy)]
pub(super) struct Binding {
    pub(super) item: ItemId,
    pub(super) initializer: ExpressionId,
    pub(super) symbol: UnitSymbolId,
    pub(super) span: Span,
}
pub(super) struct Roles {
    pub(super) function: ItemId,
    // root, retained, parent counter, parent detached old, child counter, child detached old
    pub(super) bindings: [Binding; 6],
    pub(super) parameters: [UnitSymbolId; 2],
    pub(super) controls: [StatementId; 2],
    pub(super) conditions: [ExpressionId; 2],
    pub(super) steps: [ExpressionId; 2],
    pub(super) incoming: [ExpressionId; 2],
    pub(super) suffix: ExpressionId,
    pub(super) final_use: Option<ExpressionId>,
}
impl DropPlanner<'_, '_> {
    pub(super) fn after_child_coverage(
        &self,
        wrapper: Option<&super::named_coverage::NamedCoverage>,
    ) -> Option<Roles> {
        let c = self.checker;
        let source = c.source_unit;
        let ast = c.parsed.ast();
        if !c.parsed.diagnostics().is_empty() {
            return None;
        }
        let function = match (c.parsed.roots(), wrapper) {
            ([function], None) => function,
            ([function, entry], Some(w))
                if w.callee == crate::type_checking::UnitItemId::new(source, *function)
                    && w.entry == crate::type_checking::UnitItemId::new(source, *entry) =>
            {
                function
            }
            _ => return None,
        };
        let Item::Function {
            extension_receiver: None,
            type_parameters,
            parameters,
            form: FunctionForm::ImplicitUnitBlock(body),
            ..
        } = ast.items().get(*function).ok()?.payload()
        else {
            return None;
        };
        if !type_parameters.is_empty() {
            return None;
        }
        let symbol_at = |span| c.symbols_by_span.get(&span_key(span)).copied();
        let reference = |id: ExpressionId| {
            let node = ast.expressions().get(id).ok()?;
            if !matches!(node.payload(), Expression::Name) {
                return None;
            }
            c.reference_symbol(node.span())
        };
        let ty = |id, expected| {
            c.typed
                .symbol_type(id)
                .and_then(|id| c.typed.types().get(id))
                == Some(&UnitTypeKind::Builtin(expected))
        };
        let expr_ty = |id: ExpressionId, expected| {
            c.typed
                .expression_type(UnitExpressionId::new(source, id))
                .and_then(|id| c.typed.types().get(id))
                == Some(&UnitTypeKind::Builtin(expected))
        };
        let ps = parameters
            .iter()
            .map(|p| {
                let NameMarker::Present(span) = p.name else {
                    return None;
                };
                if p.mode_marker.is_some() {
                    return None;
                }
                let symbol = symbol_at(span)?;
                ty(symbol, BuiltinType::Int).then_some(symbol)
            })
            .collect::<Option<Vec<_>>>()?;
        let parameters = match ps.as_slice() {
            [a, b] if a != b => [*a, *b],
            _ => return None,
        };
        let block = |id| match ast.statements().get(id).ok()?.payload() {
            Statement::Block { elements } | Statement::ControlBody { elements } => {
                Some(elements.as_slice())
            }
            _ => None,
        };
        let (a, b, outer, parent, root_suffix, suffix) = match block(*body)? {
            [a, b, n, p, r, s] => (*a, *b, *n, *p, Some(*r), *s),
            _ => return None,
        };
        let while_parts = |id| match ast.statements().get(id).ok()?.payload() {
            Statement::While {
                condition, body, ..
            } => Some((*condition, *body)),
            _ => None,
        };
        let (parent_condition, parent_body) = while_parts(parent)?;
        let [inner, child, parent_old, parent_step] = block(parent_body)? else {
            return None;
        };
        let (child_condition, child_body) = while_parts(*child)?;
        let [old, child_step] = block(child_body)? else {
            return None;
        };
        let binding = |id, kind| {
            let Statement::LocalVariable { declaration } = ast.statements().get(id).ok()?.payload()
            else {
                return None;
            };
            let Item::Variable {
                name: NameMarker::Present(span),
                kind: actual,
                initializer,
                type_ref: None,
                ..
            } = ast.items().get(*declaration).ok()?.payload()
            else {
                return None;
            };
            if *actual != kind {
                return None;
            }
            Some(Binding {
                item: *declaration,
                initializer: *initializer,
                symbol: symbol_at(*span)?,
                span: *span,
            })
        };
        let bindings = [
            binding(a, VariableKind::Var)?,
            binding(b, VariableKind::Var)?,
            binding(outer, VariableKind::Var)?,
            binding(*parent_old, VariableKind::Val)?,
            binding(*inner, VariableKind::Var)?,
            binding(*old, VariableKind::Val)?,
        ];
        for (i, b) in bindings.iter().enumerate() {
            if bindings[..i].iter().any(|p| p.symbol == b.symbol)
                || parameters.contains(&b.symbol)
                || !ty(
                    b.symbol,
                    if [2, 4].contains(&i) {
                        BuiltinType::Int
                    } else {
                        BuiltinType::String
                    },
                )
            {
                return None;
            }
        }
        if reference(bindings[2].initializer)? != parameters[0]
            || reference(bindings[4].initializer)? != parameters[1]
        {
            return None;
        }
        let mut expressions = vec![bindings[2].initializer, bindings[4].initializer];
        let string = |id| {
            expr_ty(id, BuiltinType::String)
                && matches!(ast.expressions().get(id).ok().map(|n| n.payload()),
        Some(Expression::String { parts }) if parts.iter().all(|p| matches!(p,StringPart::Text(_))))
        };
        for b in &bindings[..2] {
            if !string(b.initializer) {
                return None;
            }
            expressions.push(b.initializer);
        }
        let statement_expr = |id| match ast.statements().get(id).ok()?.payload() {
            Statement::Expression { expression } => Some(*expression),
            _ => None,
        };
        let integer = |id, text| {
            expr_ty(id, BuiltinType::Int)
                && ast.expressions().get(id).ok().is_some_and(|n| {
                    matches!(
                        n.payload(),
                        Expression::Literal(LiteralKind::Integer(IntegerLiteralKind::Unsuffixed))
                    ) && c.sources.slice(n.span()).ok() == Some(text)
                })
        };
        let steps = [statement_expr(*parent_step)?, statement_expr(*child_step)?];
        for (condition, step, counter) in [
            (parent_condition, steps[0], bindings[2].symbol),
            (child_condition, steps[1], bindings[4].symbol),
        ] {
            let Expression::Binary {
                left,
                operator: BinaryOperator::Greater,
                right,
                ..
            } = ast.expressions().get(condition).ok()?.payload()
            else {
                return None;
            };
            if reference(*left)? != counter
                || !integer(*right, "0")
                || !expr_ty(condition, BuiltinType::Boolean)
            {
                return None;
            }
            expressions.extend([condition, *left, *right]);
            let Expression::Assignment {
                target,
                operator: AssignmentOperator::SubtractAssign,
                value,
                ..
            } = ast.expressions().get(step).ok()?.payload()
            else {
                return None;
            };
            if reference(*target)? != counter || !integer(*value, "1") {
                return None;
            }
            expressions.extend([step, *target, *value]);
        }
        let mut incoming = Vec::new();
        for b in [bindings[3], bindings[5]] {
            let Expression::Call {
                callee,
                arguments,
                type_arguments,
                ..
            } = ast.expressions().get(b.initializer).ok()?.payload()
            else {
                return None;
            };
            let [root, value] = arguments.as_slice() else {
                return None;
            };
            if !type_arguments.is_empty()
                || root.named_prefix.is_some()
                || value.named_prefix.is_some()
                || !matches!(root.mode_marker, Some(ParameterModeMarker::Inout(_)))
                || value.mode_marker.is_some()
                || reference(root.value)? != bindings[0].symbol
                || !string(value.value)
                || !matches!(
                    ast.expressions().get(*callee).ok()?.payload(),
                    Expression::Name
                )
            {
                return None;
            }
            incoming.push(value.value);
            expressions.extend([b.initializer, *callee, root.value, value.value]);
        }
        let suffix = statement_expr(suffix)?;
        let final_use = match root_suffix {
            Some(id) => Some(statement_expr(id)?),
            None => None,
        };
        for (call, root) in final_use
            .into_iter()
            .map(|call| (call, bindings[0].symbol))
            .chain(std::iter::once((suffix, bindings[1].symbol)))
        {
            let Expression::Call {
                callee,
                arguments,
                type_arguments,
                ..
            } = ast.expressions().get(call).ok()?.payload()
            else {
                return None;
            };
            let [arg] = arguments.as_slice() else {
                return None;
            };
            let typed = c.typed.call(UnitExpressionId::new(source, call))?;
            if !type_arguments.is_empty()
                || arg.mode_marker.is_some()
                || arg.named_prefix.is_some()
                || reference(arg.value)? != root
                || !typed.prints_line()
                || typed.aborts()
                || typed.receiver().is_some()
                || c.typed.types().get(typed.return_type())
                    != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                || !matches!(
                    ast.expressions().get(*callee).ok()?.payload(),
                    Expression::Name
                )
            {
                return None;
            }
            expressions.extend([call, *callee, arg.value]);
        }
        let mut items = vec![*function];
        items.extend(bindings.iter().map(|b| b.item));
        let mut statements = vec![
            *body,
            a,
            b,
            outer,
            parent,
            parent_body,
            *parent_old,
            *inner,
            *child,
            *parent_step,
            child_body,
            *old,
            *child_step,
        ];
        // Recover the suffix statement roles, not expression IDs cast into statements.
        match block(*body)? {
            [_, _, _, _, r, s] => statements.extend([*r, *s]),
            _ => return None,
        }
        if let Some(w) = wrapper {
            if items.len() != 7 || statements.len() != 15 || expressions.len() != 30 {
                return None;
            }
            items.extend(w.items.iter().copied());
            statements.extend(w.statements.iter().copied());
            expressions.extend(w.expressions.iter().copied());
        }
        if ast.items().iter().count() != items.len()
            || ast.statements().iter().count() != statements.len()
            || ast.expressions().iter().count() != expressions.len()
            || ast
                .items()
                .iter()
                .any(|(id, _)| items.iter().filter(|v| **v == id).count() != 1)
            || ast
                .statements()
                .iter()
                .any(|(id, _)| statements.iter().filter(|v| **v == id).count() != 1)
            || ast
                .expressions()
                .iter()
                .any(|(id, _)| expressions.iter().filter(|v| **v == id).count() != 1)
        {
            return None;
        }
        Some(Roles {
            function: *function,
            bindings,
            parameters,
            controls: [parent, *child],
            conditions: [parent_condition, child_condition],
            steps,
            incoming: incoming.try_into().ok()?,
            suffix,
            final_use,
        })
    }
}
