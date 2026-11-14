//! Test-support resource snapshots and adapters shared by ordinary frames and loop edges.
use super::{Checker, Flows, State};
use crate::ownership_checking::{BorrowFrameChecker, LoanKind, research_borrow_loop as loops};
use crate::{
    ast::{ExpressionId, StatementId},
    parser::Expression,
};
impl Checker<'_> {
    pub(super) fn research_direct_string_call(
        &self,
        call: ExpressionId,
        initializer: bool,
    ) -> bool {
        let Some(typed) = self.typed.call(self.unit_expression(call)) else {
            return false;
        };
        let Ok(node) = self.parsed.ast().expressions().get(call) else {
            return false;
        };
        let Expression::Call { arguments, .. } = node.payload() else {
            return false;
        };
        let expected_return = if initializer {
            crate::type_checking::BuiltinType::String
        } else {
            crate::type_checking::BuiltinType::Unit
        };
        typed.receiver().is_none()
            && !typed.aborts()
            && typed.borrow_return().is_some() == initializer
            && self.typed.types().get(typed.return_type())
                == Some(&crate::type_checking::UnitTypeKind::Builtin(
                    expected_return,
                ))
            && matches!(
                typed.target(),
                crate::type_checking::UnitCallTarget::Declaration(_)
            )
            && arguments.iter().all(|arg| {
                self.typed
                    .expression_type(self.unit_expression(arg.value))
                    .and_then(|ty| self.typed.types().get(ty))
                    == Some(&crate::type_checking::UnitTypeKind::Builtin(
                        crate::type_checking::BuiltinType::String,
                    ))
            })
    }
    pub(super) fn research_boolean_parameter(&self, condition: ExpressionId) -> bool {
        !self.is_constant_use(condition)
            && self
                .parsed
                .ast()
                .expressions()
                .get(condition)
                .ok()
                .and_then(|node| self.reference_symbol(node.span()))
                .is_some_and(|symbol| {
                    self.bindings.contains_key(&symbol)
                        && self
                            .names
                            .source_units()
                            .iter()
                            .find(|unit| unit.source_unit() == symbol.source_unit())
                            .and_then(|unit| {
                                unit.resolution().symbols().get(symbol.symbol().index())
                            })
                            .is_some_and(|declaration| {
                                declaration.kind()
                                    == crate::name_resolution::SymbolKind::ValueParameter
                            })
                })
            && self.ordinary_condition_outcomes(condition) == (true, true)
            && self
                .typed
                .expression_type(self.unit_expression(condition))
                .and_then(|ty| self.typed.types().get(ty))
                == Some(&crate::type_checking::UnitTypeKind::Builtin(
                    crate::type_checking::BuiltinType::Boolean,
                ))
    }
    pub(super) fn research_carried_loop_start(
        &self,
        statement: Option<StatementId>,
        body: StatementId,
        state: &State,
    ) -> Option<loops::Ticket<State>> {
        if !loops::is_capturing() || !self.diagnostics.is_empty() {
            return None;
        }
        let statement = statement?;
        if state.borrow_bindings.is_empty() {
            return None;
        }
        let entry = self.resource_snapshot(state);
        if !self.simple_resources(&entry) {
            return None;
        }
        // Require every real carried binding's exact nonowning marker, place and shared loan.
        if entry.loans.len() != entry.borrow_bindings.len()
            || entry.borrow_bindings.iter().any(|(binding, place)| {
                !entry.non_owning.contains_key(binding)
                    || entry
                        .loans
                        .iter()
                        .filter(|loan| {
                            loan.owner == super::flow::ActiveLoanOwner::BorrowBinding(*binding)
                                && loan.kind == LoanKind::Shared
                                && loan.target
                                    == super::flow::ActiveLoanTarget::Place(place.clone())
                        })
                        .count()
                        != 1
            })
        {
            return None;
        }
        let shape = loops::shape(
            self.parsed,
            statement,
            body,
            |condition| self.research_boolean_parameter(condition),
            |expression| {
                if !self.research_direct_string_call(expression, false) {
                    return false;
                }
                let Some(call) = self.typed.call(self.unit_expression(expression)) else {
                    return false;
                };
                let Ok(node) = self.parsed.ast().expressions().get(expression) else {
                    return false;
                };
                let Expression::Call { arguments, .. } = node.payload() else {
                    return false;
                };
                call.arguments().len() == arguments.len()
                    && call.arguments().iter().all(|arg| {
                        arg.mode() == crate::type_checking::ParameterMode::Borrow
                            && !arg.crosses_thread()
                    })
                    && arguments.iter().all(|arg| {
                        self.parsed
                            .ast()
                            .expressions()
                            .get(arg.value)
                            .ok()
                            .and_then(|node| self.reference_symbol(node.span()))
                            .is_some_and(|symbol| entry.borrow_bindings.contains_key(&symbol))
                    })
            },
        )?;
        let carried = entry.borrow_bindings.len();
        loops::begin(
            entry,
            shape,
            BorrowFrameChecker::Unit,
            carried,
            (
                self.borrow_results.bindings.len(),
                self.borrow_results.ends.len(),
                self.diagnostics.len(),
            ),
            Some(self.source_unit),
        )
    }
    pub(super) fn research_carried_loop_finish(
        &self,
        ticket: Option<loops::Ticket<State>>,
        flows: &Flows,
    ) {
        if ticket.is_none() {
            return;
        }
        let edges = [
            flows.next.as_ref(),
            flows.continues.as_ref(),
            flows.breaks.as_ref(),
        ]
        .map(|state| state.map(|state| self.resource_snapshot(state)));
        loops::finish(
            ticket,
            edges,
            (
                self.borrow_results.bindings.len(),
                self.borrow_results.ends.len(),
                self.diagnostics.len(),
            ),
        );
    }
}
