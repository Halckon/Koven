//! Research-only normal body lifecycle with actual temporary call loans. Not fresh binding support.
use super::{
    BorrowLoopBinding as Binding, BorrowLoopExpression as Expression,
    research_borrow_frame as frames,
};
use crate::{ast::StatementId, source::Span};
use std::{any::Any, cell::RefCell, rc::Rc};
pub(crate) type Call = (Expression, Vec<(Expression, Binding)>);
#[derive(Clone, Debug)]
/// One actual temporary-loan consumer return in a body frame.
pub struct BorrowBodyConsumerReturn {
    identity: Rc<()>,
    call: Expression,
    arguments: Vec<(Expression, Binding)>,
    acquired: usize,
    released: usize,
}
impl BorrowBodyConsumerReturn {
    /// Original source-qualified call.
    pub fn call(&self) -> Expression {
        self.call
    }
    /// Original ordered operands and their external parameter identities.
    pub fn arguments(&self) -> &[(Expression, Binding)] {
        &self.arguments
    }
    /// Actual call-loan insertions certified at the operand boundary.
    pub fn acquired_count(&self) -> usize {
        self.acquired
    }
    /// Actual loans removed at normal call return.
    pub fn released_count(&self) -> usize {
        self.released
    }
}
#[derive(Clone, Debug)]
/// Condition-after body-entry through normal pre-merge research evidence.
pub struct BorrowConsumerBodyObservation {
    identity: Rc<()>,
    span: Span,
    body_span: Span,
    returns: Vec<BorrowBodyConsumerReturn>,
    valid: bool,
}
impl BorrowConsumerBodyObservation {
    /// Research normal-edge certificate, not a runtime iteration or language capability.
    pub fn validate(&self) -> bool {
        self.valid
    }
    /// Actual normal returns, in body statement order.
    pub fn consumer_returns(&self) -> &[BorrowBodyConsumerReturn] {
        &self.returns
    }
    /// Identity membership, not merely equal spans or AST indices.
    pub fn owns_consumer_return(&self, event: &BorrowBodyConsumerReturn) -> bool {
        self.returns
            .iter()
            .any(|e| Rc::ptr_eq(&e.identity, &event.identity))
    }
    /// Original while statement span.
    pub fn loop_span(&self) -> Span {
        self.span
    }
    /// Actual claimed body scope span.
    pub fn body_span(&self) -> Span {
        self.body_span
    }
    /// Same actual research invocation.
    pub fn same_loop_as(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.identity, &other.identity)
    }
}
struct Capture {
    session: Rc<()>,
    active: Option<Box<dyn Any>>,
    active_identity: Option<Rc<()>>,
    observations: Vec<BorrowConsumerBodyObservation>,
}
thread_local! { static CAPTURE:RefCell<Option<Capture>>=const{RefCell::new(None)}; }
/// Capture independent parameter-consumer bodies without changing ownership products.
pub fn capture_borrow_consumer_bodies<T>(
    run: impl FnOnce() -> T,
) -> (T, Vec<BorrowConsumerBodyObservation>) {
    struct Restore(Option<Capture>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CAPTURE.with(|c| *c.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(CAPTURE.with(|c| {
        c.replace(Some(Capture {
            session: Rc::new(()),
            active: None,
            active_identity: None,
            observations: Vec::new(),
        }))
    }));
    let (result, _) = frames::capture_borrow_frames(run);
    let observations =
        CAPTURE.with(|c| std::mem::take(&mut c.borrow_mut().as_mut().unwrap().observations));
    (result, observations)
}
pub(crate) fn capturing() -> bool {
    CAPTURE.with(|c| c.borrow().is_some())
}
struct Active<S> {
    session: Rc<()>,
    identity: Rc<()>,
    parent: frames::FrameIdentity,
    scope: StatementId,
    span: Span,
    body_span: Option<Span>,
    frame: Option<frames::FrameIdentity>,
    entry: S,
    current: S,
    calls: Vec<Call>,
    next: usize,
    operands: usize,
    pending: bool,
    valid: bool,
    closed: Option<S>,
    returns: Vec<BorrowBodyConsumerReturn>,
    checkpoint: (usize, usize, usize, usize),
}
pub(crate) struct LoopTicket<S> {
    session: Rc<()>,
    identity: Rc<()>,
    _state: std::marker::PhantomData<S>,
}
impl<S> Drop for LoopTicket<S> {
    fn drop(&mut self) {
        CAPTURE.with(|c| {
            if let Some(c) = c.borrow_mut().as_mut()
                && Rc::ptr_eq(&c.session, &self.session)
                && c.active_identity
                    .as_ref()
                    .is_some_and(|id| Rc::ptr_eq(id, &self.identity))
            {
                c.active = None;
                c.active_identity = None;
            }
        });
    }
}
pub(crate) struct BodyTicket {
    session: Rc<()>,
    identity: Rc<()>,
}
pub(crate) struct Prepared<S> {
    session: Rc<()>,
    identity: Rc<()>,
    call: Expression,
    expected: S,
}
pub(crate) struct Released<S>(Prepared<S>);
pub(crate) fn begin<S: Clone + PartialEq + 'static>(
    scope: StatementId,
    span: Span,
    state: &S,
    calls: Vec<Call>,
    checkpoint: (usize, usize, usize, usize),
) -> Option<LoopTicket<S>> {
    let parent = frames::current_identity()?;
    if !valid_calls(&calls, &parent, span) {
        return None;
    }
    CAPTURE.with(|c| {
        let mut c = c.borrow_mut();
        let c = c.as_mut()?;
        if c.active.is_some() {
            return None;
        }
        #[cfg(test)]
        tests::begun();
        let identity = Rc::new(());
        let session = c.session.clone();
        c.active_identity = Some(identity.clone());
        c.active = Some(Box::new(Active {
            session: session.clone(),
            identity: identity.clone(),
            parent,
            scope,
            span,
            body_span: None,
            frame: None,
            entry: state.clone(),
            current: state.clone(),
            calls,
            next: 0,
            operands: 0,
            pending: false,
            valid: true,
            closed: None,
            returns: Vec::new(),
            checkpoint,
        }));
        Some(LoopTicket {
            session,
            identity,
            _state: std::marker::PhantomData,
        })
    })
}
fn with<S: 'static, T>(f: impl FnOnce(&mut Active<S>) -> T) -> Option<T> {
    CAPTURE.with(|c| {
        let mut c = c.borrow_mut();
        let c = c.as_mut()?;
        let a = c.active.as_mut()?.downcast_mut::<Active<S>>()?;
        if !Rc::ptr_eq(&a.session, &c.session) {
            return None;
        }
        Some(f(a))
    })
}
pub(crate) fn enter<S: PartialEq + 'static>(scope: StatementId, state: &S) -> Option<BodyTicket> {
    let frame = frames::current_activity()?;
    with::<S, _>(|a| {
        if a.scope != scope
            || a.frame.is_some()
            || !frame.valid
            || frame.acquisitions != 0
            || frame.ends != 0
            || frame.scope != scope
            || !frame.parent.as_ref().is_some_and(|p| p.same(&a.parent))
            || &a.entry != state
        {
            return None;
        }
        a.frame = Some(frame.identity);
        a.body_span = Some(frame.span);
        Some(BodyTicket {
            session: a.session.clone(),
            identity: a.identity.clone(),
        })
    })
    .flatten()
}
pub(crate) fn expects<S: 'static>(call: Expression, arg: Expression, binding: Binding) -> bool {
    with::<S, _>(|a| {
        #[cfg(test)]
        tests::before_expects(a);
        a.valid
            && a.frame.is_some()
            && !a.pending
            && a.closed.is_none()
            && a.calls.get(a.next).is_some_and(|(c, args)| {
                *c == call && args.get(a.operands) == Some(&(arg, binding))
            })
    })
    .unwrap_or(false)
}
pub(crate) fn acquired<S: Clone + PartialEq + 'static>(
    call: Expression,
    arg: Expression,
    binding: Binding,
    before: &S,
    after: &S,
    exact: bool,
    fact_cursor: usize,
) {
    let frame = frames::current_identity();
    with::<S, _>(|a| {
        let ok = exact
            && a.returns
                .iter()
                .try_fold(a.checkpoint.3, |n, r| n.checked_add(r.acquired))
                .and_then(|n| n.checked_add(a.operands))
                == Some(fact_cursor)
            && a.valid
            && !a.pending
            && a.closed.is_none()
            && a.frame
                .as_ref()
                .is_some_and(|f| frame.as_ref().is_some_and(|actual| f.same(actual)))
            && &a.current == before
            && a.calls.get(a.next).is_some_and(|(c, args)| {
                *c == call && args.get(a.operands) == Some(&(arg, binding))
            });
        if !ok {
            a.valid = false;
            return;
        }
        a.current = after.clone();
        a.operands += 1;
    });
}
pub(crate) fn prepare<S: Clone + PartialEq + 'static>(
    call: Expression,
    before: &S,
    expected: &S,
) -> Option<Prepared<S>> {
    with::<S, _>(|a| {
        #[cfg(test)]
        tests::before_prepare(a);
        if !current_frame(a)
            || !a.valid
            || a.pending
            || a.closed.is_some()
            || &a.current != before
            || &a.entry != expected
            || !a
                .calls
                .get(a.next)
                .is_some_and(|(c, args)| *c == call && a.operands == args.len())
        {
            a.valid = false;
            return None;
        }
        a.pending = true;
        Some(Prepared {
            session: a.session.clone(),
            identity: a.identity.clone(),
            call,
            expected: expected.clone(),
        })
    })
    .flatten()
}
pub(crate) fn release<S: PartialEq + 'static>(
    token: Option<Prepared<S>>,
    actual: Option<&S>,
) -> Option<Released<S>> {
    #[cfg(test)]
    if tests::skip_release() {
        return None;
    }
    let token = token?;
    let ok = with::<S, _>(|a| {
        current_frame(a)
            && a.valid
            && a.pending
            && Rc::ptr_eq(&a.session, &token.session)
            && Rc::ptr_eq(&a.identity, &token.identity)
            && actual == Some(&token.expected)
    })
    .unwrap_or(false);
    ok.then_some(Released(token))
}
pub(crate) fn complete<S: Clone + PartialEq + 'static>(
    token: Option<Released<S>>,
    actual: Option<&S>,
    normal: bool,
) {
    let Some(Released(t)) = token else {
        return;
    };
    #[cfg(test)]
    let t = {
        let mut t = t;
        if !tests::before_complete(&mut t) {
            return;
        }
        t
    };
    with::<S, _>(|a| {
        if !current_frame(a)
            || !a.valid
            || !a.pending
            || !normal
            || actual != Some(&t.expected)
            || !Rc::ptr_eq(&a.session, &t.session)
            || !Rc::ptr_eq(&a.identity, &t.identity)
            || !a.calls.get(a.next).is_some_and(|(c, _)| *c == t.call)
        {
            a.valid = false;
            return;
        }
        let (_, args) = &a.calls[a.next];
        a.returns.push(BorrowBodyConsumerReturn {
            identity: Rc::new(()),
            call: t.call,
            arguments: args.clone(),
            acquired: a.operands,
            released: a.operands,
        });
        a.current = t.expected;
        a.next += 1;
        a.operands = 0;
        a.pending = false;
    });
}
pub(crate) fn close<S: Clone + PartialEq + 'static>(
    ticket: Option<BodyTicket>,
    normal: Option<&S>,
    only_normal: bool,
    checkpoint: (usize, usize, usize, usize),
) {
    let Some(t) = ticket else {
        return;
    };
    let Some(frame) = frames::current_activity() else {
        return;
    };
    with::<S, _>(|a| {
        #[cfg(test)]
        tests::before_close(a);
        if !current_frame(a)
            || !a.valid
            || a.pending
            || a.operands != 0
            || a.closed.is_some()
            || a.next != a.calls.len()
            || !only_normal
            || normal != Some(&a.entry)
            || !checkpoint_matches(a, checkpoint)
            || !frame.valid
            || frame.acquisitions != 0
            || frame.ends != 0
            || !a.frame.as_ref().is_some_and(|f| f.same(&frame.identity))
            || !Rc::ptr_eq(&a.session, &t.session)
            || !Rc::ptr_eq(&a.identity, &t.identity)
        {
            a.valid = false;
            return;
        }
        a.closed = normal.cloned();
    });
}
pub(crate) fn finish<S: PartialEq + 'static>(
    ticket: Option<LoopTicket<S>>,
    normal: Option<&S>,
    only_normal: bool,
    checkpoint: (usize, usize, usize, usize),
) {
    let Some(t) = ticket else {
        return;
    };
    let parent = frames::current_identity();
    CAPTURE.with(|c| {
        let mut c = c.borrow_mut();
        let Some(c) = c.as_mut() else {
            return;
        };
        if !Rc::ptr_eq(&c.session, &t.session)
            || !c
                .active_identity
                .as_ref()
                .is_some_and(|id| Rc::ptr_eq(id, &t.identity))
        {
            return;
        }
        c.active_identity = None;
        let Some(active) = c.active.take() else {
            return;
        };
        let Ok(a) = active.downcast::<Active<S>>() else {
            return;
        };
        if !Rc::ptr_eq(&a.identity, &t.identity)
            || !parent.as_ref().is_some_and(|p| p.same(&a.parent))
        {
            return;
        }
        let Some(body_span) = a.body_span else {
            return;
        };
        let valid = a.valid
            && only_normal
            && a.closed.is_some()
            && normal == a.closed.as_ref()
            && checkpoint_matches(&a, checkpoint)
            && !a.pending
            && a.next == a.calls.len()
            && valid_calls(&a.calls, &a.parent, a.span)
            && a.returns.len() == a.calls.len()
            && a.returns
                .iter()
                .zip(&a.calls)
                .enumerate()
                .all(|(i, (e, (call, args)))| {
                    e.call == *call
                        && e.arguments == *args
                        && e.acquired == args.len()
                        && e.released == args.len()
                        && !a.returns[..i]
                            .iter()
                            .any(|p| Rc::ptr_eq(&p.identity, &e.identity))
                });
        c.observations.push(BorrowConsumerBodyObservation {
            identity: a.identity,
            span: a.span,
            body_span,
            returns: a.returns,
            valid,
        });
    });
}

