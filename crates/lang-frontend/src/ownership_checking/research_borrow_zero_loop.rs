//! Ordinary-frame closure handed once to a real empty-while header. Not fresh-loop borrowing.
#[cfg(test)]
mod tests;
use super::{BorrowFrameChecker, BorrowFrameObservation, research_borrow_frame as frames};
use crate::{
    ast::StatementId,
    parser::{Expression, ParsedFile, Statement},
    source::Span,
};
use std::{any::Any, cell::RefCell, rc::Rc};

/// Independent research observation of an empty while's zero-loan header and normal edge.
#[derive(Clone, Debug)]
pub struct BorrowZeroLoopObservation {
    checker: BorrowFrameChecker,
    span: Span,
    frame: BorrowFrameObservation,
    identity: Rc<()>,
    header_preserved: bool,
    backedge_preserved: bool,
    clean: bool,
}
impl BorrowZeroLoopObservation {
    /// Checker producing the actual states.
    pub fn checker(&self) -> BorrowFrameChecker {
        self.checker
    }
    /// The actual while statement.
    pub fn loop_span(&self) -> Span {
        self.span
    }
    /// Nonempty ordinary-frame closure consumed once at loop entry, not once per iteration.
    pub fn preceding_frame(&self) -> &BorrowFrameObservation {
        &self.frame
    }
    /// Actual condition-before and condition-after core obligations agree.
    pub fn header_preserved(&self) -> bool {
        self.header_preserved
    }
    /// Actual normal pre-merge backedge agrees with the zero-resource header.
    pub fn backedge_preserved(&self) -> bool {
        self.backedge_preserved
    }
    /// Research only: does not authorize loop-local acquisition or a Complete capability.
    pub fn validate(&self) -> bool {
        self.frame.complete_closure()
            && self.header_preserved
            && self.backedge_preserved
            && self.clean
    }
    /// Actual invocation identity, not just equal source spans.
    pub fn same_loop_as(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.identity, &other.identity)
    }
}
struct Ready {
    parent: frames::FrameIdentity,
    next: StatementId,
    frame: BorrowFrameObservation,
    exit: Box<dyn Any>,
}
struct Capture {
    identity: Rc<()>,
    ready: Option<Ready>,
    header: Option<Box<dyn Any>>,
    active: Option<Rc<()>>,
    observations: Vec<BorrowZeroLoopObservation>,
}
thread_local! { static CAPTURE: RefCell<Option<Capture>>=const{RefCell::new(None)}; }
/// Capture actual ordinary-frame-to-empty-while handoffs without changing ownership products.
pub fn capture_borrow_zero_loops<T>(
    run: impl FnOnce() -> T,
) -> (T, Vec<BorrowZeroLoopObservation>) {
    struct Restore(Option<Capture>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CAPTURE.with(|c| *c.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(CAPTURE.with(|c| {
        c.replace(Some(Capture {
            identity: Rc::new(()),
            ready: None,
            header: None,
            active: None,
            observations: Vec::new(),
        }))
    }));
    let (result, _frames) = frames::capture_borrow_frames(run);
    let observations =
        CAPTURE.with(|c| std::mem::take(&mut c.borrow_mut().as_mut().unwrap().observations));
    (result, observations)
}
pub(crate) fn active() -> bool {
    CAPTURE.with(|c| c.borrow().is_some())
}
pub(crate) fn closed<S: Clone + 'static>(
    frame: &BorrowFrameObservation,
    parent: frames::FrameIdentity,
    next: StatementId,
    exit: &S,
) {
    CAPTURE.with(|c| {
        let mut c = c.borrow_mut();
        let Some(c) = c.as_mut() else {
            return;
        };
        c.ready = frame.complete_closure().then(|| Ready {
            parent,
            next,
            frame: frame.clone(),
            exit: Box::new(exit.clone()),
        });
    });
}
pub(crate) fn shape(
    parsed: &ParsedFile,
    id: StatementId,
    body: StatementId,
    condition_ok: impl Fn(crate::ast::ExpressionId) -> bool,
) -> Option<Span> {
    let node = parsed.ast().statements().get(id).ok()?;
    let Statement::While {
        condition,
        body: actual,
        ..
    } = node.payload()
    else {
        return None;
    };
    if *actual != body
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
    elements.is_empty().then_some(node.span())
}
struct Header<S> {
    session: Rc<()>,
    identity: Rc<()>,
    parent: frames::FrameIdentity,
    statement: StatementId,
    span: Span,
    entry: S,
    checkpoint: (usize, usize, usize),
    ready: Ready,
}
/// A non-clone lifetime guard for a single actual While evaluation.
pub(crate) struct Guard {
    session: Rc<()>,
    identity: Rc<()>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        CAPTURE.with(|c| {
            if let Some(c) = c.borrow_mut().as_mut()
                && Rc::ptr_eq(&c.identity, &self.session)
                && c.active
                    .as_ref()
                    .is_some_and(|id| Rc::ptr_eq(id, &self.identity))
            {
                c.header = None;
                c.active = None;
            }
        });
    }
}
pub(crate) fn header<S: Clone + PartialEq + 'static>(
    statement: StatementId,
    span: Span,
    state: &S,
    zero: bool,
    checkpoint: (usize, usize, usize),
) -> Option<Guard> {
    #[cfg(test)]
    if tests::skip_header() {
        return None;
    }
    let parent = frames::current_identity()?;
    CAPTURE.with(|c| {
        let mut c = c.borrow_mut();
        let c = c.as_mut()?;
        let ready = c.ready.take()?;
        if c.active.is_some()
            || !zero
            || !parent.source_matches(span)
            || !ready.parent.same(&parent)
            || ready.next != statement
            || !ready.frame.complete_closure()
            || ready.exit.downcast_ref::<S>() != Some(state)
        {
            return None;
        }
        let identity = Rc::new(());
        let guard = Guard {
            session: c.identity.clone(),
            identity: identity.clone(),
        };
        c.active = Some(identity.clone());
        let header = Header {
            session: c.identity.clone(),
            identity,
            parent,
            statement,
            span,
            entry: state.clone(),
            checkpoint,
            ready,
        };
        #[cfg(test)]
        let header = {
            let mut header = header;
            tests::before_header(&mut header);
            header
        };
        c.header = Some(Box::new(header));
        Some(guard)
    })
}
pub(crate) struct Ticket<S> {
    header: Header<S>,
    condition_preserved: bool,
}
pub(crate) fn condition<S: PartialEq + 'static>(
    statement: Option<StatementId>,
    state: &S,
    zero: bool,
    checkpoint: (usize, usize, usize),
) -> Option<Ticket<S>> {
    #[cfg(test)]
    if tests::skip_condition() {
        return None;
    }
    let parent = frames::current_identity()?;
    CAPTURE.with(|c| {
        let mut c = c.borrow_mut();
        let c = c.as_mut()?;
        let header = *c.header.take()?.downcast::<Header<S>>().ok()?;
        #[cfg(test)]
        let header = {
            let mut header = header;
            tests::before_condition(&mut header);
            header
        };
        if !Rc::ptr_eq(&header.session, &c.identity)
            || !c
                .active
                .as_ref()
                .is_some_and(|id| Rc::ptr_eq(id, &header.identity))
            || !parent.same(&header.parent)
            || statement != Some(header.statement)
        {
            return None;
        }
        let condition_preserved = zero && &header.entry == state && header.checkpoint == checkpoint;
        Some(Ticket {
            header,
            condition_preserved,
        })
    })
}
pub(crate) fn finish<S: PartialEq>(
    ticket: Option<Ticket<S>>,
    normal: Option<&S>,
    only_normal: bool,
    zero: bool,
    checkpoint: (usize, usize, usize),
) {
    let Some(ticket) = ticket else {
        return;
    };
    #[cfg(test)]
    let (normal, only_normal) = {
        let mut normal = normal;
        let mut only_normal = only_normal;
        tests::finish_inputs(&mut normal, &mut only_normal);
        (normal, only_normal)
    };
    let parent = frames::current_identity();
    CAPTURE.with(|c| {
        let mut c = c.borrow_mut();
        let Some(c) = c.as_mut() else {
            return;
        };
        if !parent
            .as_ref()
            .is_some_and(|parent| parent.same(&ticket.header.parent))
            || !Rc::ptr_eq(&ticket.header.session, &c.identity)
            || !c
                .active
                .as_ref()
                .is_some_and(|id| Rc::ptr_eq(id, &ticket.header.identity))
        {
            return;
        }
        c.active = None;
        c.observations.push(BorrowZeroLoopObservation {
            checker: ticket.header.parent.checker,
            span: ticket.header.span,
            frame: ticket.header.ready.frame,
            identity: ticket.header.identity,
            header_preserved: ticket.condition_preserved,
            backedge_preserved: zero && normal == Some(&ticket.header.entry),
            clean: only_normal && normal.is_some() && checkpoint == ticket.header.checkpoint,
        });
    });
}
