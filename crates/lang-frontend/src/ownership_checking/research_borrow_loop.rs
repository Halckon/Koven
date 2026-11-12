//! Research of nonempty existing loans at actual pre-merge loop edges, not fresh instances.
use super::BorrowFrameChecker;
use crate::{
    ast::{ExpressionId, StatementId},
    parser::{Expression, ParsedFile, Statement},
    source::Span,
};
use std::{cell::RefCell, rc::Rc};
mod consumer;
pub use consumer::{BorrowLoopBinding, BorrowLoopConsumerReturn, BorrowLoopExpression};
pub(crate) use consumer::{
    Prepared, Released, active as consumer_active, complete as complete_consumer,
    prepare as prepare_consumer, release as release_consumer,
};

/// Actual outgoing edge class, never inferred from the merged result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorrowLoopEdgeKind {
    /// Ordinary body fallthrough that repeats the loop.
    NormalBackedge,
    /// Explicit tail continue that repeats the loop.
    ContinueBackedge,
    /// Explicit tail break; not a repeating edge.
    BreakExit,
}
/// Limited existing-resource transport observation. It is not a loop lifetime capability.
#[derive(Clone, Debug)]
pub struct BorrowLoopEdgeObservation {
    checker: BorrowFrameChecker,
    kind: BorrowLoopEdgeKind,
    span: Span,
    carried: usize,
    preserved: bool,
    strict: bool,
    loop_identity: Rc<()>,
    consumers: Vec<BorrowLoopConsumerReturn>,
}
impl BorrowLoopEdgeObservation {
    /// Actual checker producing this edge.
    pub fn checker(&self) -> BorrowFrameChecker {
        self.checker
    }
    /// Actual unmerged edge kind.
    pub fn kind(&self) -> BorrowLoopEdgeKind {
        self.kind
    }
    /// Source span of the explicitly identified while statement.
    pub fn loop_span(&self) -> Span {
        self.span
    }
    /// Nonzero number of real incoming result bindings.
    pub fn carried_binding_count(&self) -> usize {
        self.carried
    }
    /// Whether normalized condition-after/body-entry and outgoing resource states agree.
    pub fn preserved(&self) -> bool {
        self.preserved
    }
    /// Certify only repeating edges; a break remains an exit even if resources agree.
    pub fn validate(&self) -> bool {
        self.strict
            && self.preserved
            && self.carried > 0
            && self.kind != BorrowLoopEdgeKind::BreakExit
    }
    /// Ordered actual normal consumer-return events captured before this edge.
    pub fn consumer_returns(&self) -> &[BorrowLoopConsumerReturn] {
        &self.consumers
    }
    /// Whether a return event belongs to this actual loop invocation.
    pub fn owns_consumer_return(&self, event: &BorrowLoopConsumerReturn) -> bool {
        Rc::ptr_eq(&self.loop_identity, &event.identity)
    }
    /// Compare the identity of the actual loop analysis invocation, not just its span.
    pub fn same_loop_as(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.loop_identity, &other.loop_identity)
    }
}
struct Capture {
    identity: Rc<()>,
    observations: Vec<BorrowLoopEdgeObservation>,
    active: Option<Rc<RefCell<consumer::Protocol>>>,
}
thread_local! { static CAPTURE:RefCell<Option<Capture>>=const{RefCell::new(None)}; }
/// Observe only real core-checker loop edge states during this call.
pub fn capture_borrow_loop_edges<T>(
    run: impl FnOnce() -> T,
) -> (T, Vec<BorrowLoopEdgeObservation>) {
    struct Restore(Option<Capture>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CAPTURE.with(|c| *c.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(CAPTURE.with(|c| {
        c.replace(Some(Capture {
            identity: Rc::new(()),
            observations: Vec::new(),
            active: None,
        }))
    }));
    let result = run();
    let observations =
        CAPTURE.with(|c| std::mem::take(&mut c.borrow_mut().as_mut().unwrap().observations));
    (result, observations)
}
pub(crate) fn is_capturing() -> bool {
    CAPTURE.with(|c| c.borrow().is_some())
}
pub(crate) struct Shape {
    span: Span,
    expected: BorrowLoopEdgeKind,
    calls: Vec<ExpressionId>,
}
pub(crate) fn shape(
    parsed: &ParsedFile,
    statement: StatementId,
    body: StatementId,
    condition_ok: impl Fn(ExpressionId) -> bool,
    consumer_ok: impl Fn(ExpressionId) -> bool,
) -> Option<Shape> {
    let node = parsed.ast().statements().get(statement).ok()?;
    let Statement::While {
        condition,
        body: actual_body,
        ..
    } = node.payload()
    else {
        return None;
    };
    if *actual_body != body
        || !condition_ok(*condition)
        || !matches!(
            parsed.ast().expressions().get(*condition).ok()?.payload(),
            Expression::Name
        )
    {
        return None;
    }
    let (Statement::Block { elements } | Statement::ControlBody { elements }) =
        parsed.ast().statements().get(body).ok()?.payload()
    else {
        return None;
    };
    let (tail, prefix) = elements.split_last()?;
    let expression = match parsed.ast().statements().get(*tail).ok()?.payload() {
        Statement::Expression { expression } => *expression,
        _ => return None,
    };
    let (expected, consumers) = match parsed.ast().expressions().get(expression).ok()?.payload() {
        Expression::Continue { .. } => (BorrowLoopEdgeKind::ContinueBackedge, prefix),
        Expression::Break { .. } => (BorrowLoopEdgeKind::BreakExit, prefix),
        _ => (BorrowLoopEdgeKind::NormalBackedge, elements.as_slice()),
    };
    if consumers.is_empty() {
        return None;
    }
    let mut calls = Vec::new();
    for statement in consumers {
        let Statement::Expression { expression } =
            parsed.ast().statements().get(*statement).ok()?.payload()
        else {
            return None;
        };
        let Expression::Call {
            callee, arguments, ..
        } = parsed.ast().expressions().get(*expression).ok()?.payload()
        else {
            return None;
        };
        calls.push(*expression);
        if arguments.is_empty()
            || !matches!(
                parsed.ast().expressions().get(*callee).ok()?.payload(),
                Expression::Name
            )
            || arguments.iter().any(|a| {
                !parsed
                    .ast()
                    .expressions()
                    .get(a.value)
                    .is_ok_and(|n| matches!(n.payload(), Expression::Name))
            })
            || !consumer_ok(*expression)
        {
            return None;
        }
    }
    Some(Shape {
        span: node.span(),
        expected,
        calls,
    })
}
/// One non-clone token per actual condition-after/body invocation.
pub(crate) struct Ticket<S> {
    entry: S,
    shape: Shape,
    checker: BorrowFrameChecker,
    carried: usize,
    checkpoint: (usize, usize, usize),
    session: Rc<()>,
    identity: Rc<()>,
    protocol: Rc<RefCell<consumer::Protocol>>,
}
impl<S> Drop for Ticket<S> {
    fn drop(&mut self) {
        CAPTURE.with(|c| {
            if let Some(c) = c.borrow_mut().as_mut()
                && c.active
                    .as_ref()
                    .is_some_and(|active| Rc::ptr_eq(active, &self.protocol))
            {
                c.active = None;
            }
        });
    }
}
pub(crate) fn begin<S: Clone + 'static>(
    entry: S,
    shape: Shape,
    checker: BorrowFrameChecker,
    carried: usize,
    checkpoint: (usize, usize, usize),
    source_unit: Option<crate::name_resolution::SourceUnitId>,
) -> Option<Ticket<S>> {
    if carried == 0 {
        return None;
    }
    CAPTURE.with(|c| {
        let mut c = c.borrow_mut();
        let c = c.as_mut()?;
        if let Some(active) = &c.active {
            active.borrow_mut().valid = false;
            return None;
        }
        let identity = Rc::new(());
        let protocol = Rc::new(RefCell::new(consumer::Protocol::new(
            identity.clone(),
            checker,
            shape.calls.clone(),
            entry.clone(),
            shape.span.source_id(),
            source_unit,
        )));
        c.active = Some(protocol.clone());
        Some(Ticket {
            entry,
            shape,
            checker,
            carried,
            checkpoint,
            session: c.identity.clone(),
            identity,
            protocol,
        })
    })
}

pub(crate) fn finish<S: PartialEq>(
    ticket: Option<Ticket<S>>,
    edges: [Option<S>; 3],
    checkpoint: (usize, usize, usize),
) {
    let Some(t) = ticket else { return };
    CAPTURE.with(|c| {
        let mut c = c.borrow_mut();
        let Some(c) = c.as_mut() else { return };
        if !Rc::ptr_eq(&c.identity, &t.session) {
            return;
        }
        let present = edges.iter().filter(|e| e.is_some()).count();
        for (kind, edge) in [
            BorrowLoopEdgeKind::NormalBackedge,
            BorrowLoopEdgeKind::ContinueBackedge,
            BorrowLoopEdgeKind::BreakExit,
        ]
        .into_iter()
        .zip(edges)
        {
            if let Some(edge) = edge {
                c.observations.push(BorrowLoopEdgeObservation {
                    checker: t.checker,
                    kind,
                    span: t.shape.span,
                    carried: t.carried,
                    preserved: edge == t.entry,
                    strict: present == 1
                        && kind == t.shape.expected
                        && checkpoint == t.checkpoint
                        && consumer::complete_tape(&t.protocol.borrow()),
                    loop_identity: t.identity.clone(),
                    consumers: t.protocol.borrow().returns.clone(),
                });
            }
        }
    });
}
