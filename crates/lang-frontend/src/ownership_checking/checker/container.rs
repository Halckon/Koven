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

        self.access_element_value(expression, usage, state)?;
        Ok(flows)
    }

    /// 检查已完成求值的 element 交付，不重复 receiver/index 的副作用。
    pub(super) fn access_element_value(
        &mut self,
        expression: ExpressionId,
        usage: ExpressionUse,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(descriptor) = self.element_place_descriptor(expression)? else {
            return Ok(());
        };

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
                return Ok(());
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
        Ok(())
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
        // Element storage transfers ownership to the container, beyond a local capture's lifetime.
        flows = self.chain_escaping_expression(flows, value, ExpressionUse::Consume)?;
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

pub(super) fn populate_container_call_contracts(
    typed: &crate::type_checking::TypedFile,
    receivers_by_expression: &mut std::collections::BTreeMap<
        usize,
        crate::type_checking::CallReceiverDescriptor,
    >,
    calls_by_expression: &mut std::collections::BTreeMap<
        usize,
        Vec<crate::type_checking::ParameterMode>,
    >,
) {
    for construction in typed.container_constructions() {
        calls_by_expression
            .entry(construction.expression().index())
            .or_insert_with(|| construction.parameter_modes().to_vec());
    }
    for append in typed.container_appends() {
        receivers_by_expression.insert(
            append.expression().index(),
            crate::type_checking::CallReceiverDescriptor {
                origin: crate::type_checking::CallReceiverOrigin::Expression(append.receiver()),
                mode: crate::type_checking::ParameterMode::Inout,
                category: typed
                    .expression_category(append.receiver())
                    .unwrap_or(crate::type_checking::ExpressionCategory::Place),
                ty: append.container_type(),
            },
        );
        calls_by_expression
            .entry(append.expression().index())
            .or_insert_with(|| vec![crate::type_checking::ParameterMode::Value]);
    }
}

