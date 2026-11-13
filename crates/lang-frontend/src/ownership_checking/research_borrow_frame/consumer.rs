//! Actual normal-return tokens for ordinary Block frames. No scope-cleanup inference.
#[cfg(test)]
mod tests;
use super::{Binding, BorrowFrameChecker, CAPTURE, Frame};
use crate::ownership_checking::{BorrowLoopBinding, BorrowLoopExpression};
use crate::{
    ast::{ExpressionId, StatementId},
    source::Span,
};
use std::rc::Rc;

/// Source-qualified actual call return belonging to one ordinary frame invocation.
#[derive(Clone, Debug)]
pub struct BorrowFrameConsumerReturn {
    pub(super) identity: Rc<()>,
    call: BorrowLoopExpression,
    arguments: Vec<(BorrowLoopExpression, BorrowLoopBinding)>,
    released: usize,
}
impl BorrowFrameConsumerReturn {
    /// Original call identity, not just its source span.
    pub fn call(&self) -> BorrowLoopExpression {
        self.call
    }
    /// Original operands and actual resolved local binding identities, in operand order.
    pub fn arguments(&self) -> &[(BorrowLoopExpression, BorrowLoopBinding)] {
        &self.arguments
    }
    /// Actual temporary call loans removed; does not count local binding loans.
    pub fn released_loan_count(&self) -> usize {
        self.released
    }
}
type ConsumerPlan = (StatementId, ExpressionId, Vec<(ExpressionId, Binding)>);

pub(super) struct Tape {
    calls: Vec<ConsumerPlan>,
    steps: Vec<Step>,
    next: usize,
    valid: bool,
    pending: bool,
    pub(super) returns: Vec<BorrowFrameConsumerReturn>,
    closed: Vec<Binding>,
}
pub(crate) enum Name {
    Declaration(crate::parser::NameMarker),
    Reference(Span),
}
pub(super) enum Step {
    Acquire(Binding),
    Consume(StatementId, ExpressionId, Vec<(ExpressionId, Binding)>),
}
pub(super) fn plan(
    parsed: &crate::parser::ParsedFile,
    elements: &[StatementId],
    resolve: impl Fn(Name) -> Option<Binding>,
) -> Option<Vec<Step>> {
    use crate::parser::{Expression, Item, Statement};
    elements
        .iter()
        .map(
            |id| match parsed.ast().statements().get(*id).ok()?.payload() {
                Statement::LocalVariable { declaration } => {
                    let Item::Variable { name, .. } =
                        parsed.ast().items().get(*declaration).ok()?.payload()
                    else {
                        return None;
                    };
                    Some(Step::Acquire(resolve(Name::Declaration(*name))?))
                }
                Statement::Expression { expression } => {
                    let Expression::Call { arguments, .. } =
                        parsed.ast().expressions().get(*expression).ok()?.payload()
                    else {
                        return None;
                    };
                    let bindings = arguments
                        .iter()
                        .map(|a| {
                            Some((
                                a.value,
                                resolve(Name::Reference(
                                    parsed.ast().expressions().get(a.value).ok()?.span(),
                                ))?,
                            ))
                        })
                        .collect::<Option<Vec<_>>>()?;
                    Some(Step::Consume(*id, *expression, bindings))
                }
                _ => None,
            },
        )
        .collect()
}
impl Tape {
    pub(super) fn new(steps: Option<Vec<Step>>) -> Self {
        let valid = steps.is_some();
        let steps = steps.unwrap_or_default();
        let calls = steps
            .iter()
            .filter_map(|s| match s {
                Step::Consume(stmt, call, args) => Some((*stmt, *call, args.clone())),
                _ => None,
            })
            .collect();
        Self {
            calls,
            steps,
            next: 0,
            valid,
            pending: false,
            returns: Vec::new(),
            closed: Vec::new(),
        }
    }
}
pub(super) fn acquire(f: &mut Frame, binding: Binding) {
    if !matches!(f.consumers.steps.get(f.consumers.next), Some(Step::Acquire(expected)) if *expected==binding)
        || f.consumers.pending
    {
        f.consumers.valid = false;
    } else {
        f.consumers.next += 1;
    }
}
fn raw(call: BorrowLoopExpression) -> ExpressionId {
    match call {
        BorrowLoopExpression::Single { expression, .. } => expression,
        BorrowLoopExpression::Unit(id) => id.expression(),
    }
}
fn binding(id: BorrowLoopBinding) -> Binding {
    match id {
        BorrowLoopBinding::Single { symbol, .. } => Binding::Single(symbol),
        BorrowLoopBinding::Unit(id) => Binding::Unit(id),
    }
}
fn identities_match(
    f: &Frame,
    call: BorrowLoopExpression,
    args: &[(BorrowLoopExpression, BorrowLoopBinding)],
) -> bool {
    match call {
        BorrowLoopExpression::Single { source, .. } => {
            f.checker == BorrowFrameChecker::Single
                && f.source_unit.is_none()
                && source == f.span.source_id()
                && args.iter().all(|(a, b)| match (a, b) {
                    (
                        BorrowLoopExpression::Single { source: s, .. },
                        BorrowLoopBinding::Single { source: t, .. },
                    ) => *s == source && *t == source,
                    _ => false,
                })
        }
        BorrowLoopExpression::Unit(call) => {
            f.checker == BorrowFrameChecker::Unit
                && f.source_unit == Some(call.source_unit())
                && args.iter().all(|(a, b)| match (a, b) {
                    (BorrowLoopExpression::Unit(a), BorrowLoopBinding::Unit(b)) => {
                        a.source_unit() == call.source_unit()
                            && b.source_unit() == call.source_unit()
                    }
                    _ => false,
                })
        }
    }
}

