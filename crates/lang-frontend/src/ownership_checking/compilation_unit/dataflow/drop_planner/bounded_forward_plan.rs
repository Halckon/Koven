//! Strict bounded terminal declarations. This plan is not evidence that any edge occurred.
use super::*;
use crate::ownership_checking::UnitLoanTarget;
use crate::type_checking::{BorrowReturnOrigin, UnitCallTarget, UnitItemId, UnitTypeId};

#[derive(Clone, Debug)]
pub(super) struct Plan {
    pub(super) callable: UnitItemId,
    pub(super) root: UnitSymbolId,
    pub(super) nodes: Vec<Node>,
    pub(super) leaves: Vec<usize>,
    pub(super) split_count: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Node {
    pub(super) parent: Option<(usize, bool)>,
    pub(super) depth: usize,
    pub(super) kind: Kind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Split {
        split: UnitExpressionId,
        condition: UnitExpressionId,
        flag: UnitSymbolId,
        children: [usize; 2],
    },
    Leaf {
        control: UnitExpressionId,
        value: UnitExpressionId,
        argument: UnitExpressionId,
    },
}
impl Plan {
    /// Validate only the finite declaration graph; this never supplies execution evidence.
    pub(super) fn valid_structure(&self) -> bool {
        let unit = self.callable.source_unit();
        if self.root.source_unit() != unit
            || self.nodes.is_empty()
            || self.nodes.len() > 15
            || !(1..=7).contains(&self.split_count)
            || !(2..=8).contains(&self.leaves.len())
            || self.leaves.len() != self.split_count + 1
            || !matches!(
                self.nodes.first(),
                Some(Node {
                    parent: None,
                    depth: 0,
                    kind: Kind::Split { .. }
                })
            )
        {
            return false;
        }
        let mut visited = vec![false; self.nodes.len()];
        let mut pending = vec![(0, None, 0)];
        let mut leaves = Vec::new();
        let mut splits = Vec::new();
        let mut conditions = Vec::new();
        let mut flags = Vec::new();
        let mut controls = Vec::new();
        let mut values = Vec::new();
        while let Some((index, expected_parent, depth)) = pending.pop() {
            let Some(node) = self.nodes.get(index) else {
                return false;
            };
            if depth > 3 || visited[index] || node.parent != expected_parent || node.depth != depth
            {
                return false;
            }
            visited[index] = true;
            match node.kind {
                Kind::Split {
                    split,
                    condition,
                    flag,
                    children,
                } => {
                    if depth >= 3
                        || splits.len() >= 7
                        || split == condition
                        || split.source_unit() != unit
                        || condition.source_unit() != unit
                        || flag.source_unit() != unit
                        || flag == self.root
                        || splits.contains(&split)
                        || conditions.contains(&condition)
                        || splits.contains(&condition)
                        || conditions.contains(&split)
                        || flags.contains(&flag)
                        || children[0] == children[1]
                        || children.iter().any(|child| *child >= self.nodes.len())
                    {
                        return false;
                    }
                    splits.push(split);
                    conditions.push(condition);
                    flags.push(flag);
                    // Push right first, so the bounded stack yields left-to-right DFS leaf order.
                    pending.push((children[1], Some((index, false)), depth + 1));
                    pending.push((children[0], Some((index, true)), depth + 1));
                }
                Kind::Leaf {
                    control,
                    value,
                    argument,
                } => {
                    if leaves.len() >= 8
                        || control == value
                        || control.source_unit() != unit
                        || value.source_unit() != unit
                        || argument.source_unit() != unit
                        || controls.contains(&control)
                        || values.contains(&value)
                        || controls.contains(&value)
                        || values.contains(&control)
                    {
                        return false;
                    }
                    leaves.push(index);
                    controls.push(control);
                    values.push(value);
                }
            }
        }
        visited.into_iter().all(|seen| seen)
            && splits.len() == self.split_count
            && leaves == self.leaves
            && controls
                .iter()
                .chain(&values)
                .all(|e| !splits.contains(e) && !conditions.contains(e))
    }
}

struct Contract {
    root: UnitSymbolId,
    source_type: UnitTypeId,
    flag_type: UnitTypeId,
    flags: Vec<UnitSymbolId>,
}
#[derive(Default)]
struct Builder {
    // Reservations are private to admission and must all be filled before a Plan exists.
    nodes: Vec<Option<Node>>,
    leaves: Vec<usize>,
    splits: usize,
}
impl DropPlanner<'_, '_> {
    pub(super) fn bounded_forward_plan(&self, item: ItemId, block: StatementId) -> Option<Plan> {
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
            || !(1..=7).contains(&flags.len())
        {
            return None;
        }
        let root = *self.checker.marker_symbol(source.name)?;
        let flags = flags
            .iter()
            .map(|p| self.checker.marker_symbol(p.name).copied())
            .collect::<Option<Vec<_>>>()?;
        if flags.contains(&root)
            || flags
                .iter()
                .enumerate()
                .any(|(i, f)| flags[i + 1..].contains(f))
        {
            return None;
        }
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
            || first.symbol() != Some(root)
            || second.mode() != ParameterMode::Borrow
            || remaining.len() + 1 != flags.len()
            || remaining
                .iter()
                .any(|p| p.mode() != ParameterMode::Borrow || p.ty() != second.ty())
            || signature.parameters()[1..]
                .iter()
                .zip(&flags)
                .any(|(p, f)| p.symbol() != Some(*f))
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
        let [statement] = elements.as_slice() else {
            return None;
        };
        let contract = Contract {
            root,
            source_type: first.ty(),
            flag_type: second.ty(),
            flags,
        };
        let mut builder = Builder::default();
        let root_node = self.bounded_tree_node(*statement, None, 0, &contract, &mut builder)?;
        let nodes = builder.nodes.into_iter().collect::<Option<Vec<_>>>()?;
        if root_node != 0
            || !matches!(nodes.first()?.kind, Kind::Split { .. })
            || builder.splits != contract.flags.len()
            || builder.leaves.len() != builder.splits + 1
            || !(2..=8).contains(&builder.leaves.len())
        {
            return None;
        }
        let mut controls = Vec::new();
        let mut calls = Vec::new();
        for &index in &builder.leaves {
            let Kind::Leaf { control, value, .. } = nodes.get(index)?.kind else {
                return None;
            };
            if control == value || controls.contains(&control) || calls.contains(&value) {
                return None;
            }
            controls.push(control);
            calls.push(value);
        }
        let plan = Plan {
            callable: UnitItemId::new(self.checker.source_unit, item),
            root,
            nodes,
            leaves: builder.leaves,
            split_count: builder.splits,
        };
        plan.valid_structure().then_some(plan)
    }

