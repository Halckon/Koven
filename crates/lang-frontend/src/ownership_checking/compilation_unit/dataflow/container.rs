//! Source-qualified intrinsic sequential-container element expression dataflow。

use crate::{
    diagnostic::{Diagnostic, Severity},
    type_checking::Copyability,
};

use super::{
    AccessKind, Checker, ExpressionUse, Flows, OwnershipCheckingError, State, add_parameter_label,
};

impl Checker<'_> {
    pub(super) fn check_element_expression(
        &mut self,
        expression: crate::ast::ExpressionId,
        state: State,
        usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        let Some(descriptor) = self.element_place_descriptor(expression)? else {
            return Ok(Flows::next(state));
        };
        if descriptor.receiver().source_unit() != self.source_unit
            || descriptor.index().source_unit() != self.source_unit
        {
            return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                source_unit: self.source_unit.index(),
                expression: expression.index(),
            });
        }
        let diagnostic_count = self.diagnostics.len();
        let flows = self.check_expression(
            descriptor.receiver().expression(),
            state,
            ExpressionUse::Place {
                parameter_span: None,
            },
        )?;
        let mut flows =
            self.chain_expression(flows, descriptor.index().expression(), ExpressionUse::Read)?;
        let Some(state) = flows.next.as_mut() else {
            return Ok(flows);
        };
        if self.diagnostics.len() != diagnostic_count
            || matches!(usage, ExpressionUse::Place { .. })
        {
            return Ok(flows);
        }

        let move_only = match self.typed.copyability(descriptor.element_type()) {
            Copyability::Copyable => false,
            Copyability::MoveOnly => true,
            Copyability::Unknown | Copyability::Error => {
                return Err(OwnershipCheckingError::InvalidUnitArgumentType {
                    source_unit: self.source_unit.index(),
                    expression: expression.index(),
                });
            }
        };
        let parameter_span = match usage {
            ExpressionUse::Consume { parameter_span } => parameter_span,
            ExpressionUse::Read | ExpressionUse::Place { .. } => None,
        };
        let primary = self.parsed.ast().expressions().get(expression)?.span();
        let place = self.place(expression)?;
        if let Some(place) = &place {
            let access = if matches!(usage, ExpressionUse::Consume { .. }) && move_only {
                AccessKind::Move
            } else {
                AccessKind::Read
            };
            if !self.access_place(place, access, primary, parameter_span, state)? {
                return Ok(flows);
            }
        }
        if !move_only {
            return Ok(flows);
        }

        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.codes.container_element_move,
            "cannot move a non-Copyable element out of a sequential container",
            primary,
        )?;
        if let Some(place) = place {
            diagnostic.add_label(
                self.sources,
                self.symbol_span(place.root())?,
                "container owner remains responsible for every initialized element",
            )?;
        } else if let Some(owner) = self.temporary_element_owner(expression)? {
            diagnostic.add_label(
                self.sources,
                self.parsed
                    .ast()
                    .expressions()
                    .get(owner.expression())?
                    .span(),
                "temporary container owns every initialized element",
            )?;
        }
        add_parameter_label(self.sources, &mut diagnostic, parameter_span)?;
        self.diagnostics.push(diagnostic);
        Ok(flows)
    }
}
