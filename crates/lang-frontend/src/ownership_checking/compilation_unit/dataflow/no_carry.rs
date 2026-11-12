//! Production no-carry frame authority; independent of optional observation.
use super::{ActiveLoanOwner, ActiveLoanTarget, Checker, Flows, State, UnitSymbolId};
use crate::{
    ast::{ExpressionId, StatementId},
    ownership_checking::{
        LoanKind, OwnershipBindingKind, UnitDropPoint, UnitLoanTarget as LoanTarget,
        loop_no_carry::{self as core, shape::Name},
    },
    parser::{Expression, NameMarker},
    source::Span,
    type_checking::{BuiltinType, ParameterMode, UnitCallTarget, UnitStatementId, UnitTypeKind},
};
pub(super) struct Session {
    frame: core::Frame<State, UnitSymbolId>,
    entry: State,
    bindings: usize,
    ends: usize,
    loans: usize,
    diagnostics: usize,
}
impl Checker<'_> {
    fn no_carry_parameter(&self, id: ExpressionId, boolean: bool) -> bool {
        let Ok(node) = self.parsed.ast().expressions().get(id) else {
            return false;
        };
        if !matches!(node.payload(), Expression::Name) || self.is_constant_use(id) {
            return false;
        }
        let Some(symbol) = self.reference_symbol(node.span()) else {
            return false;
        };
        self.names
            .source_units()
            .iter()
            .find(|u| u.source_unit() == symbol.source_unit())
            .and_then(|u| u.resolution().symbols().get(symbol.symbol().index()))
            .is_some_and(|s| s.kind() == crate::name_resolution::SymbolKind::ValueParameter)
            && (boolean
                || self
                    .bindings
                    .get(&symbol)
                    .is_some_and(|b| b.kind() == OwnershipBindingKind::Shared))
            && (!boolean || self.ordinary_condition_outcomes(id) == (true, true))
            && self
                .typed
                .expression_type(self.unit_expression(id))
                .and_then(|t| self.typed.types().get(t))
                == Some(&UnitTypeKind::Builtin(if boolean {
                    BuiltinType::Boolean
                } else {
                    BuiltinType::String
                }))
    }
    fn no_carry_call(&self, id: ExpressionId, initializer: bool) -> bool {
        let Some(call) = self.typed.call(self.unit_expression(id)) else {
            return false;
        };
        call.receiver().is_none()
            && !call.aborts()
            && matches!(call.target(), UnitCallTarget::Declaration(_))
            && call.borrow_return().is_some() == initializer
            && self.typed.types().get(call.return_type())
                == Some(&UnitTypeKind::Builtin(if initializer {
                    BuiltinType::String
                } else {
                    BuiltinType::Unit
                }))
            && call.arguments().len() == 1
            && call.arguments().iter().all(|a| {
                a.mode() == ParameterMode::Borrow
                    && !a.crosses_thread()
                    && self.typed.types().get(a.parameter_type())
                        == Some(&UnitTypeKind::Builtin(BuiltinType::String))
            })
    }
    pub(super) fn no_carry_begin(
        &self,
        repeated: StatementId,
        body: StatementId,
        state: &State,
    ) -> Option<Session> {
        if !self.diagnostics.is_empty()
            || !state.loans.is_empty()
            || !state.borrow_bindings.is_empty()
            || !state.non_owning.is_empty()
        {
            return None;
        }
        let entry = self.resource_snapshot(state);
        if !self.simple_resources(&entry) {
            return None;
        }
        let plan = core::shape::plan(
            self.parsed,
            repeated,
            body,
            |name| match name {
                Name::Declaration(n) => self.marker_symbol(n).copied(),
                Name::Reference(s) => self.reference_symbol(s),
            },
            |id, b| self.no_carry_parameter(id, b),
            |id, b| self.no_carry_call(id, b),
        )?;
        if self.borrowed_call_source(plan.source_call) != Some(plan.source_argument) {
            return None;
        }
        if plan.binding.source_unit() != self.source_unit
            || plan.root.source_unit() != self.source_unit
        {
            return None;
        }
        Some(Session {
            frame: core::Frame::new(plan, entry.clone()),
            entry,
            bindings: self.borrow_results.bindings.len(),
            ends: self.borrow_results.ends.len(),
            loans: self.loans.len(),
            diagnostics: self.diagnostics.len(),
        })
    }
    pub(super) fn no_carry_claim(
        &mut self,
        body: StatementId,
        state: &State,
    ) -> Option<core::ScopeTicket> {
        if self.no_carry.as_ref()?.frame.plan().body != body {
            return None;
        }
        let actual = self.resource_snapshot(state);
        self.no_carry.as_mut()?.frame.claim(body, &actual)
    }
    pub(super) fn no_carry_allows(
        &self,
        name: NameMarker,
        initializer: ExpressionId,
        marker: Span,
    ) -> bool {
        self.marker_symbol(name).is_some_and(|binding| {
            self.no_carry.as_ref().is_some_and(|s| {
                s.frame.plan().source == self.parsed.source_id()
                    && s.frame.allows(*binding, initializer, marker)
            })
        })
    }
    fn no_carry_facts(&self, s: &Session) -> bool {
        let p = s.frame.plan();
        if self.diagnostics.len() != s.diagnostics
            || self.borrow_results.bindings.len() != s.bindings + 1
            || self.borrow_results.ends.len() != s.ends + 1
        {
            return false;
        }
        let fact = &self.borrow_results.bindings[s.bindings];
        let Some((last, _, _)) = p.consumers.last() else {
            return false;
        };
        let end = &self.borrow_results.ends[s.ends];
        let expected = std::iter::once((p.source_call, p.source_argument))
            .chain(p.consumers.iter().map(|(_, c, a)| (*c, *a)))
            .collect::<Vec<_>>();
        let Some(loans) = self.loans.get(s.loans..) else {
            return false;
        };
        let origin_matches = match &fact.origin {
            LoanTarget::Place(place) => place.root() == p.root && place.is_root(),
            _ => false,
        };
        let loans_match = loans.len() == expected.len()
            && loans.iter().zip(expected).all(|(loan, (call, arg))| {
                let target_matches = match loan.target() {
                    LoanTarget::Place(place) => place.root() == p.root && place.is_root(),
                    _ => false,
                };
                loan.call() == self.unit_expression(call)
                    && loan.argument() == self.unit_expression(arg)
                    && loan.kind() == LoanKind::Shared
                    && target_matches
            });
        fact.binding == p.binding
            && fact.initializer == self.unit_expression(p.initializer)
            && fact.marker == p.marker
            && fact.parent.is_none()
            && fact.storage == crate::ownership_checking::BorrowBindingStorage::BorrowedStorage
            && fact.source_loan.is_some_and(|source| {
                source.call == self.unit_expression(p.source_call)
                    && source.argument == self.unit_expression(p.source_argument)
            })
            && origin_matches
            && end.binding == p.binding
            && end.point
                == UnitDropPoint::AfterStatement(UnitStatementId::new(self.source_unit, *last))
            && loans_match
    }
    pub(super) fn no_carry_acquired(&mut self, binding: UnitSymbolId, state: &State) {
        let Some(s) = self.no_carry.as_ref() else {
            return;
        };
        if s.frame.plan().binding != binding {
            return;
        }
        let p = s.frame.plan();
        let actual = self.resource_snapshot(state);
        let mut without = actual.clone();
        let place = without.borrow_bindings.remove(&binding);
        let marker = without.non_owning.remove(&binding);
        let owned = without
            .loans
            .iter()
            .filter(|l| l.owner == ActiveLoanOwner::BorrowBinding(binding))
            .collect::<Vec<_>>();
        let source = self.loans.get(s.loans);
        let active_matches = owned.len() == 1
            && owned.first().is_some_and(|loan| {
                let target_matches = match &loan.target {
                    ActiveLoanTarget::Place(target) => Some(target) == place.as_ref(),
                    _ => false,
                };
                let source_matches = source.is_some_and(|fact| {
                    let same_place = match fact.target() {
                        LoanTarget::Place(target) => Some(target) == place.as_ref(),
                        _ => false,
                    };
                    fact.begin_span() == loan.origin && same_place
                });
                loan.kind == LoanKind::Shared && !loan.reserved && target_matches && source_matches
            });
        let exact = self.borrow_results.bindings.len() == s.bindings + 1
            && self.loans.len() == s.loans + 1
            && marker == Some(p.marker)
            && place
                .as_ref()
                .is_some_and(|place| place.root() == p.root && place.is_root())
            && source.is_some_and(|fact| {
                fact.call() == self.unit_expression(p.source_call)
                    && fact.argument() == self.unit_expression(p.source_argument)
                    && fact.kind() == LoanKind::Shared
            })
            && active_matches;
        without
            .loans
            .retain(|l| l.owner != ActiveLoanOwner::BorrowBinding(binding));
        let exact = exact && without == s.entry;
        self.no_carry
            .as_mut()
            .unwrap()
            .frame
            .acquired(binding, actual, exact);
    }
    pub(super) fn no_carry_prepare(
        &mut self,
        call: ExpressionId,
        flows: &Flows,
    ) -> Option<core::ReturnTicket<State>> {
        let s = self.no_carry.as_ref()?;
        let p = s.frame.plan();
        let (_, _, argument) = p.consumers.iter().find(|(_, c, _)| *c == call)?;
        let state = flows.next.as_ref()?;
        let before = self.resource_snapshot(state);
        let mut expected = before.clone();
        let owner = ActiveLoanOwner::Call(self.unit_expression(call));
        let active = state
            .loans
            .iter()
            .filter(|l| l.owner == owner)
            .collect::<Vec<_>>();
        let facts = self
            .loans
            .iter()
            .filter(|f| f.call() == self.unit_expression(call))
            .collect::<Vec<_>>();
        let exact = flows.breaks.is_none()
            && flows.continues.is_none()
            && active.len() == 1
            && facts.len() == 1
            && facts[0].argument() == self.unit_expression(*argument)
            && facts[0].kind() == LoanKind::Shared
            && active[0].origin == facts[0].begin_span()
            && active[0].kind == LoanKind::Shared
            && !active[0].reserved
            && matches!((&active[0].target,facts[0].target()),(ActiveLoanTarget::Place(a),LoanTarget::Place(b)) if a==b&&a.root()==p.root&&a.is_root());
        expected.loans.retain(|l| l.owner != owner);
        self.no_carry
            .as_mut()?
            .frame
            .prepare_return(call, &before, &expected, exact)
    }
    pub(super) fn no_carry_released(
        &mut self,
        ticket: Option<core::ReturnTicket<State>>,
        flows: &Flows,
    ) -> Option<core::Returned<State>> {
        let ticket = ticket?;
        let state = flows.next.as_ref().map(|s| self.resource_snapshot(s));
        self.no_carry
            .as_mut()?
            .frame
            .released(ticket, state.as_ref())
    }
    pub(super) fn no_carry_complete(
        &mut self,
        ticket: Option<core::Returned<State>>,
        flows: &Flows,
    ) {
        let Some(ticket) = ticket else {
            return;
        };
        let Some(state) = flows.next.as_ref() else {
            return;
        };
        let actual = self.resource_snapshot(state);
        if let Some(s) = self.no_carry.as_mut() {
            s.frame.complete(
                ticket,
                &actual,
                flows.breaks.is_none() && flows.continues.is_none(),
            );
        }
    }
    pub(super) fn no_carry_removing(
        &mut self,
        binding: UnitSymbolId,
        statement: StatementId,
        state: &State,
    ) -> Option<core::EndTicket> {
        if self.no_carry.as_ref()?.frame.plan().binding != binding {
            return None;
        }
        let before = self.resource_snapshot(state);
        self.no_carry
            .as_mut()?
            .frame
            .removing(binding, statement, &before)
    }
    pub(super) fn no_carry_removed(&mut self, ticket: Option<core::EndTicket>, state: &State) {
        let Some(ticket) = ticket else {
            return;
        };
        let actual = self.resource_snapshot(state);
        let exact = self
            .no_carry
            .as_ref()
            .is_some_and(|s| self.no_carry_facts(s));
        if let Some(s) = self.no_carry.as_mut() {
            s.frame.removed(ticket, &actual, exact);
        }
    }
    pub(super) fn no_carry_close(
        &mut self,
        scope: StatementId,
        ticket: Option<core::ScopeTicket>,
        flows: &Flows,
    ) -> Result<(), super::OwnershipCheckingError> {
        let Some(s) = self.no_carry.as_ref() else {
            return Ok(());
        };
        if s.frame.plan().body != scope {
            return Ok(());
        }
        let marker = s.frame.plan().marker;
        let exact = self.no_carry_facts(s);
        let actual = flows.next.as_ref().map(|s| self.resource_snapshot(s));
        if !self.no_carry.as_mut().unwrap().frame.close(
            ticket,
            actual.as_ref(),
            flows.breaks.is_none() && flows.continues.is_none(),
            exact,
        ) {
            self.emit_borrow_binding_diagnostic(
                crate::diagnostic::codes::UNSUPPORTED_BORROW_FLOW,
                "loop-local borrow does not close before body cleanup",
                marker,
            )?;
        }
        Ok(())
    }
    pub(super) fn no_carry_finish(
        &mut self,
        session: Option<Session>,
        flows: &Flows,
    ) -> Result<(), super::OwnershipCheckingError> {
        let Some(s) = session else {
            return Ok(());
        };
        let marker = s.frame.plan().marker;
        let exact = self.no_carry_facts(&s);
        let actual = flows.next.as_ref().map(|s| self.resource_snapshot(s));
        if let Some(plan) = s.frame.finish(
            actual.as_ref(),
            flows.breaks.is_none() && flows.continues.is_none(),
            exact,
        ) {
            self.no_carry_proofs.push(plan);
        } else {
            self.emit_borrow_binding_diagnostic(
                crate::diagnostic::codes::UNSUPPORTED_BORROW_FLOW,
                "loop-local borrow does not restore the normal backedge",
                marker,
            )?;
        }
        Ok(())
    }
}

impl Checker<'_> {
    pub(super) fn finish_no_carry_proofs(
        &mut self,
        ends: &[crate::ownership_checking::BorrowBindingEndFact<UnitSymbolId, UnitDropPoint>],
    ) -> Result<Vec<super::super::UnitLoopNoCarryFact>, super::OwnershipCheckingError> {
        let mut facts = Vec::new();
        for plan in std::mem::take(&mut self.no_carry_proofs) {
            let marker = plan.marker;
            if let Some(fact) = super::super::UnitLoopNoCarryFact::new(self.source_unit, plan)
                && ends
                    .iter()
                    .filter(|end| end.binding == fact.binding())
                    .count()
                    == 1
                && ends
                    .iter()
                    .any(|end| end.binding == fact.binding() && end.point == fact.end())
            {
                facts.push(fact);
            } else {
                self.emit_borrow_binding_diagnostic(
                    crate::diagnostic::codes::UNSUPPORTED_BORROW_FLOW,
                    "loop-local borrow final End does not match its closed core frame",
                    marker,
                )?;
            }
        }
        Ok(facts)
    }
}
