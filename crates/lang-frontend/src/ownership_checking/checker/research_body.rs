//! Independent external-parameter temporary-loan body research; no local binding authority.
use super::{Checker, Flows, State};
use crate::{
    ast::{ExpressionId, StatementId},
    ownership_checking::{
        BorrowLoopBinding, BorrowLoopExpression, LoanKind, LoanTarget,
        research_borrow_consumer_body as body,
    },
    parser::{Expression, Statement},
};
pub(super) struct Acquisition {
    before: State,
    facts: usize,
    call: ExpressionId,
    argument: ExpressionId,
    binding: crate::name_resolution::SymbolId,
}
impl Checker<'_> {
    fn body_expression(&self, id: ExpressionId) -> BorrowLoopExpression {
        BorrowLoopExpression::Single {
            source: self.parsed.source_id(),
            expression: id,
        }
    }
    fn body_binding(&self, binding: crate::name_resolution::SymbolId) -> BorrowLoopBinding {
        BorrowLoopBinding::Single {
            source: self.parsed.source_id(),
            symbol: binding,
        }
    }
    fn body_parameter(&self, expression: ExpressionId) -> Option<crate::name_resolution::SymbolId> {
        let node = self.parsed.ast().expressions().get(expression).ok()?;
        if !matches!(node.payload(), Expression::Name) {
            return None;
        }
        let binding = self.reference_symbol(node.span())?;
        (self.typed.parameter_mode(binding) == Some(crate::type_checking::ParameterMode::Borrow))
            .then_some(binding)
    }
    pub(super) fn research_body_loop_start(
        &self,
        statement: Option<StatementId>,
        scope: StatementId,
        state: &State,
    ) -> Option<body::LoopTicket<State>> {
        if !body::capturing()
            || !self.diagnostics.is_empty()
            || !state.loans.is_empty()
            || !state.borrow_bindings.is_empty()
        {
            return None;
        }
        let snapshot = self.resource_snapshot(state);
        if !self.simple_resources(&snapshot) {
            return None;
        }
        let node = self.parsed.ast().statements().get(statement?).ok()?;
        let Statement::While {
            condition,
            body: actual,
            ..
        } = node.payload()
        else {
            return None;
        };
        if *actual != scope
            || !self.research_boolean_parameter(*condition)
            || !matches!(
                self.parsed
                    .ast()
                    .expressions()
                    .get(*condition)
                    .ok()?
                    .payload(),
                Expression::Name
            )
        {
            return None;
        }
        let (Statement::Block { elements } | Statement::ControlBody { elements }) =
            self.parsed.ast().statements().get(scope).ok()?.payload()
        else {
            return None;
        };
        if elements.is_empty() {
            return None;
        }
        let mut calls = Vec::new();
        for statement in elements {
            let Statement::Expression { expression } = self
                .parsed
                .ast()
                .statements()
                .get(*statement)
                .ok()?
                .payload()
            else {
                return None;
            };
            let expression = *expression;
            if !self.research_direct_string_call(expression, false) {
                return None;
            }
            let Expression::Call {
                callee, arguments, ..
            } = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .ok()?
                .payload()
            else {
                return None;
            };
            if !matches!(
                self.parsed.ast().expressions().get(*callee).ok()?.payload(),
                Expression::Name
            ) || arguments.is_empty()
            {
                return None;
            }
            let typed = self.typed.call(expression)?;
            if typed.arguments().len() != arguments.len()
                || typed.arguments().iter().any(|a| {
                    a.mode() != crate::type_checking::ParameterMode::Borrow || a.crosses_thread()
                })
            {
                return None;
            }
            let mut identities = Vec::new();
            for argument in arguments {
                let binding = self.body_parameter(argument.value)?;
                identities.push((
                    self.body_expression(argument.value),
                    self.body_binding(binding),
                ));
            }
            calls.push((self.body_expression(expression), identities));
        }
        body::begin(scope, node.span(), &snapshot, calls, self.body_checkpoint())
    }
    fn body_checkpoint(&self) -> (usize, usize, usize, usize) {
        (
            self.borrow_results.bindings.len(),
            self.borrow_results.ends.len(),
            self.diagnostics.len(),
            self.loans.len(),
        )
    }
    pub(super) fn research_body_enter(
        &self,
        scope: StatementId,
        state: &State,
    ) -> Option<body::BodyTicket> {
        if !body::capturing() {
            return None;
        }
        body::enter(scope, &self.resource_snapshot(state))
    }
    pub(super) fn research_body_close(&self, ticket: Option<body::BodyTicket>, flows: &Flows) {
        if ticket.is_none() {
            return;
        }
        let normal = flows.next.as_ref().map(|s| self.resource_snapshot(s));
        body::close(
            ticket,
            normal.as_ref(),
            flows.breaks.is_none() && flows.continues.is_none(),
            self.body_checkpoint(),
        );
    }
    pub(super) fn research_body_finish(
        &self,
        ticket: Option<body::LoopTicket<State>>,
        flows: &Flows,
    ) {
        if ticket.is_none() {
            return;
        }
        let normal = flows.next.as_ref().map(|s| self.resource_snapshot(s));
        body::finish(
            ticket,
            normal.as_ref(),
            flows.breaks.is_none() && flows.continues.is_none(),
            self.body_checkpoint(),
        );
    }
    pub(super) fn research_body_acquiring(
        &self,
        call: ExpressionId,
        argument: ExpressionId,
        state: &State,
    ) -> Option<Acquisition> {
        if !body::capturing() {
            return None;
        }
        let binding = self.body_parameter(argument)?;
        if !body::expects::<State>(
            self.body_expression(call),
            self.body_expression(argument),
            self.body_binding(binding),
        ) {
            return None;
        }
        Some(Acquisition {
            before: self.resource_snapshot(state),
            facts: self.loans.len(),
            call,
            argument,
            binding,
        })
    }
    pub(super) fn research_body_acquired(
        &self,
        token: Option<Acquisition>,
        actual: Option<&State>,
    ) {
        let Some(t) = token else {
            return;
        };
        let Some(actual) = actual else {
            return;
        };
        let call = t.call;
        let argument = t.argument;
        let after = self.resource_snapshot(actual);
        let mut stripped = after.clone();
        let added = stripped.loans.pop();
        let place = self
            .place(argument)
            .ok()
            .flatten()
            .map(|p| self.canonical_borrow_place(&p, &t.before));
        let exact = self.diagnostics.is_empty()
            && t.facts.checked_add(1) == Some(self.loans.len())
            && stripped == t.before
            && added.as_ref().is_some_and(|loan| {
                let fact = &self.loans[t.facts];
                loan.owner == super::loan::ActiveLoanOwner::Call(call)
                    && loan.kind == LoanKind::Shared
                    && place.as_ref().is_some_and(|p| {
                        loan.target == super::loan::ActiveLoanTarget::Place(p.clone())
                            && fact.target() == &LoanTarget::Place(p.clone())
                    })
                    && fact.call() == call
                    && fact.argument() == argument
                    && fact.kind() == LoanKind::Shared
                    && loan.origin == fact.begin_span()
            });
        body::acquired(
            self.body_expression(call),
            self.body_expression(argument),
            self.body_binding(t.binding),
            &t.before,
            &after,
            exact,
            t.facts,
        );
    }
    pub(super) fn research_body_prepare_return(
        &self,
        call: ExpressionId,
        flows: &Flows,
    ) -> Option<body::Prepared<State>> {
        if !body::capturing()
            || !self.diagnostics.is_empty()
            || flows.breaks.is_some()
            || flows.continues.is_some()
        {
            return None;
        }
        let before = self.resource_snapshot(flows.next.as_ref()?);
        let mut expected = before.clone();
        expected
            .loans
            .retain(|l| l.owner != super::loan::ActiveLoanOwner::Call(call));
        body::prepare(self.body_expression(call), &before, &expected)
    }
    pub(super) fn research_body_release_return(
        &self,
        t: Option<body::Prepared<State>>,
        flows: &Flows,
    ) -> Option<body::Released<State>> {
        t.as_ref()?;
        let actual = flows.next.as_ref().map(|s| self.resource_snapshot(s));
        body::release(t, actual.as_ref())
    }
    pub(super) fn research_body_complete_return(
        &self,
        t: Option<body::Released<State>>,
        flows: &Flows,
    ) {
        if t.is_none() {
            return;
        }
        let actual = flows.next.as_ref().map(|s| self.resource_snapshot(s));
        body::complete(
            t,
            actual.as_ref(),
            self.diagnostics.is_empty() && flows.breaks.is_none() && flows.continues.is_none(),
        );
    }
}