    fn bounded_tree_node(
        &self,
        statement: StatementId,
        parent: Option<(usize, bool)>,
        depth: usize,
        contract: &Contract,
        builder: &mut Builder,
    ) -> Option<usize> {
        if depth > 3 || builder.nodes.len() >= 15 {
            return None;
        }
        let ast = self.checker.parsed.ast();
        let Statement::Expression { expression } = ast.statements().get(statement).ok()?.payload()
        else {
            return None;
        };
        let index = builder.nodes.len();
        builder.nodes.push(None);
        let kind = match ast.expressions().get(*expression).ok()?.payload() {
            Expression::If {
                condition,
                then_branch,
                else_branch: Some(other),
                ..
            } => {
                if depth >= 3 || builder.splits >= 7 {
                    return None;
                }
                let flag = *contract.flags.get(builder.splits)?;
                builder.splits += 1;
                if !matches!(
                    ast.expressions().get(*condition).ok()?.payload(),
                    Expression::Name
                ) || self
                    .checker
                    .typed
                    .expression_type(self.checker.unit_expression(*condition))
                    != Some(contract.flag_type)
                    || !self
                        .checker
                        .place(*condition)
                        .ok()?
                        .is_some_and(|p| p.is_root() && p.root() == flag)
                {
                    return None;
                }
                let left = self.bounded_tree_node(
                    self.bounded_sole_statement(*then_branch)?,
                    Some((index, true)),
                    depth + 1,
                    contract,
                    builder,
                )?;
                let right = self.bounded_tree_node(
                    self.bounded_sole_statement(*other)?,
                    Some((index, false)),
                    depth + 1,
                    contract,
                    builder,
                )?;
                Kind::Split {
                    split: self.checker.unit_expression(*expression),
                    condition: self.checker.unit_expression(*condition),
                    flag,
                    children: [left, right],
                }
            }
            Expression::Return {
                value: Some(value), ..
            } => {
                if builder.leaves.len() >= 8 {
                    return None;
                }
                let kind = self.bounded_leaf(*expression, *value, contract)?;
                builder.leaves.push(index);
                kind
            }
            _ => return None,
        };
        builder.nodes[index] = Some(Node {
            parent,
            depth,
            kind,
        });
        Some(index)
    }
    fn bounded_sole_statement(&self, body: StatementId) -> Option<StatementId> {
        let (Statement::Block { elements } | Statement::ControlBody { elements }) = self
            .checker
            .parsed
            .ast()
            .statements()
            .get(body)
            .ok()?
            .payload()
        else {
            return None;
        };
        let [statement] = elements.as_slice() else {
            return None;
        };
        Some(*statement)
    }
    fn bounded_leaf(
        &self,
        control: ExpressionId,
        value: ExpressionId,
        contract: &Contract,
    ) -> Option<Kind> {
        let ast = self.checker.parsed.ast();
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = ast.expressions().get(value).ok()?.payload()
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
                .is_some_and(|p| p.is_root() && p.root() == contract.root)
        {
            return None;
        }
        let body = self.checker.unit_expression(value);
        let argument = self.checker.unit_expression(argument.value);
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
            || parameter.ty() != contract.source_type
            || callee.return_type() != contract.source_type
            || callee.borrow_return()?.origin() != BorrowReturnOrigin::Parameter(0)
            || call.result_source() != callee.result_source()
            || call.return_type() != contract.source_type
            || actual.argument_index() != 0
            || actual.parameter_index() != 0
            || actual.mode() != ParameterMode::Borrow
            || actual.crosses_thread()
            || actual.parameter_type() != contract.source_type
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
        if !matches!(origins.next()?.origin(), UnitLoanTarget::Place(p) if p.is_root() && p.root() == contract.root)
            || origins.next().is_some()
        {
            return None;
        }
        Some(Kind::Leaf {
            control: self.checker.unit_expression(control),
            value: body,
            argument,
        })
    }
}