fn current_frame<S>(a: &Active<S>) -> bool {
    let frame = frames::current_identity();
    a.frame
        .as_ref()
        .is_some_and(|f| frame.as_ref().is_some_and(|actual| f.same(actual)))
}
fn valid_calls(calls: &[Call], parent: &frames::FrameIdentity, span: Span) -> bool {
    let expression = |id: Expression| match id {
        Expression::Single { source, .. } => {
            parent.source_unit().is_none() && source == span.source_id()
        }
        Expression::Unit(id) => Some(id.source_unit()) == parent.source_unit(),
    };
    let binding = |id: Binding| match id {
        Binding::Single { source, .. } => {
            parent.source_unit().is_none() && source == span.source_id()
        }
        Binding::Unit(id) => Some(id.source_unit()) == parent.source_unit(),
    };
    !calls.is_empty()
        && parent.source_matches(span)
        && calls.iter().enumerate().all(|(i, (call, args))| {
            expression(*call)
                && !args.is_empty()
                && !calls[..i].iter().any(|(c, _)| c == call)
                && args.iter().enumerate().all(|(j, (arg, b))| {
                    expression(*arg)
                        && binding(*b)
                        && arg != call
                        && !args[..j].iter().any(|(a, _)| a == arg)
                })
        })
}

fn checkpoint_matches<S>(a: &Active<S>, checkpoint: (usize, usize, usize, usize)) -> bool {
    (checkpoint.0, checkpoint.1, checkpoint.2) == (a.checkpoint.0, a.checkpoint.1, a.checkpoint.2)
        && a.calls
            .iter()
            .try_fold(a.checkpoint.3, |n, (_, args)| n.checked_add(args.len()))
            == Some(checkpoint.3)
}

#[cfg(test)]
mod tests;
