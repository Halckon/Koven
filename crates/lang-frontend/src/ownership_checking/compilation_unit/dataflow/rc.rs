//! Intrinsic Rc retain 与 payload-borrow 的 source-qualified effect。

use crate::{
    diagnostic::{Diagnostic, Severity},
    ownership_checking::RcOwnershipEffectKind,
    parser::Expression,
    type_checking::{Copyability, RcOperationKind, UnitRcOperationDescriptor},
};

use super::{
    AccessKind, Checker, ExpressionUse, Flows, OwnershipCheckingError, State,
    UnitRcOwnershipEffect, add_parameter_label,
};

impl Checker<'_> {
    pub(super) fn check_rc_operation(
        &mut self,
        descriptor: UnitRcOperationDescriptor,
        state: State,
        usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        let diagnostic_count = self.diagnostics.len();
        let receiver = descriptor.receiver();
        if receiver.source_unit() != self.source_unit {
            return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                source_unit: self.source_unit.index(),
                expression: descriptor.expression().expression().index(),
            });
        }
        let mut flows = self.check_expression(
            receiver.expression(),
            state,
            ExpressionUse::Place {
                parameter_span: None,
            },
        )?;
        let Some(state) = flows.next.as_mut() else {
            return Ok(flows);
        };
        if self.diagnostics.len() != diagnostic_count {
            return Ok(flows);
        }
        let primary = self
            .parsed
            .ast()
            .expressions()
            .get(descriptor.expression().expression())?
            .span();
        if let Some(place) = self.place(receiver.expression())? {
            if !self.access_place(&place, AccessKind::Read, primary, None, state)? {
                return Ok(flows);
            }
        } else if self
            .temporary_expression_origin(receiver.expression())?
            .is_none()
        {
            return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                source_unit: self.source_unit.index(),
                expression: receiver.expression().index(),
            });
        }

        let kind = match descriptor.kind() {
            RcOperationKind::Share => RcOwnershipEffectKind::Retain,
            RcOperationKind::Value => {
                let parameter_span = match usage {
                    ExpressionUse::Consume { parameter_span } => parameter_span,
                    ExpressionUse::Read | ExpressionUse::Place { .. } => None,
                };
                if matches!(usage, ExpressionUse::Consume { .. })
                    && self.typed.copyability(descriptor.payload_type()) == Copyability::MoveOnly
                {
                    let name_span = match self
                        .parsed
                        .ast()
                        .expressions()
                        .get(descriptor.expression().expression())?
                        .payload()
                    {
                        Expression::Member { name_span, .. } => *name_span,
                        _ => primary,
                    };
                    let mut diagnostic = Diagnostic::new(
                        self.sources,
                        Severity::Error,
                        self.codes.partial_move,
                        "cannot move a non-Copyable payload out of Rc",
                        name_span,
                    )?;
                    diagnostic.add_label(
                        self.sources,
                        self.parsed
                            .ast()
                            .expressions()
                            .get(receiver.expression())?
                            .span(),
                        "the Rc owner remains responsible for its payload",
                    )?;
                    add_parameter_label(self.sources, &mut diagnostic, parameter_span)?;
                    self.diagnostics.push(diagnostic);
                    return Ok(flows);
                }
                RcOwnershipEffectKind::BorrowPayload
            }
        };
        self.rc_effects.push(UnitRcOwnershipEffect::new(
            descriptor.expression(),
            receiver,
            descriptor.payload_type(),
            kind,
        ));
        Ok(flows)
    }
}
