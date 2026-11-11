//! Strict declaration admission for one explicit nested side, never a completion certificate.
use super::*;
use crate::ownership_checking::{UnitBorrowNestedForwardTopology, UnitLoanTarget};
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
    pub(super) topology: UnitBorrowNestedForwardTopology,
    pub(super) callable: UnitItemId,
    pub(super) root: UnitSymbolId,
    pub(super) tree: FixedNestedTree,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FixedNestedTree {
    Three {
        splits: [NestedSplit; 2],
        leaves: [NestedLeaf; 3],
    },
    BalancedFour {
        splits: [NestedSplit; 3],
        leaves: [NestedLeaf; 4],
    },
}
impl NestedForwardPlan {
    pub(super) fn topology_matches_tree(&self) -> bool {
        matches!(
            (self.topology, self.tree),
            (
                UnitBorrowNestedForwardTopology::NestedThen
                    | UnitBorrowNestedForwardTopology::NestedElse,
                FixedNestedTree::Three { .. }
            ) | (
                UnitBorrowNestedForwardTopology::BalancedBoth,
                FixedNestedTree::BalancedFour { .. }
            )
        )
    }
    pub(super) fn splits(&self) -> &[NestedSplit] {
        match &self.tree {
            FixedNestedTree::Three { splits, .. } => splits,
            FixedNestedTree::BalancedFour { splits, .. } => splits,
        }
    }
    pub(super) fn leaves(&self) -> &[NestedLeaf] {
        match &self.tree {
            FixedNestedTree::Three { leaves, .. } => leaves,
            FixedNestedTree::BalancedFour { leaves, .. } => leaves,
        }
    }
    #[cfg(test)]
    pub(super) fn splits_mut(&mut self) -> &mut [NestedSplit] {
        match &mut self.tree {
            FixedNestedTree::Three { splits, .. } => splits,
            FixedNestedTree::BalancedFour { splits, .. } => splits,
        }
    }
    #[cfg(test)]
    pub(super) fn leaves_mut(&mut self) -> &mut [NestedLeaf] {
        match &mut self.tree {
            FixedNestedTree::Three { leaves, .. } => leaves,
            FixedNestedTree::BalancedFour { leaves, .. } => leaves,
        }
    }
    pub(super) fn split_depth(&self, index: usize) -> usize {
        usize::from(index != 0)
    }
    pub(super) fn parent_side(&self, index: usize) -> Option<bool> {
        match (self.topology, index) {
            (UnitBorrowNestedForwardTopology::NestedThen, 1) => Some(true),
            (UnitBorrowNestedForwardTopology::NestedElse, 1) => Some(false),
            (UnitBorrowNestedForwardTopology::BalancedBoth, 1) => Some(true),
            (UnitBorrowNestedForwardTopology::BalancedBoth, 2) => Some(false),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NestedForwardState {
    pub(super) plan: NestedForwardPlan,
    pub(super) path: [Option<usize>; 2],
    pub(super) leaf: Option<usize>,
    pub(super) ready_split: Option<usize>,
    pub(super) coverage: Option<usize>,
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
        let [source, flags @ ..] = parameters.as_slice() else {
            return None;
        };
        if !type_parameters.is_empty()
            || extension_receiver.is_some()
            || self.checker.receiver_context(*name).is_some()
        {
            return None;
        }
        let root = *self.checker.marker_symbol(source.name)?;
        if !(2..=3).contains(&flags.len()) {
            return None;
        }
        let flags = flags
            .iter()
            .map(|p| self.checker.marker_symbol(p.name).copied())
            .collect::<Option<Vec<_>>>()?;
        let symbol = *self.checker.marker_symbol(*name)?;
        let signature = self
            .checker
            .typed
            .signatures()
            .declarations()
            .iter()
            .find(|d| d.symbol() == symbol)?
            .callable()?;
        let [first, second, remaining @ ..] = signature.parameters() else {
            return None;
        };
        if signature.receiver().is_some()
            || !signature.type_parameters().is_empty()
            || first.mode() != ParameterMode::Borrow
            || second.mode() != ParameterMode::Borrow
            || remaining.len() + 1 != flags.len()
            || remaining
                .iter()
                .any(|p| p.mode() != ParameterMode::Borrow || p.ty() != second.ty())
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
        let (outer_split, outer_condition, then_statement, else_statement) =
            self.nested_split_shape(*outer_statement)?;
        let (topology, split_shapes, returned) = match (
            self.nested_split_shape(then_statement),
            self.nested_split_shape(else_statement),
            flags.len(),
        ) {
            (Some((split, condition, left, middle)), None, 2) => (
                UnitBorrowNestedForwardTopology::NestedThen,
                vec![(outer_split, outer_condition), (split, condition)],
                vec![left, middle, else_statement],
            ),
            (None, Some((split, condition, middle, right)), 2) => (
                UnitBorrowNestedForwardTopology::NestedElse,
                vec![(outer_split, outer_condition), (split, condition)],
                vec![then_statement, middle, right],
            ),
            (Some((left, lc, a, b)), Some((right, rc, c, d)), 3) => (
                UnitBorrowNestedForwardTopology::BalancedBoth,
                vec![(outer_split, outer_condition), (left, lc), (right, rc)],
                vec![a, b, c, d],
            ),
            _ => return None,
        };
        let mut splits = Vec::new();
        for ((split, condition), flag) in split_shapes.into_iter().zip(flags) {
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
        for (leaf, statement) in returned.into_iter().enumerate() {
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
            let path = topology.leaf_path(leaf);
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
            topology,
            callable,
            root,
            tree: match topology {
                UnitBorrowNestedForwardTopology::BalancedBoth => FixedNestedTree::BalancedFour {
                    splits: splits.try_into().ok()?,
                    leaves: leaves.try_into().ok()?,
                },
                _ => FixedNestedTree::Three {
                    splits: splits.try_into().ok()?,
                    leaves: leaves.try_into().ok()?,
                },
            },
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
        let expected =
            match (nested.path, nested.leaf, nested.ready_split) {
                ([None, None], None, None) => Some(0),
                ([Some(parent), None], None, None) => nested
                    .plan
                    .splits()
                    .iter()
                    .enumerate()
                    .find_map(|(i, split)| {
                        (split.split == self.checker.unit_expression(id)
                            && self.iteration_actions.nested_entry_matches(
                                parent,
                                nested.plan.callable,
                                nested.plan.topology,
                                nested.plan.splits()[0].split,
                                nested.plan.parent_side(i)?,
                                None,
                            ))
                        .then_some(i)
                    }),
                _ => None,
            };
        let Some(index) = expected.filter(|&index| {
            let split = nested.plan.splits()[index];
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
            .filter(|&i| nested.plan.splits()[i].split == self.checker.unit_expression(id))
        else {
            state.nested_forward = None;
            return;
        };
        let split = nested.plan.splits()[index];
        let parent = if index == 0 { None } else { nested.path[0] };
        let Some(entry) = self.iteration_actions.enter_nested_forward_edge(
            nested.plan.callable,
            nested.plan.topology,
            split.split,
            split.condition,
            then_arm,
            parent,
        ) else {
            state.nested_forward = None;
            return;
        };
        let depth = nested.plan.split_depth(index);
        nested.path[depth] = Some(entry);
        nested.ready_split = None;
        nested.leaf = nested.plan.leaves().iter().position(|leaf| {
            let depth = leaf.path.iter().flatten().count();
            depth == nested.plan.split_depth(index) + 1
                && leaf.path.iter().zip(nested.path).all(|(wanted, handle)| {
                    match (wanted, handle) {
                        (Some((split, side)), Some(handle)) => {
                            self.iteration_actions.nested_entry_matches(
                                handle,
                                nested.plan.callable,
                                nested.plan.topology,
                                nested.plan.splits()[*split].split,
                                *side,
                                if *split == 0 { None } else { nested.path[0] },
                            )
                        }
                        (None, None) => true,
                        _ => false,
                    }
                })
        });
        state.nested_forward = Some(nested);
        self.observe_nested_arm(state, index, then_arm, entry);
    }
}
