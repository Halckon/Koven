use crate::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, Severity},
    parser::AssignmentOperator,
    type_checking::Copyability,
};

use super::{AccessKind, Checker, ExpressionUse, Flows, OwnershipCheckingError, State};

impl Checker<'_> {
    pub(super) fn check_element_expression(
        &mut self,
        expression: ExpressionId,
        state: State,
        usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        let Some(descriptor) = self.element_place_descriptor(expression)? else {
            return Ok(Flows::next(state));
        };
        let diagnostic_count = self.diagnostics.len();
        let flows = self.check_expression(descriptor.receiver(), state, ExpressionUse::Place)?;
        let mut flows = self.chain_expression(flows, descriptor.index(), ExpressionUse::Read)?;
        let Some(state) = flows.next.as_mut() else {
            return Ok(flows);
        };
        if self.diagnostics.len() != diagnostic_count || usage == ExpressionUse::Place {
            return Ok(flows);
        }

        let move_only =
            self.typed.copyability(descriptor.element_type()) == Some(Copyability::MoveOnly);
        let primary = self.parsed.ast().expressions().get(expression)?.span();
        if let Some(place) = self.place(expression)? {
            let access = if usage == ExpressionUse::Consume && move_only {
                AccessKind::Move
            } else {
                AccessKind::Read
            };
            if !self.access_place(&place, access, move_only, primary, state)? {
                return Ok(flows);
            }
        }
        if move_only {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.container_element_move_code,
                "cannot move a non-Copyable element out of a sequential container",
                primary,
            )?;
            if let Some(place) = self.place(expression)?
                && let Some(root) = self.names.symbols().get(place.root().index())
            {
                diagnostic.add_label(
                    self.sources,
                    root.span(),
                    "container owner remains responsible for every initialized element",
                )?;
            }
            self.diagnostics.push(diagnostic);
        }
        Ok(flows)
    }

    pub(super) fn check_element_assignment(
        &mut self,
        target: ExpressionId,
        operator: AssignmentOperator,
        value: ExpressionId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let descriptor = self
            .element_place_descriptor(target)?
            .expect("checked element descriptor");
        let diagnostic_count = self.diagnostics.len();
        let flows = self.check_expression(descriptor.receiver(), state, ExpressionUse::Place)?;
        let mut flows = self.chain_expression(flows, descriptor.index(), ExpressionUse::Read)?;
        if operator != AssignmentOperator::Assign
            && let Some(state) = flows.next.as_mut()
            && let Some(place) = self.place(target)?
        {
            self.access_place(
                &place,
                AccessKind::Read,
                false,
                self.parsed.ast().expressions().get(target)?.span(),
                state,
            )?;
        }
        flows = self.chain_expression(flows, value, ExpressionUse::Consume)?;
        let Some(state) = flows.next.as_mut() else {
            return Ok(flows);
        };
        if self.diagnostics.len() != diagnostic_count {
            return Ok(flows);
        }
        let Some(place) = self.place(target)? else {
            return Ok(flows);
        };
        let target_span = self.parsed.ast().expressions().get(target)?.span();
        if let Some(origin) = state.moved.get(&place.root()).copied() {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.use_after_move_code,
                "cannot replace an element after its container owner was moved",
                target_span,
            )?;
            diagnostic.add_label(self.sources, origin, "container owner was moved here")?;
            self.diagnostics.push(diagnostic);
            return Ok(flows);
        }
        self.access_place(&place, AccessKind::Mutation, false, target_span, state)?;
        Ok(flows)
    }
}
