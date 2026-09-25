//! Lambda body 的参数与局部 owner 属于独立 callable，不继承创建点的迭代 frame。
use super::{
    ClosureOrigin, DropPlanner, DropPoint, ExpressionUse, IterationCleanupAction, OwnedValue,
    OwnershipCheckingError, ValueState,
};
use crate::{
    ast::{ExpressionId, StatementId},
    ownership_checking::ClosureCaptureSource,
    parser::Expression,
    source::Span,
};

impl DropPlanner<'_, '_> {
    pub(super) fn plan_lambda_expression(
        &mut self,
        lambda: ExpressionId,
        environment: &ClosureOrigin,
    ) -> Result<(), OwnershipCheckingError> {
        let Expression::Lambda {
            opener_span,
            parameters,
            arrow_span,
            body,
            ..
        } = self
            .checker
            .parsed
            .ast()
            .expressions()
            .get(lambda)?
            .payload()
            .clone()
        else {
            return Ok(());
        };
        let mut parameters = parameters;
        if arrow_span.is_none()
            && let Some(symbol) = self
                .checker
                .symbols_by_span
                .get(&super::super::span_key(opener_span))
            && self.checker.typed.parameter_mode(*symbol).is_some()
        {
            parameters.push(opener_span);
        }
        let enclosing = self
            .current_environment
            .replace((environment.owner, lambda));
        let result = self.plan_lambda_body(lambda, &parameters, body, environment);
        self.current_environment = enclosing;
        result
    }

    pub(super) fn plan_lambda_body(
        &mut self,
        lambda: ExpressionId,
        parameters: &[Span],
        body: StatementId,
        environment: &ClosureOrigin,
    ) -> Result<(), OwnershipCheckingError> {
        if self.liveness.skipped_lambdas.contains(&lambda.index()) {
            return Ok(());
        }
        let loops = std::mem::take(&mut self.loop_boundaries);
        let depth = std::mem::replace(&mut self.scope_depth, 0);
        let mut state = ValueState::default();
        self.cleanup.push((
            DropPoint::LambdaEntry(lambda),
            IterationCleanupAction::BindClosureEnvironment {
                owner: environment.owner,
                closure: lambda,
            },
        ));
        for captured in &environment.captured {
            if let Some(ClosureCaptureSource::Symbol(symbol)) = captured.captured_from {
                let mut origin = captured.clone();
                origin.captured_from = None;
                state.closures.entry(symbol).or_default().push(origin);
            }
        }
        for &origin in parameters {
            if let Some(&symbol) = self
                .checker
                .symbols_by_span
                .get(&super::super::span_key(origin))
                && self.checker.is_move_only_variable(symbol)
            {
                self.binding_depths.insert(symbol, 0);
                state.insert(OwnedValue {
                    versions: vec![self.parameter_owner(symbol, origin)],
                    condition: state.path,
                    symbol,
                    origin,
                    scope_depth: 0,
                });
            }
        }
        // Capture owner 仍由 closure environment 持有，不在每次调用中重复析构。
        if let Some(live) = self.liveness.lambda_live_in.get(&lambda.index()) {
            let unused = state
                .values
                .iter()
                .rev()
                .filter(|value| !live.contains(&value.symbol))
                .map(|value| value.symbol)
                .collect::<Vec<_>>();
            for symbol in unused {
                self.drop_named(DropPoint::LambdaEntry(lambda), symbol, &mut state);
            }
        }
        if self.control_body(body, ExpressionUse::Consume, &mut state)? {
            self.drop_all(DropPoint::AfterStatement(body), &mut state);
        }
        self.scope_depth = depth;
        self.loop_boundaries = loops;
        Ok(())
    }
}
