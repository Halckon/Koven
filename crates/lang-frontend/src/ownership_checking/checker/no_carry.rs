//! Real Single-checker adapter for the finite no-carry body protocol.
use super::loan::{ActiveLoanOwner, ActiveLoanTarget};
use super::{Checker, Flows, OwnershipCheckingError, State};
use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
    ownership_checking::{DropPoint, LoanKind, LoanTarget, OwnershipPlace, loop_no_carry as core},
    parser::Expression,
    source::Span,
    type_checking::{BuiltinType, CallableTarget, ParameterMode, TypeKind},
};
pub(super) struct Session {
    frame: core::Frame<State, SymbolId>,
    plan: core::Plan<SymbolId>,
    entry: State,
    bindings: usize,
    ends: usize,
    loans: usize,
    diagnostics: usize,
    cursor: usize,
}
impl Checker<'_> {
    fn no_carry_call(&self, id: ExpressionId, initializer: bool) -> bool {
        let Some(t) = self.typed.call(id) else {
            return false;
        };
        let Ok(node) = self.parsed.ast().expressions().get(id) else {
            return false;
        };
        let Expression::Call { arguments, .. } = node.payload() else {
            return false;
        };
        t.receiver().is_none()
            && !t.aborts()
            && t.borrow_return().is_some() == initializer
            && matches!(t.target(), CallableTarget::Source(_))
            && arguments.len() == 1
            && t.arguments().len() == 1
            && t.arguments().iter().all(|a| {
                a.mode() == ParameterMode::Borrow
                    && !a.crosses_thread()
                    && self.typed.types().get(a.parameter_type())
                        == Some(&TypeKind::Builtin(BuiltinType::String))
            })
            && self.typed.types().get(t.return_type())
                == Some(&TypeKind::Builtin(if initializer {
                    BuiltinType::String
                } else {
                    BuiltinType::Unit
                }))
            && arguments.iter().all(|a| {
                self.typed
                    .expression_type(a.value)
                    .and_then(|t| self.typed.types().get(t))
                    == Some(&TypeKind::Builtin(BuiltinType::String))
            })
    }
    pub(super) fn no_carry_begin(
        &mut self,
        statement: Option<StatementId>,
        body: StatementId,
        state: &State,
    ) {
        if self.no_carry.is_some()
            || !self.diagnostics.is_empty()
            || !state.loans.is_empty()
            || !state.borrow_bindings.is_empty()
        {
            return;
        }
        let entry = self.resource_snapshot(state);
        if !self.simple_resources(&entry) {
            return;
        }
        let Some(statement) = statement else { return };
        let Some(plan) = core::shape::plan(
            self.parsed,
            statement,
            body,
            |name| match name {
                core::shape::Name::Declaration(m) => self.marker_symbol(m),
                core::shape::Name::Reference(s) => self.reference_symbol(s),
            },
            |id, boolean| {
                self.parsed
                    .ast()
                    .expressions()
                    .get(id)
                    .ok()
                    .and_then(|n| self.reference_symbol(n.span()))
                    .is_some_and(|s| {
                        (boolean || self.typed.parameter_mode(s) == Some(ParameterMode::Borrow))
                            && self.names.symbols().get(s.index()).is_some_and(|s| {
                                s.kind() == crate::name_resolution::SymbolKind::ValueParameter
                            })
                    })
                    && !self.is_constant_use(id)
                    && self
                        .typed
                        .expression_type(id)
                        .and_then(|t| self.typed.types().get(t))
                        == Some(&TypeKind::Builtin(if boolean {
                            BuiltinType::Boolean
                        } else {
                            BuiltinType::String
                        }))
            },
            |id, init| self.no_carry_call(id, init),
        ) else {
            return;
        };
        if self.borrowed_call_source(plan.source_call) != Some(plan.source_argument) {
            return;
        }
        self.no_carry = Some(Session {
            frame: core::Frame::new(plan.clone(), entry.clone()),
            plan,
            entry,
            bindings: self.borrow_results.bindings.len(),
            ends: self.borrow_results.ends.len(),
            loans: self.loans.len(),
            diagnostics: self.diagnostics.len(),
            cursor: self.loans.len(),
        });
    }
    pub(super) fn no_carry_claim(
        &mut self,
        scope: StatementId,
        state: &State,
    ) -> Option<core::ScopeTicket> {
        if self.no_carry.as_ref()?.plan.body != scope {
            return None;
        }
        let state = self.resource_snapshot(state);
        self.no_carry.as_mut()?.frame.claim(scope, &state)
    }
    pub(super) fn no_carry_allows(
        &self,
        binding: SymbolId,
        initializer: ExpressionId,
        marker: Span,
    ) -> bool {
        self.no_carry
            .as_ref()
            .is_some_and(|s| s.frame.allows(binding, initializer, marker))
    }
    pub(super) fn no_carry_acquired(&mut self, binding: SymbolId, state: &State) {
        if self.no_carry.is_none() {
            return;
        }
        let live = self.resource_snapshot(state);
        let Some(s) = self.no_carry.as_mut() else {
            return;
        };
        let p = &s.plan;
        let place = OwnershipPlace::new(p.root, Vec::new());
        let facts = &self.loans[s.cursor..];
        let exact = binding == p.binding
            && facts.len() == 1
            && facts[0].call() == p.source_call
            && facts[0].argument() == p.source_argument
            && facts[0].kind() == LoanKind::Shared
            && facts[0].target() == &LoanTarget::Place(place.clone())
            && self.borrow_results.bindings.len() == s.bindings + 1
            && self.borrow_results.ends.len() == s.ends
            && self.diagnostics.len() == s.diagnostics
            && live.borrow_bindings.get(&binding) == Some(&place)
            && live.non_owning.get(&binding) == Some(&p.marker)
            && live.loans.len() == s.entry.loans.len() + 1
            && live.loans.last().is_some_and(|l| {
                l.owner == ActiveLoanOwner::BorrowBinding(binding)
                    && l.kind == LoanKind::Shared
                    && l.target == ActiveLoanTarget::Place(place)
                    && l.origin == facts[0].begin_span()
            });
        let mut restored = live.clone();
        restored.borrow_bindings.remove(&binding);
        restored.non_owning.remove(&binding);
        restored
            .loans
            .retain(|l| l.owner != ActiveLoanOwner::BorrowBinding(binding));
        s.frame
            .acquired(binding, live, exact && restored == s.entry);
        s.cursor = self.loans.len();
    }
    pub(super) fn no_carry_prepare(
        &mut self,
        call: ExpressionId,
        flows: &Flows,
    ) -> Option<core::ReturnTicket<State>> {
        if call == self.no_carry.as_ref()?.plan.source_call {
            return None;
        }
        let before = self.resource_snapshot(flows.next.as_ref()?);
        let s = self.no_carry.as_mut()?;
        let argument = s
            .plan
            .consumers
            .iter()
            .find(|(_, c, _)| *c == call)
            .map(|(_, _, a)| *a);
        let facts = &self.loans[s.cursor..];
        let place = OwnershipPlace::new(s.plan.root, Vec::new());
        let active = before
            .loans
            .iter()
            .filter(|l| l.owner == ActiveLoanOwner::Call(call))
            .collect::<Vec<_>>();
        let exact = flows.breaks.is_none()
            && flows.continues.is_none()
            && self.diagnostics.len() == s.diagnostics
            && facts.len() == 1
            && Some(facts[0].argument()) == argument
            && facts[0].call() == call
            && facts[0].kind() == LoanKind::Shared
            && facts[0].target() == &LoanTarget::Place(place.clone())
            && active.len() == 1
            && active[0].kind == LoanKind::Shared
            && active[0].target == ActiveLoanTarget::Place(place)
            && active[0].origin == facts[0].begin_span();
        let mut expected = before.clone();
        expected
            .loans
            .retain(|l| l.owner != ActiveLoanOwner::Call(call));
        s.cursor = self.loans.len();
        s.frame.prepare_return(call, &before, &expected, exact)
    }
    pub(super) fn no_carry_release(
        &mut self,
        ticket: Option<core::ReturnTicket<State>>,
        flows: &Flows,
    ) -> Option<core::Returned<State>> {
        self.no_carry.as_ref()?;
        let ticket = ticket?;
        let actual = flows.next.as_ref().map(|s| self.resource_snapshot(s));
        self.no_carry
            .as_mut()?
            .frame
            .released(ticket, actual.as_ref())
    }
    pub(super) fn no_carry_complete(
        &mut self,
        ticket: Option<core::Returned<State>>,
        flows: &Flows,
    ) {
        if self.no_carry.is_none() || ticket.is_none() {
            return;
        }
        let actual = flows.next.as_ref().map(|s| self.resource_snapshot(s));
        if let (Some(s), Some(t), Some(a)) = (self.no_carry.as_mut(), ticket, actual) {
            s.frame.complete(
                t,
                &a,
                flows.breaks.is_none()
                    && flows.continues.is_none()
                    && self.diagnostics.len() == s.diagnostics,
            );
        }
    }
    pub(super) fn no_carry_removing(
        &mut self,
        binding: SymbolId,
        statement: StatementId,
        state: &State,
    ) -> Option<core::EndTicket> {
        self.no_carry.as_ref()?;
        let state = self.resource_snapshot(state);
        self.no_carry
            .as_mut()?
            .frame
            .removing(binding, statement, &state)
    }
    pub(super) fn no_carry_removed(&mut self, ticket: Option<core::EndTicket>, state: &State) {
        if self.no_carry.is_none() || ticket.is_none() {
            return;
        }
        let state = self.resource_snapshot(state);
        if let (Some(s), Some(t)) = (self.no_carry.as_mut(), ticket) {
            let exact = self.borrow_results.ends.len() == s.ends + 1
                && self.borrow_results.ends.last().is_some_and(|e| {
                    e.binding == s.plan.binding
                        && s.plan
                            .consumers
                            .last()
                            .is_some_and(|(last, _, _)| e.point == DropPoint::AfterStatement(*last))
                });
            s.frame.removed(t, &state, exact);
        }
    }
    fn no_carry_facts(&self, s: &Session) -> bool {
        let facts = &self.loans[s.loans..];
        let binding_matches = self
            .borrow_results
            .bindings
            .get(s.bindings)
            .is_some_and(|f| {
                f.binding == s.plan.binding
                    && f.initializer == s.plan.initializer
                    && f.marker == s.plan.marker
                    && f.parent.is_none()
                    && f.storage == crate::ownership_checking::BorrowBindingStorage::BorrowedStorage
                    && f.origin == LoanTarget::Place(OwnershipPlace::new(s.plan.root, Vec::new()))
                    && f.source_loan.is_some_and(|l| {
                        l.call() == s.plan.source_call && l.argument() == s.plan.source_argument
                    })
            });
        let end_matches = self.borrow_results.ends.get(s.ends).is_some_and(|e| {
            e.binding == s.plan.binding
                && s.plan.consumers.last().is_some_and(|(statement, _, _)| {
                    e.point == DropPoint::AfterStatement(*statement)
                })
        });
        binding_matches
            && end_matches
            && facts.len() == 1 + s.plan.consumers.len()
            && facts.first().is_some_and(|f| {
                f.call() == s.plan.source_call && f.argument() == s.plan.source_argument
            })
            && facts
                .iter()
                .skip(1)
                .zip(&s.plan.consumers)
                .all(|(f, (_, call, arg))| {
                    f.call() == *call
                        && f.argument() == *arg
                        && f.kind() == LoanKind::Shared
                        && f.target()
                            == &LoanTarget::Place(OwnershipPlace::new(s.plan.root, Vec::new()))
                })
            && self.borrow_results.bindings.len() == s.bindings + 1
            && self.borrow_results.ends.len() == s.ends + 1
            && self.diagnostics.len() == s.diagnostics
            && s.cursor == self.loans.len()
    }
    pub(super) fn no_carry_close(
        &mut self,
        scope: StatementId,
        ticket: Option<core::ScopeTicket>,
        flows: &Flows,
    ) {
        if self
            .no_carry
            .as_ref()
            .is_none_or(|session| session.plan.body != scope)
        {
            return;
        }
        let actual = flows.next.as_ref().map(|s| self.resource_snapshot(s));
        let exact = self
            .no_carry
            .as_ref()
            .is_some_and(|s| self.no_carry_facts(s));
        if let Some(s) = self.no_carry.as_mut() {
            s.frame.close(
                ticket,
                actual.as_ref(),
                flows.breaks.is_none() && flows.continues.is_none(),
                exact,
            );
        }
    }
    pub(super) fn no_carry_finish(&mut self, flows: &Flows) -> Result<(), OwnershipCheckingError> {
        let Some(s) = self.no_carry.take() else {
            return Ok(());
        };
        let exact = self.no_carry_facts(&s);
        let actual = flows.next.as_ref().map(|s| self.resource_snapshot(s));
        if s.frame
            .finish(
                actual.as_ref(),
                flows.breaks.is_none() && flows.continues.is_none(),
                exact,
            )
            .is_none()
        {
            self.emit_borrow_binding_diagnostic(
                crate::diagnostic::codes::UNSUPPORTED_BORROW_FLOW,
                "loop-local borrow does not close before its backedge",
                s.plan.marker,
            )?;
        }
        Ok(())
    }
}
