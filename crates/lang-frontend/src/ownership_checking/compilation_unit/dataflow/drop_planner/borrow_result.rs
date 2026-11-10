//! Borrow binding 的来源保护及显式子到父终止顺序。
use super::*;
use crate::ownership_checking::BorrowBindingEndFact;
impl DropPlanner<'_, '_> {
    pub(super) fn continue_borrow_call_source(
        &self,
        id: ExpressionId,
        state: &mut ValueState,
    ) -> Option<(UnitSymbolId, PlannerDropPoint)> {
        let mut copied_source = None;
        for fact in self
            .checker
            .borrow_results
            .call_uses
            .iter()
            .filter(|fact| fact.expression() == self.checker.unit_expression(id))
        {
            let crate::ownership_checking::UnitLoanTarget::Place(place) = fact.origin() else {
                continue;
            };
            if fact.end_point()
                == crate::ownership_checking::UnitDropPoint::AfterExpression(fact.argument())
            {
                copied_source = Some((
                    place.root(),
                    PlannerDropPoint::AfterExpression(fact.argument().expression()),
                ));
            } else if state.position(place.root()).is_some()
                && !state
                    .pending_borrows
                    .contains(&(fact.call().expression(), place.root()))
            {
                state
                    .pending_borrows
                    .push((fact.call().expression(), place.root()));
            }
        }
        copied_source
    }

    pub(super) fn continue_range_source(&self, id: ExpressionId, state: &mut ValueState) {
        if let Some(fact) = self.checker.range_use(id) {
            let consumer = match fact.site() {
                crate::ownership_checking::RangeUseSite::Call(outer) => outer.expression(),
                crate::ownership_checking::RangeUseSite::Iteration => id,
            };
            // Call consumers keep the root until their return. Iteration consumers
            // hand this pending protection to the provider after source evaluation.
            if let crate::ownership_checking::UnitLoanTarget::Place(place) = fact.origin()
                && state.position(place.root()).is_some()
                && !state.pending_borrows.contains(&(consumer, place.root()))
            {
                state.pending_borrows.push((consumer, place.root()));
            }
            match fact.site() {
                crate::ownership_checking::RangeUseSite::Call(outer) => {
                    for temporary in &mut state.pending_temporaries {
                        if temporary.control == id {
                            temporary.control = outer.expression();
                        }
                    }
                }
                crate::ownership_checking::RangeUseSite::Iteration => {
                    state
                        .pending_temporaries
                        .retain(|temporary| temporary.control != id);
                }
            }
        }
    }
    pub(super) fn finish_call_borrows(
        &self,
        id: ExpressionId,
        borrowed_roots: &mut Vec<UnitSymbolId>,
        state: &mut ValueState,
    ) {
        state.pending_capture_borrows.retain(|borrow| {
            if borrow.call != id {
                return true;
            }
            if !borrowed_roots.contains(&borrow.symbol) {
                borrowed_roots.push(borrow.symbol);
            }
            false
        });
        state.pending_borrows.retain(|(call, root)| {
            if *call != id {
                return true;
            }
            if !borrowed_roots.contains(root) {
                borrowed_roots.push(*root);
            }
            false
        });
    }
    pub(super) fn statement_with_borrow_ends(
        &mut self,
        id: StatementId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        self.enter_binding_region(id, state);
        let result = self.statement_inner(id, state);
        if matches!(result, Ok(true)) {
            let point = PlannerDropPoint::AfterStatement(id);
            let selected = self
                .checker
                .borrow_results
                .ends
                .iter()
                .filter(|fact| fact.point == point.into_unit(self.checker.source_unit))
                .map(|fact| fact.binding)
                .collect();
            self.end_selected_borrow_bindings(point, state, &selected);
        }
        self.finish_binding_region_statement(id, matches!(result, Ok(true)), state);
        result
    }
    pub(super) fn borrow_source_protected(&self, symbol: UnitSymbolId, state: &ValueState) -> bool {
        state.borrow_bindings.iter().chain(&state.pending_borrow_results).any(|binding| self.checker.borrow_results.bindings.iter().any(|fact| fact.binding == *binding && matches!(&fact.origin, crate::ownership_checking::UnitLoanTarget::Place(place) if place.root() == symbol)))
    }
    pub(super) fn end_borrow_bindings(
        &mut self,
        point: PlannerDropPoint,
        state: &mut ValueState,
        selected: impl Fn(usize) -> bool,
    ) {
        let bindings = state
            .borrow_bindings
            .iter()
            .copied()
            .filter(|binding| {
                self.binding_depths
                    .get(binding)
                    .is_some_and(|depth| selected(*depth))
            })
            .collect();
        self.end_selected_borrow_bindings(point, state, &bindings);
    }

    fn end_selected_borrow_bindings(
        &mut self,
        point: PlannerDropPoint,
        state: &mut ValueState,
        selected: &std::collections::BTreeSet<UnitSymbolId>,
    ) {
        let mut roots: Vec<(UnitSymbolId, Option<super::action_source::BindingDropCause>)> =
            Vec::new();
        for index in (0..state.borrow_bindings.len()).rev() {
            let binding = state.borrow_bindings[index];
            if !selected.contains(&binding) {
                continue;
            }
            state.borrow_bindings.remove(index);
            let transport = self.finish_borrow_binding_transport(binding, point, state);
            let fact = BorrowBindingEndFact {
                binding,
                point: point.into_unit(self.checker.source_unit),
            };
            let binding_end = self.iteration_actions.end_borrow_binding(fact);
            self.retire_borrow_binding_child(binding, point, binding_end.clone(), state);
            let cause = transport.and_then(|(transaction, pending)| {
                let retirement = pending.publish_retirement(
                    &mut self.iteration_actions,
                    transaction,
                    binding_end,
                )?;
                self.iteration_actions
                    .binding_drop_cause(binding, retirement)
            });
            if let Some(fact) = self
                .checker
                .borrow_results
                .bindings
                .iter()
                .find(|fact| fact.binding == binding)
                && let crate::ownership_checking::UnitLoanTarget::Place(place) = &fact.origin
            {
                if let Some((_, existing)) =
                    roots.iter_mut().find(|(root, _)| *root == place.root())
                {
                    // Preserve the old single cleanup attempt, but no binding wins ambiguity.
                    *existing = None;
                } else {
                    roots.push((place.root(), cause));
                }
            }
        }
        for (root, cause) in roots {
            let live = match point {
                PlannerDropPoint::AfterStatement(id) => {
                    self.liveness.statement_after[id.index()].contains(&root)
                }
                _ => true,
            };
            if !live {
                self.drop_named_asap_with_binding_drop(point, root, state, cause);
            }
        }
    }
}
