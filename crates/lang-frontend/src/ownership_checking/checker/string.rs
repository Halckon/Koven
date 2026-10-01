use super::{Checker, ExpressionUse, Flows, OwnershipCheckingError, State};
use crate::{parser::CallArgument, type_checking::StringOperationDescriptor};
impl Checker<'_> {
    pub(super) fn check_string_operation(
        &mut self,
        descriptor: StringOperationDescriptor,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let before = self.diagnostics.len();
        let receiver = descriptor.receiver();
        let mut flows = self.check_expression(receiver, state, ExpressionUse::Place)?;
        if self.diagnostics.len() == before {
            let span = self.parsed.ast().expressions().get(receiver)?.span();
            self.apply_argument_contract(
                descriptor.expression(),
                CallArgument {
                    span,
                    named_prefix: None,
                    mode_marker: None,
                    value: receiver,
                },
                descriptor.receiver_mode(),
                true,
                &mut flows,
            )?;
        }
        self.end_call_loans(descriptor.expression(), &mut flows);
        Ok(flows)
    }
}
