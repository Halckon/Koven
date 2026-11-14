//! Independent wrapper roles. Combined with the callee visit set, never a cropped ParsedFile.
use super::*;
use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    parser::{
        FunctionForm, IntegerLiteralKind, Item, LiteralKind, NameMarker, Statement, VariableKind,
    },
    type_checking::{ParameterMode, UnitCallTarget, UnitItemId},
};

pub(in super::super) struct NamedCoverage {
    pub(super) entry: UnitItemId,
    pub(super) callee: UnitItemId,
    pub(super) roots: [UnitSymbolId; 2],
    pub(super) initializers: [UnitExpressionId; 2],
    pub(super) call: UnitExpressionId,
    pub(super) arguments: [UnitExpressionId; 2],
    pub(super) items: Vec<ItemId>,
    pub(super) statements: Vec<StatementId>,
    pub(super) expressions: Vec<ExpressionId>,
}

impl DropPlanner<'_, '_> {
    pub(in super::super) fn named_wrapper_coverage(&self) -> Option<NamedCoverage> {
        let c = self.checker;
        let ast = c.parsed.ast();
        let [callee, entry] = c.parsed.roots() else {
            return None;
        };
        if !c.parsed.diagnostics().is_empty() {
            return None;
        }
        let Item::Function {
            extension_receiver: None,
            type_parameters,
            parameters,
            form: FunctionForm::ImplicitUnitBlock(body),
            ..
        } = ast.items().get(*entry).ok()?.payload()
        else {
            return None;
        };
        if !type_parameters.is_empty() || !parameters.is_empty() {
            return None;
        }
        let Statement::Block { elements } = ast.statements().get(*body).ok()?.payload() else {
            return None;
        };
        let [first, second, invocation] = elements.as_slice() else {
            return None;
        };
        let binding = |statement| {
            let Statement::LocalVariable { declaration } =
                ast.statements().get(statement).ok()?.payload()
            else {
                return None;
            };
            let Item::Variable {
                name: NameMarker::Present(span),
                kind: VariableKind::Val,
                initializer,
                type_ref: None,
                ..
            } = ast.items().get(*declaration).ok()?.payload()
            else {
                return None;
            };
            let symbol = *c.symbols_by_span.get(&span_key(*span))?;
            let node = ast.expressions().get(*initializer).ok()?;
            if !matches!(
                node.payload(),
                Expression::Literal(LiteralKind::Integer(IntegerLiteralKind::Unsuffixed))
            ) || c
                .typed
                .symbol_type(symbol)
                .and_then(|t| c.typed.types().get(t))
                != Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                || c.typed
                    .expression_type(c.unit_expression(*initializer))
                    .and_then(|t| c.typed.types().get(t))
                    != Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            {
                return None;
            }
            Some((*declaration, symbol, *initializer))
        };
        let p = binding(*first)?;
        let q = binding(*second)?;
        if p.1 == q.1 {
            return None;
        }
        let Statement::Expression { expression } =
            ast.statements().get(*invocation).ok()?.payload()
        else {
            return None;
        };
        let Expression::Call {
            callee: target,
            type_arguments,
            arguments,
            ..
        } = ast.expressions().get(*expression).ok()?.payload()
        else {
            return None;
        };
        if !type_arguments.is_empty()
            || arguments.len() != 2
            || !matches!(
                ast.expressions().get(*target).ok()?.payload(),
                Expression::Name
            )
        {
            return None;
        }
        let call = c.unit_expression(*expression);
        let typed_call = c.typed.call(call)?;
        let UnitCallTarget::Declaration(declaration) = typed_call.target() else {
            return None;
        };
        let declaration = c.names.index().declarations().get(declaration.index())?;
        if declaration.source_unit() != c.source_unit
            || declaration.root() != *callee
            || typed_call.receiver().is_some()
            || typed_call.aborts()
            || c.typed.types().get(typed_call.return_type())
                != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
            || typed_call.arguments().len() != 2
        {
            return None;
        }
        for (index, (argument, symbol)) in arguments.iter().zip([p.1, q.1]).enumerate() {
            let node = ast.expressions().get(argument.value).ok()?;
            let typed_argument = typed_call.arguments()[index];
            if argument.mode_marker.is_some()
                || argument.named_prefix.is_some()
                || !matches!(node.payload(), Expression::Name)
                || c.reference_symbol(node.span()) != Some(symbol)
                || typed_argument.argument_index() != index
                || typed_argument.parameter_index() != index
                || typed_argument.mode() != ParameterMode::Borrow
                || typed_argument.crosses_thread()
                || c.typed.types().get(typed_argument.parameter_type())
                    != Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            {
                return None;
            }
        }
        Some(NamedCoverage {
            entry: UnitItemId::new(c.source_unit, *entry),
            callee: UnitItemId::new(c.source_unit, *callee),
            roots: [p.1, q.1],
            initializers: [c.unit_expression(p.2), c.unit_expression(q.2)],
            call,
            arguments: [
                c.unit_expression(arguments[0].value),
                c.unit_expression(arguments[1].value),
            ],
            items: vec![*entry, p.0, q.0],
            statements: vec![*body, *first, *second, *invocation],
            expressions: vec![
                p.2,
                q.2,
                *expression,
                *target,
                arguments[0].value,
                arguments[1].value,
            ],
        })
    }
}
