//! 调用前缀持有的 loan 与 temporary 随分支状态流动。
use super::{
    CleanupConditionId, DropPlanner, LoanEndFact, LoanEndPoint, LoanTarget, NullableTemporary,
    OwnershipCheckingError, ValueState,
};
use crate::{
    ast::ExpressionId,
    name_resolution::SymbolId,
    ownership_checking::{CleanupOwnerValue, CleanupOwnerValueId, LoanFact},
    parser::{Expression, VariableKind},
    type_checking::{CallableTarget, Copyability, ParameterMode},
};

#[derive(Clone, Debug)]
pub(super) struct PendingCall {
    pub(super) call: ExpressionId,
    pub(super) loop_depth: usize,
    pub(super) loans: Vec<LoanFact>,
    pub(super) callees: Vec<SymbolId>,
    pub(super) closure_environment: Option<(CleanupOwnerValueId, Option<ExpressionId>)>,
}
impl PendingCall {
    pub(super) fn new(call: ExpressionId, loop_depth: usize) -> Self {
        Self {
            call,
            loop_depth,
            loans: Vec::new(),
            callees: Vec::new(),
            closure_environment: None,
        }
    }
}
impl DropPlanner<'_, '_> {
    /// Callable owners are held through argument evaluation even without a source-level call loan.
    pub(super) fn register_pending_callee(
        &mut self,
        call: ExpressionId,
        callee: ExpressionId,
        state: &mut ValueState,
    ) -> Result<(), OwnershipCheckingError> {
        let closures = std::mem::take(&mut state.result_closures);
        let versions = std::mem::take(&mut state.result_owners);
        // A mutable callee binding may be replaced by an argument after evaluation.
        // Until that evaluated value is retained independently, its owner is not a call-entry source.
        let stable_callee = matches!(
            self.checker
                .parsed
                .ast()
                .expressions()
                .get(callee)?
                .payload(),
            Expression::Name
        ) && self.checker.place(callee)?.is_some_and(|place| {
            self.checker.variable_kinds.get(&place.root()) != Some(&VariableKind::Var)
        });
        let callee_environment = if self
            .checker
            .typed
            .call(call)
            .is_some_and(|descriptor| descriptor.target() == CallableTarget::FunctionValue)
            && stable_callee
            && state.path != CleanupConditionId::NEVER
        {
            let closure = match (closures.as_slice(), versions.as_slice()) {
                ([origin], [version])
                    if origin.owner == version.owner
                        && self.conditions.and(state.path, version.condition) == state.path =>
                {
                    let definition = match self.conditions.owner_snapshot(version.owner) {
                        Some(snapshot) => match snapshot.capture_inputs() {
                            [input] => Some((input.owner(), input.condition())),
                            _ => None,
                        },
                        None => Some((version.owner, CleanupConditionId::ALWAYS)),
                    };
                    // 从唯一值来源证明 lambda 身份；origin 的清理 guard 可能已重绑为
                    // snapshot selector，不能要求当前控制路径直接蕴含该副本。
                    // Pass 在可达调用点执行，只要求当前路径保证值及来源可用。
                    definition
                        .filter(|(_, guard)| self.conditions.and(state.path, *guard) == state.path)
                        .map(|(owner, _)| owner)
                        .filter(|&owner| owner == origin.layout_owner)
                        .and_then(|owner| {
                            matches!(
                                self.conditions.owner_value(owner),
                                Some(CleanupOwnerValue::Closure { expression, .. })
                                    if *expression == origin.closure
                            )
                            .then_some(origin.closure)
                        })
                }
                _ => None,
            };
            // owner 的可用性与 lambda 的静态身份是两个证明。phi / nullable 提取
            // 可以保有唯一实际值而没有唯一静态 lambda，此时传值自身的环境。
            match versions.as_slice() {
                [version] if self.conditions.and(state.path, version.condition) == state.path => {
                    Some((version.owner, closure))
                }
                _ => None,
            }
        } else {
            None
        };
        if let Some(environment) = callee_environment
            && let Some(frame) = state
                .pending_calls
                .iter_mut()
                .rev()
                .find(|frame| frame.call == call)
        {
            frame.closure_environment = Some(environment);
        }
        if let Some(place) = self.checker.place(callee)? {
            if let Some(frame) = state
                .pending_calls
                .iter_mut()
                .rev()
                .find(|frame| frame.call == call)
            {
                frame.callees.push(place.root());
            }
        } else if self.is_move_only_temporary(callee)
            && self
                .checker
                .typed
                .call(call)
                .is_some_and(|descriptor| descriptor.target() == CallableTarget::FunctionValue)
        {
            // A static function target has no evaluated environment owner to drop.
            state.nullable_temporaries.push(NullableTemporary {
                versions,
                closures,
                transfers_at_call: false,
                control: call,
                subject: callee,
                origin: self.checker.parsed.ast().expressions().get(callee)?.span(),
                loop_depth: self.loop_boundaries.len(),
                prior_symbols: state.values.iter().map(|value| value.symbol).collect(),
            });
        }
        Ok(())
    }

