//! Actual scope-entry and pre-cleanup body-exit tokens; never fresh-binding authorization.
use super::{CAPTURE, Capture, Header};
use crate::{ast::StatementId, ownership_checking::research_borrow_frame as frames, source::Span};
use std::rc::Rc;

/// Evidence of the actual empty body scope, sealed before any scope restoration.
#[derive(Clone, Debug)]
pub struct BorrowZeroBodyFrame {
    pub(super) loop_identity: Rc<()>,
    identity: frames::FrameIdentity,
    span: Span,
    acquisitions: usize,
    ends: usize,
    preserved: bool,
}
impl BorrowZeroBodyFrame {
    /// Actual dispatched scope's source span; internal identity is not span-based.
    pub fn scope_span(&self) -> Span {
        self.span
    }
    /// Actual scope acquisition count at sealing, not an AST-derived count.
    pub fn acquisition_count(&self) -> usize {
        self.acquisitions
    }
    /// Actual scope End-event count at sealing, not a cleanup inference.
    pub fn end_count(&self) -> usize {
        self.ends
    }
    /// Actual entry and normal exit obligations agree before scope cleanup.
    pub fn preserved_before_cleanup(&self) -> bool {
        self.preserved
    }
    pub(super) fn same(&self, other: &Self) -> bool {
        self.identity.same(&other.identity)
    }
}
struct Permit<S> {
    session: Rc<()>,
    loop_identity: Rc<()>,
    parent: frames::FrameIdentity,
    body: StatementId,
    entry: S,
    checkpoint: (usize, usize, usize),
}
pub(crate) struct BodyTicket<S> {
    permit: Permit<S>,
    frame: frames::FrameIdentity,
    span: Span,
}
struct Exit<S> {
    permit: Permit<S>,
    proof: BorrowZeroBodyFrame,
    exit: S,
}
pub(super) fn mint<S: Clone + 'static>(capture: &mut Capture, header: &Header<S>, actual: &S) {
    #[cfg(test)]
    tests::minted();
    if capture.body_permit.is_some() || capture.body_exit.is_some() {
        return;
    }
    capture.body_permit = Some(Box::new(Permit {
        session: header.session.clone(),
        loop_identity: header.identity.clone(),
        parent: header.parent.clone(),
        body: header.body,
        entry: actual.clone(),
        checkpoint: header.checkpoint,
    }));
}
pub(crate) fn enter<S: PartialEq + 'static>(
    scope: StatementId,
    actual: &S,
    zero: bool,
    checkpoint: (usize, usize, usize),
) -> Option<BodyTicket<S>> {
    #[cfg(test)]
    if tests::skip_enter() {
        return None;
    }
    let frame = frames::current_activity()?;
    CAPTURE.with(|c| {
        let mut c = c.borrow_mut();
        let c = c.as_mut()?;
        let permit = *c.body_permit.take()?.downcast::<Permit<S>>().ok()?;
        #[cfg(test)]
        let permit = {
            let mut permit = permit;
            tests::before_enter(&mut permit);
            permit
        };
        if !Rc::ptr_eq(&permit.session, &c.identity)
            || !c
                .active
                .as_ref()
                .is_some_and(|id| Rc::ptr_eq(id, &permit.loop_identity))
            || scope != permit.body
            || frame.scope != permit.body
            || !frame
                .parent
                .as_ref()
                .is_some_and(|parent| parent.same(&permit.parent))
            || !zero
            || !frame.valid
            || frame.acquisitions != 0
            || frame.ends != 0
            || &permit.entry != actual
            || permit.checkpoint != checkpoint
        {
            return None;
        }
        Some(BodyTicket {
            permit,
            frame: frame.identity,
            span: frame.span,
        })
    })
}
pub(crate) fn close<S: Clone + PartialEq + 'static>(
    ticket: Option<BodyTicket<S>>,
    actual: Option<&S>,
    only_normal: bool,
    zero: bool,
    checkpoint: (usize, usize, usize),
) {
    let Some(ticket) = ticket else {
        return;
    };
    #[cfg(test)]
    if tests::skip_close() {
        return;
    }
    #[cfg(test)]
    let ticket = {
        let mut ticket = ticket;
        tests::before_close(&mut ticket);
        ticket
    };
    let Some(frame) = frames::current_activity() else {
        return;
    };
    CAPTURE.with(|c| {
        let mut c = c.borrow_mut();
        let Some(c) = c.as_mut() else {
            return;
        };
        if !Rc::ptr_eq(&ticket.permit.session, &c.identity)
            || !c
                .active
                .as_ref()
                .is_some_and(|id| Rc::ptr_eq(id, &ticket.permit.loop_identity))
            || c.body_exit.is_some()
            || !frame.identity.same(&ticket.frame)
            || frame.scope != ticket.permit.body
            || !frame
                .parent
                .as_ref()
                .is_some_and(|p| p.same(&ticket.permit.parent))
            || !frame.valid
            || frame.acquisitions != 0
            || frame.ends != 0
            || !only_normal
            || !zero
            || actual != Some(&ticket.permit.entry)
            || checkpoint != ticket.permit.checkpoint
        {
            return;
        }
        let Some(actual) = actual else {
            return;
        };
        c.body_exit = Some(Box::new(Exit {
            proof: BorrowZeroBodyFrame {
                loop_identity: ticket.permit.loop_identity.clone(),
                identity: ticket.frame,
                span: ticket.span,
                acquisitions: frame.acquisitions,
                ends: frame.ends,
                preserved: true,
            },
            permit: ticket.permit,
            exit: actual.clone(),
        }));
    });
}
pub(super) fn consume<S: PartialEq + 'static>(
    c: &mut Capture,
    header: &Header<S>,
    normal: Option<&S>,
    only_normal: bool,
    zero: bool,
    checkpoint: (usize, usize, usize),
) -> Option<BorrowZeroBodyFrame> {
    let exit = *c.body_exit.take()?.downcast::<Exit<S>>().ok()?;
    #[cfg(test)]
    let exit = {
        let mut exit = exit;
        tests::before_consume(&mut exit);
        exit
    };
    if c.body_permit.is_some()
        || !Rc::ptr_eq(&exit.permit.session, &header.session)
        || !Rc::ptr_eq(&exit.permit.loop_identity, &header.identity)
        || !exit.permit.parent.same(&header.parent)
        || exit.permit.body != header.body
        || !only_normal
        || !zero
        || normal != Some(&exit.exit)
        || exit.permit.checkpoint != checkpoint
    {
        return None;
    }
    Some(exit.proof)
}

#[cfg(test)]
mod tests;