pub(crate) fn consumer_active() -> bool {
    CAPTURE.with(|c| {
        c.borrow()
            .as_ref()
            .and_then(|c| c.frames.last())
            .is_some_and(|f| f.eligible && !f.acquired.is_empty() && !f.consumers.calls.is_empty())
    })
}
/// Match each actual operand's live binding resources to its real frame acquisition.
pub(crate) fn consumer_live<
    B: Copy + Into<Binding>,
    P: Clone + PartialEq + 'static,
    L: PartialEq + 'static,
>(
    binding: B,
    place: Option<&P>,
    marker: Option<&Span>,
    loans: Vec<L>,
) -> bool {
    CAPTURE.with(|c| {
        let mut c = c.borrow_mut();
        let Some(f) = c.as_mut().and_then(|c| c.frames.last_mut()) else {
            return false;
        };
        let snapshot = (place.cloned(), marker.copied(), loans);
        let valid = f
            .acquired
            .iter()
            .find(|a| a.binding == binding.into())
            .is_some_and(|a| {
                !a.ended
                    && a.snapshot
                        .downcast_ref::<(Option<P>, Option<Span>, Vec<L>)>()
                        == Some(&snapshot)
            });
        f.consumers.valid &= valid;
        valid
    })
}
pub(crate) struct Prepared<S> {
    frame: Rc<()>,
    expected: S,
    event: BorrowFrameConsumerReturn,
}
pub(crate) struct Released<S> {
    prepared: Prepared<S>,
}
pub(crate) fn prepare_consumer<S>(
    call: BorrowLoopExpression,
    arguments: Vec<(BorrowLoopExpression, BorrowLoopBinding)>,
    expected: S,
    released: usize,
) -> Option<Prepared<S>> {
    CAPTURE.with(|c| {
        let mut c = c.borrow_mut();
        let f = c.as_mut()?.frames.last_mut()?;
        if !f.eligible {
            return None;
        }
        #[cfg(test)]
        tests::before_prepare(f);
        let step_matches = match f.consumers.steps.get(f.consumers.next) {
            Some(Step::Consume(_, expected, args)) => {
                *expected == raw(call)
                    && args
                        .iter()
                        .copied()
                        .eq(arguments.iter().map(|(a, b)| (raw(*a), binding(*b))))
            }
            _ => false,
        };
        if !f.consumers.valid
            || f.consumers.pending
            || arguments.is_empty()
            || released != arguments.len()
            || !identities_match(f, call, &arguments)
            || f.consumers
                .calls
                .get(f.consumers.returns.len())
                .map(|(_, c, _)| *c)
                != Some(raw(call))
            || !step_matches
            || arguments.iter().any(|(_, b)| {
                !f.acquired
                    .iter()
                    .any(|a| a.binding == binding(*b) && !a.ended)
            })
        {
            f.consumers.valid = false;
            return None;
        }
        f.consumers.pending = true;
        Some(Prepared {
            frame: f.identity.clone(),
            expected,
            event: BorrowFrameConsumerReturn {
                identity: f.identity.clone(),
                call,
                arguments,
                released,
            },
        })
    })
}
pub(crate) fn release_consumer<S: PartialEq>(
    prepared: Option<Prepared<S>>,
    actual: Option<&S>,
) -> Option<Released<S>> {
    let prepared = prepared?;
    #[cfg(test)]
    if tests::skip_release() {
        return None;
    }
    if actual != Some(&prepared.expected) {
        return None;
    }
    Some(Released { prepared })
}
pub(crate) fn complete_consumer<S: PartialEq>(
    released: Option<Released<S>>,
    actual: Option<&S>,
    clean: bool,
) {
    let Some(Released { prepared }) = released else {
        return;
    };
    #[cfg(test)]
    let prepared = {
        let mut prepared = prepared;
        if !tests::before_complete(&mut prepared) {
            return;
        }
        prepared
    };
    CAPTURE.with(|c| {
        let mut c = c.borrow_mut();
        let Some(f) = c.as_mut().and_then(|c| c.frames.last_mut()) else {
            return;
        };
        if !Rc::ptr_eq(&f.identity, &prepared.frame)
            || !f.consumers.pending
            || !clean
            || actual != Some(&prepared.expected)
        {
            f.consumers.valid = false;
            return;
        }
        f.consumers.pending = false;
        f.consumers.next += 1;
        f.consumers.returns.push(prepared.event);
    });
}
pub(super) fn close_binding(f: &mut Frame, id: Binding, statement: StatementId) {
    let valid =
        !f.consumers.steps.iter().skip(f.consumers.next).any(
            |step| matches!(step, Step::Consume(_,_,args) if args.iter().any(|(_,b)| *b == id)),
        ) && !f.consumers.pending
            && !f.consumers.closed.contains(&id)
            && f.consumers
                .returns
                .iter()
                .enumerate()
                .rev()
                .find(|(_, e)| e.arguments.iter().any(|(_, b)| binding(*b) == id))
                .is_some_and(|(i, _)| {
                    f.consumers
                        .calls
                        .get(i)
                        .is_some_and(|(s, _, _)| *s == statement)
                });
    f.consumers.valid &= valid;
    f.consumers.closed.push(id);
}
pub(super) fn complete_tape(f: &Frame) -> bool {
    f.consumers.valid
        && !f.consumers.pending
        && f.consumers.next == f.consumers.steps.len()
        && !f.consumers.calls.is_empty()
        && f.consumers.closed.len() == f.acquired.len()
        && f.consumers.returns.len() == f.consumers.calls.len()
        && f.consumers
            .returns
            .iter()
            .zip(&f.consumers.calls)
            .all(|(e, (_, call, args))| {
                Rc::ptr_eq(&e.identity, &f.identity)
                    && raw(e.call) == *call
                    && !e.arguments.is_empty()
                    && e.released == e.arguments.len()
                    && args
                        .iter()
                        .copied()
                        .eq(e.arguments.iter().map(|(a, b)| (raw(*a), binding(*b))))
                    && identities_match(f, e.call, &e.arguments)
            })
}
