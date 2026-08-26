use crate::{
    diagnostic::{Diagnostic, Severity},
    parser::Expression,
    type_checking::{Copyability, RcOperationDescriptor, RcOperationKind},
};

use crate::ownership_checking::{RcOwnershipEffect, RcOwnershipEffectKind};

use super::{AccessKind, Checker, ExpressionUse, Flows, OwnershipCheckingError, State};

impl Checker<'_> {
    pub(super) fn check_rc_operation(
        &mut self,
        descriptor: RcOperationDescriptor,
        state: State,
        usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        let diagnostic_count = self.diagnostics.len();
        let receiver = descriptor.receiver();
        let mut flows = self.check_expression(receiver, state, ExpressionUse::Place)?;
        let Some(state) = flows.next.as_mut() else {
            return Ok(flows);
        };
        let Some(place) = self.place(receiver)? else {
            self.defer(
                descriptor.expression(),
                crate::ownership_checking::OwnershipDeferredReason::MemberReceiver,
            );
            return Ok(flows);
        };
        let primary = self
            .parsed
            .ast()
            .expressions()
            .get(descriptor.expression())?
            .span();
        self.access_place(&place, AccessKind::Read, false, primary, state)?;
        if self.diagnostics.len() != diagnostic_count {
            return Ok(flows);
        }

        let effect = match descriptor.kind() {
            RcOperationKind::Share => RcOwnershipEffectKind::Retain,
            RcOperationKind::Value => {
                if usage == ExpressionUse::Consume
                    && self.typed.copyability(descriptor.payload_type())
                        == Some(Copyability::MoveOnly)
                {
                    let name_span = match self
                        .parsed
                        .ast()
                        .expressions()
                        .get(descriptor.expression())?
                        .payload()
                    {
                        Expression::Member { name_span, .. } => *name_span,
                        _ => primary,
                    };
                    let mut diagnostic = Diagnostic::new(
                        self.sources,
                        Severity::Error,
                        self.partial_move_code,
                        "cannot move a non-Copyable payload out of Rc",
                        name_span,
                    )?;
                    diagnostic.add_label(
                        self.sources,
                        self.parsed.ast().expressions().get(receiver)?.span(),
                        "the Rc owner remains responsible for its payload",
                    )?;
                    self.diagnostics.push(diagnostic);
                    return Ok(flows);
                }
                RcOwnershipEffectKind::BorrowPayload
            }
        };
        self.rc_effects.push(RcOwnershipEffect::new(
            descriptor.expression(),
            receiver,
            descriptor.payload_type(),
            effect,
        ));
        Ok(flows)
    }
}
