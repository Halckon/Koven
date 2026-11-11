//! Narrow Partial evidence for two actual same-source branch returns.
use super::borrow_forward_transport::ForwardScope;
use super::*;
use crate::ownership_checking::{
    UnitBorrowBranchForwardTopology, UnitLoanForwardBranch, UnitLoanTarget,
};
use crate::type_checking::{BorrowReturnOrigin, UnitCallTarget, UnitItemId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BranchForwardPlan {
    pub(super) topology: UnitBorrowBranchForwardTopology,
    pub(super) split: UnitExpressionId,
    pub(super) condition: UnitExpressionId,
    pub(super) flag: UnitSymbolId,
    pub(super) scopes: [ForwardScope; 2],
    pub(super) coverage: Option<usize>,
}

impl DropPlanner<'_, '_> {
    pub(super) fn branch_forward_plan(
        &self,
        item: ItemId,
        block: StatementId,
    ) -> Option<BranchForwardPlan> {
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
        let [source, flag] = parameters.as_slice() else {
            return None;
        };
        if !type_parameters.is_empty()
            || extension_receiver.is_some()
            || self.checker.receiver_context(*name).is_some()
        {
            return None;
        }
        let root = *self.checker.marker_symbol(source.name)?;
        let flag_root = *self.checker.marker_symbol(flag.name)?;
        let symbol = *self.checker.marker_symbol(*name)?;
        let signature = self
            .checker
            .typed
            .signatures()
            .declarations()
            .iter()
            .find(|d| d.symbol() == symbol)?
            .callable()?;
        let [first, second] = signature.parameters() else {
            return None;
        };
        if signature.receiver().is_some()
            || !signature.type_parameters().is_empty()
            || first.mode() != ParameterMode::Borrow
            || second.mode() != ParameterMode::Borrow
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
        let (conditional, tail) = match elements.as_slice() {
            [conditional, tail] => (conditional, Some(*tail)),
            [conditional] => (conditional, None),
            _ => return None,
        };
        let Statement::Expression { expression: split } =
            ast.statements().get(*conditional).ok()?.payload()
        else {
            return None;
        };
        let Expression::If {
            condition,
            then_branch,
            else_branch,
            ..
        } = ast.expressions().get(*split).ok()?.payload()
        else {
            return None;
        };
        let (topology, second_return) = match (tail, else_branch) {
            (Some(tail), None) => (UnitBorrowBranchForwardTopology::TailContinuation, tail),
            (None, Some(other)) => {
                let (Statement::Block { elements } | Statement::ControlBody { elements }) =
                    ast.statements().get(*other).ok()?.payload()
                else {
                    return None;
                };
                let [returned] = elements.as_slice() else {
                    return None;
                };
                (UnitBorrowBranchForwardTopology::BothArmsReturn, *returned)
            }
            _ => return None,
        };
        if !matches!(
            ast.expressions().get(*condition).ok()?.payload(),
            Expression::Name
        ) || self
            .checker
            .typed
            .expression_type(self.checker.unit_expression(*condition))
            != Some(second.ty())
            || !self
                .checker
                .place(*condition)
                .ok()?
                .is_some_and(|p| p.is_root() && p.root() == flag_root)
        {
            return None;
        }
        let (Statement::Block { elements } | Statement::ControlBody { elements }) =
            ast.statements().get(*then_branch).ok()?.payload()
        else {
            return None;
        };
        let [then_return] = elements.as_slice() else {
            return None;
        };
        let callable = UnitItemId::new(self.checker.source_unit, item);
        let mut scopes = Vec::new();
        let mut targets = Vec::new();
        for (arm, statement) in [*then_return, second_return].into_iter().enumerate() {
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
            let branch = UnitLoanForwardBranch {
                callable,
                split: self.checker.unit_expression(*split),
                condition: self.checker.unit_expression(*condition),
                then_arm: arm == 0,
                control,
                value: body,
            };
            scopes.push(ForwardScope {
                region: None,
                callable,
                body,
                control,
                argument,
                root,
                branch: Some(branch),
                branch_entry: None,
            });
            targets.push(target);
        }
        if targets[0] != targets[1]
            || scopes[0].body == scopes[1].body
            || scopes[0].control == scopes[1].control
        {
            return None;
        }
        Some(BranchForwardPlan {
            topology,
            split: self.checker.unit_expression(*split),
            condition: self.checker.unit_expression(*condition),
            scopes: [scopes[0], scopes[1]],
            flag: flag_root,
            coverage: None,
        })
    }

    /// Called only after the real condition evaluation; unsupported/nested splits revoke evidence.
    pub(super) fn prepare_forward_split(
        &self,
        id: ExpressionId,
        condition: ExpressionId,
        outcomes: (bool, bool),
        state: &mut ValueState,
    ) {
        if !state.branch_forward_plan.is_some_and(|p| {
            p.split == self.checker.unit_expression(id)
                && p.condition == self.checker.unit_expression(condition)
                && outcomes == (true, true)
                && state.forward_scope.is_none()
        }) {
            state.branch_forward_plan = None;
            if state.forward_scope.is_some_and(|s| s.branch.is_some()) {
                state.forward_scope = None;
            }
        }
    }

    /// This registration precedes evaluation of the arm's actual source acquisition.
    pub(super) fn enter_forward_arm(
        &mut self,
        id: ExpressionId,
        then_arm: bool,
        state: &mut ValueState,
    ) {
        let Some(plan) = state
            .branch_forward_plan
            .filter(|p| p.split == self.checker.unit_expression(id))
        else {
            return;
        };
        let mut scope = plan.scopes[usize::from(!then_arm)];
        scope.branch_entry = self
            .iteration_actions
            .enter_forward_branch(scope.branch.expect("planned arm"));
        state.forward_scope = scope.branch_entry.map(|_| scope);
        state.branch_forward_plan = None;
        if let Some(candidate) = plan.coverage {
            let clear =
                self.iteration_actions
                    .branch_base(candidate)
                    .is_some_and(|(depth, owner)| {
                        self.branch_boundary_clear(
                            state,
                            Some(scope),
                            None,
                            false,
                            depth + 1,
                            owner + 1,
                        )
                    });
            self.iteration_actions.branch_arm(candidate, scope, clear);
        }
    }

    pub(super) fn prepare_forward_return(
        &self,
        id: ExpressionId,
        value: Option<ExpressionId>,
        state: &mut ValueState,
    ) {
        if state.forward_scope.is_some_and(|scope| {
            scope.branch.is_some()
                && (scope.control != self.checker.unit_expression(id)
                    || value.map(|v| self.checker.unit_expression(v)) != Some(scope.body))
        }) {
            state.forward_scope = None;
        }
        state.branch_forward_plan = None;
    }
}
