//! Core, single-body no-carry lifecycle. Adapters supply actual checker snapshots.
//! No research observer or ambient session grants permission through this protocol.
use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    source::{SourceId, Span},
};
use std::rc::Rc;

mod counter_break_shape;
mod counter_shape;
pub(crate) mod shape;
#[cfg(test)]
mod tests;

pub(crate) enum Proof<B> {
    Straight(Plan<B>),
    Branch(ExpressionId, [Plan<B>; 2]),
    CounterBranch(ExpressionId, [Plan<B>; 2], CounterPlan<B>),
    CounterBreakBranch(
        ExpressionId,
        [Plan<B>; 2],
        CounterPlan<B>,
        CounterBreakPlan<B>,
    ),
    BreakBranch(ExpressionId, [Plan<B>; 2], BreakPlan),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BreakPlan {
    pub(crate) loop_statement: StatementId,
    pub(crate) loop_body: StatementId,
    pub(crate) break_statement: StatementId,
    pub(crate) break_expression: ExpressionId,
}

#[derive(Clone)]
pub(crate) struct CounterBreakPlan<B> {
    pub(crate) prefix: Option<ExpressionId>,
    pub(crate) control: ExpressionId,
    pub(crate) condition: ExpressionId,
    pub(crate) flag: B,
    pub(crate) body: StatementId,
    pub(crate) exit: BreakPlan,
}

#[derive(Clone)]
pub(crate) struct CounterPlan<B> {
    pub(crate) counter: B,
    pub(crate) rounds: B,
    pub(crate) initializer: ExpressionId,
    pub(crate) initial_statement: StatementId,
    pub(crate) condition: ExpressionId,
    pub(crate) update: StatementId,
    pub(crate) decrement: ExpressionId,
    pub(crate) decrement_operand: ExpressionId,
}

#[derive(Clone)]
pub(crate) struct Plan<B> {
    pub(crate) source: SourceId,
    pub(crate) loop_statement: StatementId,
    pub(crate) body: StatementId,
    pub(crate) callable: ItemId,
    pub(crate) marker: Span,
    pub(crate) binding: B,
    pub(crate) root: B,
    pub(crate) initializer: ExpressionId,
    pub(crate) source_call: ExpressionId,
    pub(crate) source_argument: ExpressionId,
    pub(crate) consumers: Vec<(StatementId, ExpressionId, ExpressionId)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Unclaimed,
    Acquiring,
    Live,
    Returning,
    Released,
    Removing,
    Ended,
    Closed,
}

pub(crate) struct ScopeTicket {
    identity: Rc<()>,
    body: StatementId,
}
pub(crate) struct ReturnTicket<S> {
    identity: Rc<()>,
    index: usize,
    expected: S,
}
pub(crate) struct Returned<S> {
    identity: Rc<()>,
    index: usize,
    actual: S,
}
pub(crate) struct EndTicket {
    identity: Rc<()>,
}

pub(crate) struct Frame<S, B> {
    identity: Rc<()>,
    entry: S,
    plan: Plan<B>,
    live: Option<S>,
    next: usize,
    phase: Phase,
    valid: bool,
}

impl<S: Clone + PartialEq, B: Copy + Eq> Frame<S, B> {
    pub(crate) fn new(plan: Plan<B>, entry: S) -> Self {
        let valid = !plan.consumers.is_empty()
            && plan.binding != plan.root
            && plan.initializer == plan.source_call
            && plan
                .consumers
                .iter()
                .enumerate()
                .all(|(i, (statement, call, argument))| {
                    call != argument
                        && *call != plan.initializer
                        && plan.consumers[..i]
                            .iter()
                            .all(|(s, c, a)| s != statement && c != call && a != argument)
                });
        Self {
            identity: Rc::new(()),
            entry,
            plan,
            live: None,
            next: 0,
            phase: Phase::Unclaimed,
            valid,
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.valid && self.phase == Phase::Closed
    }

    pub(crate) fn plan(&self) -> &Plan<B> {
        &self.plan
    }

    pub(crate) fn claim(&mut self, scope: StatementId, state: &S) -> Option<ScopeTicket> {
        self.valid &=
            self.phase == Phase::Unclaimed && scope == self.plan.body && *state == self.entry;
        if !self.valid {
            return None;
        }
        self.phase = Phase::Acquiring;
        Some(ScopeTicket {
            identity: self.identity.clone(),
            body: scope,
        })
    }

    pub(crate) fn allows(&self, binding: B, initializer: ExpressionId, marker: Span) -> bool {
        self.valid
            && self.phase == Phase::Acquiring
            && binding == self.plan.binding
            && initializer == self.plan.initializer
            && marker == self.plan.marker
    }

    pub(crate) fn acquired(&mut self, binding: B, live: S, exact: bool) {
        #[cfg(test)]
        if tests::at(1) {
            return;
        }
        self.valid &= exact && self.phase == Phase::Acquiring && binding == self.plan.binding;
        if self.valid {
            self.live = Some(live);
            self.phase = Phase::Live;
        }
    }

    pub(crate) fn prepare_return(
        &mut self,
        call: ExpressionId,
        _before: &S,
        expected: &S,
        exact: bool,
    ) -> Option<ReturnTicket<S>> {
        // `exact` certifies actual temporary-call facts against `before`; only their
        // removal may reconstruct the frozen binding-live state.
        self.valid &= exact
            && self.phase == Phase::Live
            && self
                .plan
                .consumers
                .get(self.next)
                .is_some_and(|(_, c, _)| *c == call)
            && self.live.as_ref() == Some(expected);
        if !self.valid {
            return None;
        }
        self.phase = Phase::Returning;
        Some(ReturnTicket {
            identity: self.identity.clone(),
            index: self.next,
            expected: expected.clone(),
        })
    }

    pub(crate) fn released(
        &mut self,
        ticket: ReturnTicket<S>,
        actual: Option<&S>,
    ) -> Option<Returned<S>> {
        #[cfg(test)]
        if tests::at(2) {
            return None;
        }
        self.valid &= self.phase == Phase::Returning
            && Rc::ptr_eq(&ticket.identity, &self.identity)
            && ticket.index == self.next
            && actual == Some(&ticket.expected)
            && actual == self.live.as_ref();
        if !self.valid {
            return None;
        }
        self.phase = Phase::Released;
        Some(Returned {
            identity: self.identity.clone(),
            index: self.next,
            actual: ticket.expected,
        })
    }

    pub(crate) fn complete(&mut self, returned: Returned<S>, actual: &S, normal: bool) {
        #[cfg(test)]
        if tests::at(3) {
            return;
        }
        self.valid &= normal
            && self.phase == Phase::Released
            && Rc::ptr_eq(&returned.identity, &self.identity)
            && returned.index == self.next
            && returned.actual == *actual
            && self.live.as_ref() == Some(actual);
        if self.valid {
            let Some(next) = self.next.checked_add(1) else {
                self.valid = false;
                return;
            };
            self.next = next;
            self.phase = Phase::Live;
        }
    }

    pub(crate) fn removing(
        &mut self,
        binding: B,
        statement: StatementId,
        before: &S,
    ) -> Option<EndTicket> {
        self.valid &= self.phase == Phase::Live
            && self.next == self.plan.consumers.len()
            && binding == self.plan.binding
            && self.live.as_ref() == Some(before)
            && self
                .plan
                .consumers
                .last()
                .is_some_and(|(s, _, _)| *s == statement);
        if !self.valid {
            return None;
        }
        self.phase = Phase::Removing;
        Some(EndTicket {
            identity: self.identity.clone(),
        })
    }

    pub(crate) fn removed(&mut self, ticket: EndTicket, after: &S, exact: bool) {
        #[cfg(test)]
        let ticket = if tests::at(4) {
            EndTicket {
                identity: Rc::new(()),
            }
        } else {
            ticket
        };
        self.valid &= exact
            && self.phase == Phase::Removing
            && Rc::ptr_eq(&ticket.identity, &self.identity)
            && *after == self.entry;
        if self.valid {
            self.phase = Phase::Ended;
        }
    }

    pub(crate) fn close(
        &mut self,
        scope: Option<ScopeTicket>,
        normal: Option<&S>,
        only_normal: bool,
        exact_facts: bool,
    ) -> bool {
        #[cfg(test)]
        let scope = if tests::at(5) { None } else { scope };
        self.valid &= self.phase == Phase::Ended
            && only_normal
            && exact_facts
            && normal == Some(&self.entry)
            && scope.is_some_and(|ticket| {
                Rc::ptr_eq(&ticket.identity, &self.identity) && ticket.body == self.plan.body
            });
        if self.valid {
            self.phase = Phase::Closed;
        }
        self.valid
    }

    pub(crate) fn finish(
        self,
        normal: Option<&S>,
        only_normal: bool,
        exact_facts: bool,
    ) -> Option<Plan<B>> {
        #[cfg(test)]
        let normal = if tests::at(6) { None } else { normal };
        (self.valid
            && self.phase == Phase::Closed
            && only_normal
            && exact_facts
            && normal == Some(&self.entry))
        .then_some(self.plan)
    }
}
