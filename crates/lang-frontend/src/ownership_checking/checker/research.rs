//! Test-support resource snapshots and adapters shared by ordinary frames and loop edges.
use super::{Checker, Flows, State};
use crate::ownership_checking::{BorrowFrameChecker, LoanKind, research_borrow_loop as loops};
use crate::{
    ast::{ExpressionId, StatementId},
    parser::Expression,
};
impl Checker<'_> {
    pub(super) fn research_resource_snapshot(&self, state: &State) -> State {
        let mut snapshot = state.clone();
        // Empty closure-result caches contain no captured environment obligations.
        snapshot
            .closure_results
            .retain(|_, origins| !origins.is_empty());
        snapshot
    }
    pub(super) fn research_simple_resources(&self, snapshot: &State) -> bool {
        let mut empty_origins = Default::default();
        // This only constructs the comparison anchor; it does not alter actual state.
        crate::ownership_checking::callable_provenance::graph::OriginState::attach(
            &mut empty_origins,
            &self.callable_sources.arena,
        );
        snapshot.origins == empty_origins
            && snapshot.closure_results.is_empty()
            && snapshot.replacements.is_empty()
            && snapshot.closures.is_empty()
            && snapshot.closure_captures.is_empty()
            && snapshot.immutable_captures.is_empty()
            && snapshot.nullable_views.is_empty()
            && snapshot.pending_closures.is_empty()
    }
    pub(super) fn research_direct_string_call(
        &self,
        call: ExpressionId,
        initializer: bool,
    ) -> bool {
        let Some(typed) = self.typed.call(call) else {
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
                == Some(&crate::type_checking::TypeKind::Builtin(expected_return))
            && matches!(
                typed.target(),
                crate::type_checking::CallableTarget::Source(_)
            )
            && arguments.iter().all(|arg| {
                self.typed
                    .expression_type(arg.value)
                    .and_then(|ty| self.typed.types().get(ty))
                    == Some(&crate::type_checking::TypeKind::Builtin(
                        crate::type_checking::BuiltinType::String,
                    ))
            })
    }
    pub(super) fn research_loop_start(
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
        let entry = self.research_resource_snapshot(state);
        if !self.research_simple_resources(&entry) {
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
                            loan.owner == super::loan::ActiveLoanOwner::BorrowBinding(*binding)
                                && loan.kind == LoanKind::Shared
                                && loan.target
                                    == super::loan::ActiveLoanTarget::Place(place.clone())
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
            |condition| {
                !self.is_constant_use(condition)
                    && self
                        .parsed
                        .ast()
                        .expressions()
                        .get(condition)
                        .ok()
                        .and_then(|node| self.reference_symbol(node.span()))
                        .is_some_and(|symbol| self.typed.parameter_mode(symbol).is_some())
                    && self
                        .typed
                        .expression_type(condition)
                        .and_then(|ty| self.typed.types().get(ty))
                        == Some(&crate::type_checking::TypeKind::Builtin(
                            crate::type_checking::BuiltinType::Boolean,
                        ))
            },
            |expression| {
                if !self.research_direct_string_call(expression, false) {
                    return false;
                }
                let Some(call) = self.typed.call(expression) else {
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
            BorrowFrameChecker::Single,
            carried,
            (
                self.borrow_results.bindings.len(),
                self.borrow_results.ends.len(),
                self.diagnostics.len(),
            ),
        )
    }
    pub(super) fn research_loop_finish(&self, ticket: Option<loops::Ticket<State>>, flows: &Flows) {
        if ticket.is_none() {
            return;
        }
        let edges = [
            flows.next.as_ref(),
            flows.continues.as_ref(),
            flows.breaks.as_ref(),
        ]
        .map(|state| state.map(|state| self.research_resource_snapshot(state)));
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
