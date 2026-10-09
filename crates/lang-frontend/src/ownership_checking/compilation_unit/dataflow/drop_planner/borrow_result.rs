//! Borrow binding 的来源保护及显式子到父终止顺序。
use super::*;
use crate::ownership_checking::BorrowBindingEndFact;
impl DropPlanner<'_, '_> {
    pub(super) fn continue_range_temporary(&self, id: ExpressionId, state: &mut ValueState) {
        if let Some(fact) = self.checker.range_use(id) {
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
    pub(super) fn statement(
        &mut self,
        id: StatementId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let continues = self.statement_inner(id, state)?;
        if continues {
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
        Ok(continues)
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
        let mut roots = Vec::new();
        for index in (0..state.borrow_bindings.len()).rev() {
            let binding = state.borrow_bindings[index];
            if !selected.contains(&binding) {
                continue;
            }
            state.borrow_bindings.remove(index);
            let fact = BorrowBindingEndFact {
                binding,
                point: point.into_unit(self.checker.source_unit),
            };
            if !self.borrow_ends.contains(&fact) {
                self.borrow_ends.push(fact);
            }
            if let Some(fact) = self
                .checker
                .borrow_results
                .bindings
                .iter()
                .find(|fact| fact.binding == binding)
                && let crate::ownership_checking::UnitLoanTarget::Place(place) = &fact.origin
                && !roots.contains(&place.root())
            {
                roots.push(place.root());
            }
        }
        for root in roots {
            let live = match point {
                PlannerDropPoint::AfterStatement(id) => {
                    self.liveness.statement_after[id.index()].contains(&root)
                }
                _ => true,
            };
            if !live {
                self.drop_named_asap(point, root, state);
            }
        }
    }
}