    pub(super) fn register_pending_argument(
        &mut self,
        call: ExpressionId,
        argument: ExpressionId,
        mode: ParameterMode,
        state: &mut ValueState,
    ) -> Result<(), OwnershipCheckingError> {
        let closures = std::mem::take(&mut state.result_closures);
        let versions = std::mem::take(&mut state.result_owners);
        // Until the call actually happens, a consumed argument remains an evaluation obligation.
        if mode == ParameterMode::Value
            && self
                .checker
                .typed
                .expression_type(argument)
                .and_then(|ty| self.checker.typed.copyability(ty))
                == Some(Copyability::MoveOnly)
        {
            state.nullable_temporaries.push(NullableTemporary {
                versions: versions.clone(),
                closures: closures.clone(),
                transfers_at_call: true,
                control: call,
                subject: argument,
                origin: self
                    .checker
                    .parsed
                    .ast()
                    .expressions()
                    .get(argument)?
                    .span(),
                loop_depth: self.loop_boundaries.len(),
                prior_symbols: state.values.iter().map(|value| value.symbol).collect(),
            });
        }
        let loans = self
            .checker
            .loans
            .iter()
            .filter(|loan| loan.call() == call && loan.argument() == argument)
            .cloned()
            .collect::<Vec<_>>();
        for loan in &loans {
            if let LoanTarget::Temporary(subject) = loan.target()
                && self.is_move_only_temporary(*subject)
            {
                state.nullable_temporaries.push(NullableTemporary {
                    versions: versions.clone(),
                    closures: closures.clone(),
                    transfers_at_call: false,
                    control: call,
                    subject: *subject,
                    origin: self
                        .checker
                        .parsed
                        .ast()
                        .expressions()
                        .get(*subject)?
                        .span(),
                    loop_depth: self.loop_boundaries.len(),
                    prior_symbols: state.values.iter().map(|value| value.symbol).collect(),
                });
            }
        }
        if let Some(frame) = state
            .pending_calls
            .iter_mut()
            .rev()
            .find(|frame| frame.call == call)
        {
            frame.loans.extend(loans);
        }
        Ok(())
    }
    /// All selected loans end before any corresponding owner cleanup is published.
    pub(super) fn end_pending_calls(
        &mut self,
        point: LoanEndPoint,
        state: &mut ValueState,
        selected: impl Fn(&PendingCall) -> bool,
    ) -> Vec<SymbolId> {
        let mut roots = Vec::new();
        for index in (0..state.pending_calls.len()).rev() {
            if !selected(&state.pending_calls[index]) {
                continue;
            }
            let frame = state.pending_calls.remove(index);
            for root in frame.callees {
                if !roots.contains(&root) {
                    roots.push(root);
                }
            }
            for loan in frame.loans {
                let fact = LoanEndFact {
                    call: loan.call(),
                    argument: loan.argument(),
                    point,
                };
                if !self.loan_ends.contains(&fact) {
                    let drop_point = match point {
                        LoanEndPoint::CallReturn(expression) => {
                            super::DropPoint::CallReturn(expression)
                        }
                        LoanEndPoint::ControlTransfer(expression) => {
                            super::DropPoint::ControlTransfer(expression)
                        }
                    };
                    self.cleanup
                        .push((drop_point, super::IterationCleanupAction::EndCallLoan(fact)));
                    self.loan_ends.push(fact);
                }
                if let LoanTarget::Place(place) = loan.target()
                    && !roots.contains(&place.root())
                {
                    roots.push(place.root());
                }
            }
        }
        roots
    }
}
