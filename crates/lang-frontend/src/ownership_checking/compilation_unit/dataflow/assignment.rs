//! Ordinary assignment commits and rollback, including a single closure environment transfer.

use crate::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, Severity},
    parser::AssignmentOperator,
};

use super::{AccessKind, Checker, ExpressionUse, Flows, OwnershipCheckingError, State};

impl Checker<'_> {
    pub(super) fn check_assignment(
        &mut self,
        assignment: ExpressionId,
        target: ExpressionId,
        operator: AssignmentOperator,
        value: ExpressionId,
        mut state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let entry_state = state.clone();
        let closure = self.closure_origins(value, &state)?;
        let source = self.expression_root_symbol(value)?;
        let root = self
            .place(target)?
            .filter(|place| place.is_root())
            .map(|place| place.root());
        if let Some(root) = root {
            state.replacements.push(root);
        }
        let diagnostic_count = self.diagnostics.len();
        let fact_lengths = (
            self.loans.len(),
            self.value_deliveries.len(),
            self.receiver_facts.len(),
            self.rc_effects.len(),
            self.construction_plans.len(),
            self.non_null_assertions.len(),
            self.ownership_primitives.len(),
            self.field_replacements.len(),
            self.formed_owned_locals.len(),
        );
        let nullable_when_checkpoint = self.nullable_whens.clone();
        let mut flows = self.check_expression(
            value,
            state,
            ExpressionUse::Consume {
                parameter_span: None,
            },
        )?;
        if root.is_some() {
            for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
                .into_iter()
                .flatten()
            {
                state.replacements.pop();
            }
        }
        if self.diagnostics.len() != diagnostic_count {
            self.rollback_facts(fact_lengths);
            self.nullable_whens = nullable_when_checkpoint;
            return Ok(Self::restore_flow_states(flows, &entry_state));
        }
        let target_span = self.parsed.ast().expressions().get(target)?.span();
        let place = self.place(target)?;
        if let Some(place) = &place
            && !self.is_mutable_place(target)?
        {
            let declaration = place.fields().last().copied().unwrap_or(place.root());
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.immutable_inout,
                "assignment target is not a mutable place",
                target_span,
            )?;
            diagnostic.add_label(
                self.sources,
                self.symbol_span(declaration)?,
                "immutable binding declared here",
            )?;
            self.diagnostics.push(diagnostic);
            self.rollback_facts(fact_lengths);
            self.nullable_whens = nullable_when_checkpoint;
            return Ok(Self::restore_flow_states(flows, &entry_state));
        }
        let Some(state) = flows.next.as_mut() else {
            return Ok(flows);
        };
        let closure = if closure.is_empty() {
            state
                .closure_results
                .get(&self.unit_expression(value))
                .cloned()
                .unwrap_or_default()
        } else {
            closure
        };
        if let Some(place) = place {
            if operator != AssignmentOperator::Assign {
                self.access_place(&place, AccessKind::Read, target_span, None, state)?;
            }
            if self.access_place(&place, AccessKind::Mutation, target_span, None, state)?
                && operator == AssignmentOperator::Assign
                && place.fields().is_empty()
            {
                let root = place.root();
                if place.is_root() {
                    // Explicit replacement commits even inside an enclosing RHS for this root.
                    self.release_closure_except(root, &closure, state);
                    if let Some(source) = source {
                        state.closures.remove(&source);
                    }
                    if !closure.is_empty() {
                        state.closures.insert(root, closure);
                    }
                }
                self.bind_callable_symbol(root, state);
                state.moved.remove(&root);
                if place.is_root()
                    && !self.expression_live_after[assignment.index()].contains(&root)
                {
                    self.release_closure(root, state);
                }
            }
            if self.diagnostics.len() != diagnostic_count {
                self.rollback_facts(fact_lengths);
                self.nullable_whens = nullable_when_checkpoint;
                return Ok(Self::restore_flow_states(flows, &entry_state));
            }
        } else {
            flows = self.chain_expression(flows, target, ExpressionUse::Read)?;
            if self.diagnostics.len() != diagnostic_count {
                self.rollback_facts(fact_lengths);
                self.nullable_whens = nullable_when_checkpoint;
                return Ok(Self::restore_flow_states(flows, &entry_state));
            }
        }
        Ok(flows)
    }

    pub(super) fn rollback_facts(
        &mut self,
        lengths: (
            usize,
            usize,
            usize,
            usize,
            usize,
            usize,
            usize,
            usize,
            usize,
        ),
    ) {
        self.loans.truncate(lengths.0);
        self.value_deliveries.truncate(lengths.1);
        self.receiver_facts.truncate(lengths.2);
        self.rc_effects.truncate(lengths.3);
        self.construction_plans.truncate(lengths.4);
        self.non_null_assertions.truncate(lengths.5);
        self.ownership_primitives.truncate(lengths.6);
        self.field_replacements.truncate(lengths.7);
        self.formed_owned_locals.truncate(lengths.8);
    }

    fn restore_flow_states(mut flows: Flows, entry: &State) -> Flows {
        for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues] {
            if state.is_some() {
                *state = Some(entry.clone());
            }
        }
        flows
    }
}
