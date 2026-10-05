use super::{Checker, ExpressionUse, Flows, OwnershipCheckingError, State};
use crate::{parser::CallArgument, type_checking::StringOperationDescriptor};
impl Checker<'_> {
    pub(super) fn check_string_operation(
        &mut self,
        descriptor: StringOperationDescriptor,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        self.check_shared_receiver_read(descriptor.expression(), descriptor.receiver(), state)
    }

    /// 同步读取 receiver；loan 在读取结束时结束，不传递给独立结果。
    pub(super) fn check_shared_receiver_read(
        &mut self,
        expression: crate::ast::ExpressionId,
        receiver: crate::ast::ExpressionId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let before = self.diagnostics.len();
        let mut flows = self.check_expression(receiver, state, ExpressionUse::Place)?;
        if self.diagnostics.len() == before {
            let span = self.parsed.ast().expressions().get(receiver)?.span();
            self.apply_argument_contract(
                expression,
                CallArgument {
                    span,
                    named_prefix: None,
                    mode_marker: None,
                    value: receiver,
                },
                crate::type_checking::ParameterMode::Borrow,
                true,
                &mut flows,
            )?;
        }
        self.end_call_loans(expression, &mut flows);
        Ok(flows)
    }
}
