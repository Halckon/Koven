//! The bounded named Range loop that can reach its final ordinary observer.
use super::*;
use crate::{
    parser::{NameMarker, VariableKind},
    type_checking::{
        BorrowReturnOrigin, BuiltinType, CallableResultSource, ParameterMode, UnitCallTarget,
        UnitTypeKind,
    },
};

#[derive(Clone, Copy)]
pub(super) struct NamedRangeLoopShape {
    pub(super) provider: StatementId,
    pub(super) source: ExpressionId,
    pub(super) observe: StatementId,
    pub(super) binding: crate::name_resolution::UnitSymbolId,
    pub(super) root: crate::name_resolution::UnitSymbolId,
    pub(super) body_control: ExpressionId,
    pub(super) consume: ExpressionId,
    pub(super) moved: ExpressionId,
}

impl Checker<'_> {
    pub(super) fn closed_range_for_reuse(
        &self,
        elements: &[StatementId],
    ) -> Option<NamedRangeLoopShape> {
        let [owner, binding, choice, observe, consume] = elements else {
            return None;
        };
        let ast = self.parsed.ast();
        let expr = |statement| match ast.statements().get(statement).ok()?.payload() {
            Statement::Expression { expression } => Some(*expression),
            _ => None,
        };
        let declaration = |statement| match ast.statements().get(statement).ok()?.payload() {
            Statement::LocalVariable { declaration } => ast.items().get(*declaration).ok(),
            _ => None,
        };
        let Item::Variable {
            kind: VariableKind::BorrowVal(_),
            name: NameMarker::Present(marker),
            initializer,
            ..
        } = declaration(*binding)?.payload()
        else {
            return None;
        };
        let named = *self.marker_symbol(NameMarker::Present(*marker))?;
        let builtin =
            |ty, expected| self.typed.types().get(ty) == Some(&UnitTypeKind::Builtin(expected));
        let ordinary_call = |id| {
            self.typed.call(self.unit_expression(id)).filter(|call| {
                !call.aborts()
                    && call.receiver().is_none()
                    && matches!(call.target(), UnitCallTarget::Declaration(_))
            })
        };
        let producer = ordinary_call(*initializer)?;
        let CallableResultSource::Carrier(contract) = producer.result_source() else {
            return None;
        };
        let Expression::Call { arguments, .. } =
            ast.expressions().get(*initializer).ok()?.payload()
        else {
            return None;
        };
        let [source, ..] = arguments.as_slice() else {
            return None;
        };
        let source_node = ast.expressions().get(source.value).ok()?;
        let root = self.reference_symbol(source_node.span())?;
        if contract.origin() != BorrowReturnOrigin::Parameter(0)
            || contract.from_span().is_none()
            || !matches!(source_node.payload(), Expression::Name)
            || !matches!(producer.arguments().first(), Some(map) if map.mode() == ParameterMode::Borrow
                && map.argument_index() == 0 && map.parameter_index() == 0 && !map.crosses_thread())
        {
            return None;
        }
        let single_arg = |id, mode| {
            let call = ordinary_call(id)?;
            let [map] = call.arguments() else {
                return None;
            };
            let Expression::Call { arguments, .. } = ast.expressions().get(id).ok()?.payload()
            else {
                return None;
            };
            let [arg] = arguments.as_slice() else {
                return None;
            };
            (builtin(call.return_type(), BuiltinType::Unit)
                && call.result_source() == CallableResultSource::Owned
                && map.mode() == mode
                && map.parameter_index() == 0
                && map.argument_index() == 0
                && !map.crosses_thread()
                && arg.mode_marker.is_none()
                && arg.named_prefix.is_none())
            .then_some((arg.value, map.parameter_type()))
        };
        let (read, _) = single_arg(expr(*observe)?, ParameterMode::Borrow)?;
        let read_node = ast.expressions().get(read).ok()?;
        if !matches!(read_node.payload(), Expression::Name)
            || self.reference_symbol(read_node.span()) != Some(named)
        {
            return None;
        }
        let consume = expr(*consume)?;
        let (moved, _) = single_arg(consume, ParameterMode::Value)?;
        let moved_node = ast.expressions().get(moved).ok()?;
        if self.reference_symbol(moved_node.span()) != Some(root)
            || !matches!(moved_node.payload(), Expression::Name)
        {
            return None;
        }
        use crate::parser::{IntegerLiteralKind, LiteralKind};
        let [_, count] = arguments.as_slice() else {
            return None;
        };
        if !matches!(
            ast.expressions().get(count.value).ok()?.payload(),
            Expression::Literal(LiteralKind::Integer(IntegerLiteralKind::Unsuffixed))
        ) {
            return None;
        }
        let Item::Variable {
            kind: VariableKind::Val,
            name,
            initializer: owner_init,
            ..
        } = declaration(*owner)?.payload()
        else {
            return None;
        };
        if self.marker_symbol(*name).copied() != Some(root)
            || self
                .typed
                .container_construction(self.unit_expression(*owner_init))
                .is_none()
        {
            return None;
        }
        let Statement::For { source, body, .. } = ast.statements().get(*choice).ok()?.payload()
        else {
            return None;
        };
        let source_node = ast.expressions().get(*source).ok()?;
        if !matches!(source_node.payload(), Expression::Name)
            || self.reference_symbol(source_node.span()) != Some(named)
        {
            return None;
        }
        let statement = crate::type_checking::UnitStatementId::new(self.source_unit, *choice);
        let typed = self.typed.sequential_iteration(statement)?;
        let crate::type_checking::UnitSequentialIterationBinding::Name(item) = typed.binding()
        else {
            return None;
        };
        if typed.source() != self.unit_expression(*source)
            || typed.provider() != crate::type_checking::IterationProvider::RangeView
            || !builtin(typed.element_type(), BuiltinType::Int)
        {
            return None;
        }
        let body_control = self.closed_range_for_body(*body, *item)?;
        Some(NamedRangeLoopShape {
            provider: *choice,
            source: *source,
            observe: *observe,
            binding: named,
            root,
            body_control,
            consume,
            moved,
        })
    }

    fn closed_range_for_body(
        &self,
        body: StatementId,
        item: crate::name_resolution::UnitSymbolId,
    ) -> Option<ExpressionId> {
        use crate::parser::{BinaryOperator, IntegerLiteralKind, LiteralKind, StringPart};
        let ast = self.parsed.ast();
        let classify = || -> Option<ExpressionId> {
            let single = |id| {
                let (Statement::Block { elements } | Statement::ControlBody { elements }) =
                    ast.statements().get(id).ok()?.payload()
                else {
                    return None;
                };
                let [statement] = elements.as_slice() else {
                    return None;
                };
                let Statement::Expression { expression } =
                    ast.statements().get(*statement).ok()?.payload()
                else {
                    return None;
                };
                Some(*expression)
            };
            let control = single(body)?;
            let Expression::If {
                condition,
                then_branch,
                else_branch: None,
                ..
            } = ast.expressions().get(control).ok()?.payload()
            else {
                return None;
            };
            let Expression::Binary {
                left,
                right,
                operator: BinaryOperator::Equal,
                ..
            } = ast.expressions().get(*condition).ok()?.payload()
            else {
                return None;
            };
            let lhs = ast.expressions().get(*left).ok()?;
            if !matches!(lhs.payload(), Expression::Name)
                || self.reference_symbol(lhs.span()) != Some(item)
                || !matches!(
                    ast.expressions().get(*right).ok()?.payload(),
                    Expression::Literal(LiteralKind::Integer(IntegerLiteralKind::Unsuffixed))
                )
            {
                return None;
            }
            let print = single(*then_branch)?;
            let Expression::Call { arguments, .. } = ast.expressions().get(print).ok()?.payload()
            else {
                return None;
            };
            let [literal] = arguments.as_slice() else {
                return None;
            };
            let Expression::String { parts } = ast.expressions().get(literal.value).ok()?.payload()
            else {
                return None;
            };
            (self.typed.call(self.unit_expression(print))?.prints_line()
                && parts.iter().all(|part| matches!(part, StringPart::Text(_))))
            .then_some(control)
        };
        classify()
    }
}
