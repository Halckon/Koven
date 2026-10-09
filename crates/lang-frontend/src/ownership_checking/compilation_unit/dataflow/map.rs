//! Map 下标赋值按源码顺序求值 receiver/key/value，并取得独占变异权限。
use super::{AccessKind, Checker, ExpressionUse, Flows, OwnershipCheckingError, State};
use crate::{
    diagnostic::{Diagnostic, Severity},
    type_checking::UnitMapPutDescriptor,
};

impl Checker<'_> {
    pub(super) fn check_map_assignment(
        &mut self,
        put: &UnitMapPutDescriptor,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let receiver = put.receiver().expression();
        let diagnostic_count = self.diagnostics.len();
        let flows = self.check_expression(
            receiver,
            state,
            ExpressionUse::Place {
                parameter_span: None,
            },
        )?;
        let flows = self.chain_expression(
            flows,
            put.key().expression(),
            ExpressionUse::Consume {
                parameter_span: None,
            },
        )?;
        let mut flows = self.chain_expression(
            flows,
            put.value().expression(),
            ExpressionUse::Consume {
                parameter_span: None,
            },
        )?;
        if self.diagnostics.len() != diagnostic_count {
            return Ok(flows);
        }
        let Some(state) = flows.next.as_mut() else {
            return Ok(flows);
        };
        let span = self
            .parsed
            .ast()
            .expressions()
            .get(put.expression().expression())?
            .span();
        if !self.is_mutable_place(receiver)? {
            self.diagnostics.push(Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.immutable_inout,
                "map update requires a mutable receiver place",
                span,
            )?);
            return Ok(flows);
        }
        if let Some(place) = self.place(receiver)? {
            self.access_place(&place, AccessKind::Mutation, span, None, state)?;
        }
        Ok(flows)
    }
}
