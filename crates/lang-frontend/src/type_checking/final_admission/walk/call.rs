//! Call and constructor recovery for source-qualified facts.
use super::*;
use crate::parser::CallArgument;

impl<F: Facts> Checker<'_, F> {
    pub(super) fn rejected_bare_enum_case(
        &self,
        expression: ExpressionId,
        name_span: Span,
    ) -> Result<bool> {
        let catalog = codes::catalog().map_err(TypeCheckingError::from)?;
        for code in [
            codes::CONSTRUCTION_INFERENCE,
            codes::TYPE_ARGUMENT_BOUND,
            codes::COPYABLE_TYPE_ARGUMENT_BOUND,
            codes::TRANSFERABLE_TYPE_ARGUMENT_BOUND,
            codes::HASHABLE_TYPE_ARGUMENT_BOUND,
        ] {
            let code = catalog.resolve(code).map_err(TypeCheckingError::from)?;
            if self
                .facts
                .rejected_bare_enum_case(expression, name_span, code)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn check_call_type_arguments(
        &mut self,
        id: ExpressionId,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
    ) -> Result<bool> {
        let rejected = self.rejected_type_arguments(id, callee)?;
        let mut complete = true;
        for (index, type_ref) in type_arguments.iter().enumerate() {
            complete &=
                self.call_type_ref(*type_ref, rejected.is_some_and(|first| index >= first))?;
        }
        Ok(complete)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn call_expression(
        &mut self,
        id: ExpressionId,
        span: Span,
        callee: ExpressionId,
        arguments: &[CallArgument],
        type_arguments: &[TypeRefId],
        selection: Option<&super::super::adapters::Selection>,
        mut complete: bool,
    ) -> Result<bool> {
        let rejected_contextual_call = self.rejected_contextual_call(id, callee)?;
        let call_shape = self
            .facts
            .expression_type(id)
            .and_then(|ty| self.facts.shape(ty));
        if selection.is_none()
            && matches!(
                call_shape,
                Some(Shape::Error | Shape::Deferred(DeferredReason::Call))
            )
            && let Some(origin) = self.facts.rejected_storage_origin(id)
            && origin.source_id() == span.source_id()
        {
            // The producer proved the same nominal storage failure at
            // the explicit type argument. Still check every operand.
            let code = codes::catalog()
                .and_then(|catalog| catalog.resolve(codes::INVALID_CONTAINER_ELEMENT))
                .map_err(TypeCheckingError::from)?;
            complete &= !self.diagnostics.iter().any(|diagnostic| {
                diagnostic.severity() == Severity::Error
                    && diagnostic.code() == code
                    && diagnostic.primary_span() == origin
            });
        }
        let unresolved_contextual_call = selection.is_none()
            && matches!(call_shape, Some(Shape::Deferred(DeferredReason::Call)));
        let type_arguments_complete = self.check_call_type_arguments(id, callee, type_arguments)?;
        complete &= type_arguments_complete;
        let previous_context = self.contextual_type_reported;
        // A diagnostic on this call's explicit type argument explains
        // context-dependent operands. Visit their independent children.
        self.contextual_type_reported |= !type_arguments_complete
            && selection.is_none()
            && matches!(
                call_shape,
                Some(Shape::Error | Shape::Deferred(DeferredReason::Call))
            );
        let mut uncommitted_deferred_lambda = false;
        let operands = (|| {
            for argument in arguments {
                if (rejected_contextual_call || unresolved_contextual_call)
                    && self.uncommitted_lambda_argument(argument.value, rejected_contextual_call)?
                {
                    // 未提交的 lambda 不是消费 root。继续检查已提交的实参及 receiver；
                    // 未决调用没有独立 child 诊断时由自己的 Deferred 事实报告。
                    uncommitted_deferred_lambda |= unresolved_contextual_call;
                    complete &= !rejected_contextual_call;
                } else {
                    complete &= self.expression(argument.value, false)?;
                }
            }
            Ok::<_, Internal>(())
        })();
        self.contextual_type_reported = previous_context;
        operands?;
        // 参数已失败时，真实静态候选只是选择器，不消费其恢复值类型。
        // Single 的容器 callee 可带 Deferred(Call)，Unit 可没有值事实；
        // 两者都不能在同一 child 根因之后制造静态壳级联。运行时 callee 仍遍历。
        let failed_static_callee = selection.is_none()
            && (!complete || self.facts.is_pending_storage_value(id))
            && self.static_candidate(callee)?;
        if selection.as_ref().is_some_and(|s| s.static_callee) || failed_static_callee {
            complete &= self.selector(callee)?;
        } else {
            let deferred_call = matches!(call_shape, Some(Shape::Deferred(_)));
            // 已选 FunctionValue callee 必须具有自己的值事实。失败候选的缺值
            // 只在已有 child 根因、未决 lambda 没有提交，或具名静态候选
            // 对应真实 Deferred 时抑制级联。
            let failed_callee = selection.is_none()
                && (!complete
                    || uncommitted_deferred_lambda
                    || (deferred_call && self.static_candidate(callee)?));
            complete &= self.expression(callee, failed_callee)?;
        }
        Ok(complete)
    }
}
