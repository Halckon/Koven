//! Strict declaration admission for one outer-then nested split, never a completion certificate.
use super::*;
use crate::ownership_checking::UnitLoanTarget;
use crate::type_checking::{BorrowReturnOrigin, UnitCallTarget, UnitItemId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NestedSplit {
    pub(super) split: UnitExpressionId,
    pub(super) condition: UnitExpressionId,
    pub(super) flag: UnitSymbolId,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NestedLeaf {
    pub(super) control: UnitExpressionId,
    pub(super) value: UnitExpressionId,
    pub(super) argument: UnitExpressionId,
    pub(super) path: [Option<(usize, bool)>; 2],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NestedForwardPlan {
    pub(super) callable: UnitItemId,
    pub(super) root: UnitSymbolId,
    pub(super) splits: [NestedSplit; 2],
    pub(super) leaves: [NestedLeaf; 3],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NestedForwardState {
    pub(super) plan: NestedForwardPlan,
    pub(super) path: [Option<usize>; 2],
    pub(super) leaf: Option<usize>,
    pub(super) ready_split: Option<usize>,
}

impl DropPlanner<'_, '_> {
    pub(super) fn nested_forward_plan(
        &self,
        item: ItemId,
        block: StatementId,
    ) -> Option<NestedForwardPlan> {
        if !self.iteration_actions.enabled || !self.checker.parsed.roots().contains(&item) {
            return None;
        }
        let ast = self.checker.parsed.ast();
        let Item::Function {
            name,
            parameters,
            type_parameters,
            extension_receiver,
            ..
        } = ast.items().get(item).ok()?.payload()
        else {
            return None;
        };
        let [source, outer, inner] = parameters.as_slice() else {
            return None;
        };
        if !type_parameters.is_empty()
            || extension_receiver.is_some()
            || self.checker.receiver_context(*name).is_some()
        {
            return None;
        }
        let root = *self.checker.marker_symbol(source.name)?;
        let flags = [
            *self.checker.marker_symbol(outer.name)?,
            *self.checker.marker_symbol(inner.name)?,
        ];
        let symbol = *self.checker.marker_symbol(*name)?;
        let signature = self
            .checker
            .typed
            .signatures()
            .declarations()
            .iter()
            .find(|d| d.symbol() == symbol)?
            .callable()?;
        let [first, second, third] = signature.parameters() else {
            return None;
        };
        if signature.receiver().is_some()
            || !signature.type_parameters().is_empty()
            || first.mode() != ParameterMode::Borrow
            || second.mode() != ParameterMode::Borrow
            || third.mode() != ParameterMode::Borrow
            || third.ty() != second.ty()
            || self.checker.typed.types().get(first.ty())
                != Some(&UnitTypeKind::Builtin(BuiltinType::String))
            || self.checker.typed.types().get(second.ty())
                != Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
            || signature.return_type() != first.ty()
            || signature.borrow_return()?.origin() != BorrowReturnOrigin::Parameter(0)
        {
            return None;
        }
        let Statement::Block { elements } = ast.statements().get(block).ok()?.payload() else {
            return None;
        };
        let [outer_statement] = elements.as_slice() else {
            return None;
        };
        let (outer_split, outer_condition, nested_statement, outer_return) =
            self.nested_split_shape(*outer_statement)?;
        let (inner_split, inner_condition, left_return, middle_return) =
            self.nested_split_shape(nested_statement)?;
        let mut splits = Vec::new();
        for ((split, condition), flag) in [
            (outer_split, outer_condition),
            (inner_split, inner_condition),
        ]
        .into_iter()
        .zip(flags)
        {
            if !matches!(
                ast.expressions().get(condition).ok()?.payload(),
                Expression::Name
            ) || self
                .checker
                .typed
                .expression_type(self.checker.unit_expression(condition))
                != Some(second.ty())
                || !self
                    .checker
                    .place(condition)
                    .ok()?
                    .is_some_and(|p| p.is_root() && p.root() == flag)
            {
                return None;
            }
            splits.push(NestedSplit {
                split: self.checker.unit_expression(split),
                condition: self.checker.unit_expression(condition),
                flag,
            });
        }
        let callable = UnitItemId::new(self.checker.source_unit, item);
        let mut leaves = Vec::new();
        for (leaf, statement) in [left_return, middle_return, outer_return]
            .into_iter()
            .enumerate()
        {
            let Statement::Expression {
                expression: control,
            } = ast.statements().get(statement).ok()?.payload()
            else {
                return None;
            };
            let Expression::Return {
                value: Some(value), ..
            } = ast.expressions().get(*control).ok()?.payload()
            else {
                return None;
            };
            let Expression::Call {
                callee,
                arguments,
                type_arguments,
                ..
            } = ast.expressions().get(*value).ok()?.payload()
            else {
                return None;
            };
            let [argument] = arguments.as_slice() else {
                return None;
            };
            if !type_arguments.is_empty()
                || argument.named_prefix.is_some()
                || argument.mode_marker.is_some()
                || !matches!(
                    ast.expressions().get(*callee).ok()?.payload(),
                    Expression::Name
                )
                || !matches!(
                    ast.expressions().get(argument.value).ok()?.payload(),
                    Expression::Name
                )
                || !self
                    .checker
                    .place(argument.value)
                    .ok()?
                    .is_some_and(|p| p.is_root() && p.root() == root)
            {
                return None;
            }
            let body = self.checker.unit_expression(*value);
            let argument = self.checker.unit_expression(argument.value);
            let control = self.checker.unit_expression(*control);
            let call = self.checker.typed.call(body)?;
            let UnitCallTarget::Declaration(target) = call.target() else {
                return None;
            };
            let callee = self
                .checker
                .typed
                .signatures()
                .declaration(target)?
                .callable()?;
            let [parameter] = callee.parameters() else {
                return None;
            };
            let [actual] = call.arguments() else {
                return None;
            };
            if call.receiver().is_some()
                || call.aborts()
                || !call.instance().type_arguments().is_empty()
                || callee.receiver().is_some()
                || !callee.type_parameters().is_empty()
                || parameter.mode() != ParameterMode::Borrow
                || parameter.ty() != first.ty()
                || callee.return_type() != first.ty()
                || callee.borrow_return()?.origin() != BorrowReturnOrigin::Parameter(0)
                || call.result_source() != callee.result_source()
                || call.return_type() != first.ty()
                || actual.argument_index() != 0
                || actual.parameter_index() != 0
                || actual.mode() != ParameterMode::Borrow
                || actual.crosses_thread()
                || actual.parameter_type() != first.ty()
            {
                return None;
            }
            let mut sources = self
                .checker
                .borrow_results
                .forwarded_source_loans()
                .iter()
                .filter(|s| s.call() == body);
            if sources.next()?.argument() != argument || sources.next().is_some() {
                return None;
            }
            let mut origins = self
                .checker
                .borrow_return_origins
                .iter()
                .filter(|o| o.expression() == body);
            if !matches!(origins.next()?.origin(), UnitLoanTarget::Place(p) if p.is_root() && p.root() == root)
                || origins.next().is_some()
            {
                return None;
            }
            let path = match leaf {
                0 => [Some((0, true)), Some((1, true))],
                1 => [Some((0, true)), Some((1, false))],
                _ => [Some((0, false)), None],
            };
            leaves.push(NestedLeaf {
                control,
                value: body,
                argument,
                path,
            });
        }
        if leaves.iter().enumerate().any(|(i, left)| {
            leaves[i + 1..]
                .iter()
                .any(|right| left.control == right.control || left.value == right.value)
        }) {
            return None;
        }
        Some(NestedForwardPlan {
            callable,
            root,
            splits: [splits[0], splits[1]],
            leaves: [leaves[0], leaves[1], leaves[2]],
        })
    }

    // Exactly one statement per arm. This is admission only, never occurrence evidence.
    fn nested_split_shape(
        &self,
        statement: StatementId,
    ) -> Option<(ExpressionId, ExpressionId, StatementId, StatementId)> {
        let ast = self.checker.parsed.ast();
        let Statement::Expression { expression: split } =
            ast.statements().get(statement).ok()?.payload()
        else {
            return None;
        };
        let Expression::If {
            condition,
            then_branch,
            else_branch: Some(other),
            ..
        } = ast.expressions().get(*split).ok()?.payload()
        else {
            return None;
        };
        let sole = |body| {
            let (Statement::Block { elements } | Statement::ControlBody { elements }) =
                ast.statements().get(body).ok()?.payload()
            else {
                return None;
            };
            let [statement] = elements.as_slice() else {
                return None;
            };
            Some(*statement)
        };
        Some((*split, *condition, sole(*then_branch)?, sole(*other)?))
    }
}

impl DropPlanner<'_, '_> {
    // Admission identities are checked only after the actual dynamic condition completes.
    pub(super) fn prepare_nested_forward_split(
        &self,
        id: ExpressionId,
        condition: ExpressionId,
        outcomes: (bool, bool),
        state: &mut ValueState,
    ) {
        let Some(mut nested) = state.nested_forward else {
            return;
        };
        let expected = match (nested.path, nested.leaf, nested.ready_split) {
            ([None, None], None, None) => Some(0),
            ([Some(_), None], None, None) => Some(1),
            _ => None,
        };
        let Some(index) = expected.filter(|&index| {
            let split = nested.plan.splits[index];
            split.split == self.checker.unit_expression(id)
                && split.condition == self.checker.unit_expression(condition)
                && outcomes == (true, true)
        }) else {
            state.nested_forward = None;
            return;
        };
        nested.ready_split = Some(index);
        state.nested_forward = Some(nested);
    }

    // Called only on the actual reached arm, before any of its source acquisitions.
    pub(super) fn enter_nested_forward_arm(
        &mut self,
        id: ExpressionId,
        then_arm: bool,
        state: &mut ValueState,
    ) {
        let Some(mut nested) = state.nested_forward else {
            return;
        };
        let Some(index) = nested
            .ready_split
            .filter(|&i| nested.plan.splits[i].split == self.checker.unit_expression(id))
        else {
            state.nested_forward = None;
            return;
        };
        let split = nested.plan.splits[index];
        let parent = if index == 0 { None } else { nested.path[0] };
        let Some(entry) = self.iteration_actions.enter_nested_forward_edge(
            nested.plan.callable,
            split.split,
            split.condition,
            then_arm,
            parent,
        ) else {
            state.nested_forward = None;
            return;
        };
        nested.path[index] = Some(entry);
        nested.ready_split = None;
        nested.leaf = match (index, then_arm) {
            (0, true) => None,
            (0, false) => Some(2),
            (1, true) => Some(0),
            (1, false) => Some(1),
            _ => None,
        };
        state.nested_forward = Some(nested);
    }
}
