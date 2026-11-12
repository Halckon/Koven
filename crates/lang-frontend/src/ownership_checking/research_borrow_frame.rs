//! Test-support research of actual ordinary-block acquisition/removal, not loop eligibility.
use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::{SourceUnitId, SymbolId, UnitSymbolId},
    parser::{Expression, Item, ParsedFile, Statement, VariableKind},
    source::Span,
};
mod consumer;
pub use consumer::BorrowFrameConsumerReturn;
pub(crate) use consumer::{
    Name, Prepared, Released, complete_consumer, consumer_active, consumer_live, prepare_consumer,
    release_consumer,
};
use std::{any::Any, cell::RefCell, collections::BTreeMap, rc::Rc};

/// Actual checker that produced the research observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorrowFrameChecker {
    /// File-local ownership checker.
    Single,
    /// Source-qualified compilation-unit checker.
    Unit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Binding {
    Single(SymbolId),
    Unit(UnitSymbolId),
}
impl From<SymbolId> for Binding {
    fn from(id: SymbolId) -> Self {
        Self::Single(id)
    }
}
impl From<UnitSymbolId> for Binding {
    fn from(id: UnitSymbolId) -> Self {
        Self::Unit(id)
    }
}

/// Actual ordinary-block evidence; never a loop or Complete capability.
#[derive(Clone, Debug)]
pub struct BorrowFrameObservation {
    checker: BorrowFrameChecker,
    scope_span: Span,
    local_binding_count: usize,
    end_count: usize,
    external_unchanged: bool,
    closed: bool,
    identity: Rc<()>,
    returns: Vec<BorrowFrameConsumerReturn>,
    returned_and_closed: bool,
}
impl BorrowFrameObservation {
    /// Producing checker.
    pub fn checker(&self) -> BorrowFrameChecker {
        self.checker
    }
    /// Actual ordinary Block source span.
    pub fn scope_span(&self) -> Span {
        self.scope_span
    }
    /// Number of actual acquisitions in this frame.
    pub fn local_binding_count(&self) -> usize {
        self.local_binding_count
    }
    /// Number of actual checked removal events.
    pub fn end_count(&self) -> usize {
        self.end_count
    }
    /// Whether the normalized core resource states agree.
    pub fn external_unchanged(&self) -> bool {
        self.external_unchanged
    }
    /// Actual normal consumer returns in this ordinary frame, never loop iterations.
    pub fn consumer_returns(&self) -> &[BorrowFrameConsumerReturn] {
        &self.returns
    }
    /// Whether this event was produced by this exact frame invocation.
    pub fn owns_consumer_return(&self, event: &BorrowFrameConsumerReturn) -> bool {
        Rc::ptr_eq(&self.identity, &event.identity)
    }
    /// Complete local acquire/consumer-return/last-End chain and external restoration.
    /// This is research evidence for an ordinary Block, not a Complete loan capability.
    pub fn complete_closure(&self) -> bool {
        self.validate() && self.returned_and_closed
    }
    /// Accept only real local closure together with unchanged external resource obligations.
    pub fn validate(&self) -> bool {
        self.closed && self.external_unchanged
    }
}
struct Acquisition {
    binding: Binding,
    snapshot: Box<dyn Any>,
    ended: bool,
}
struct Frame {
    identity: Rc<()>,
    checker: BorrowFrameChecker,
    source_unit: Option<SourceUnitId>,
    span: Span,
    elements: Vec<StatementId>,
    eligible: bool,
    valid: bool,
    acquired: Vec<Acquisition>,
    ends: Vec<(Binding, StatementId)>,
    consumers: consumer::Tape,
}
#[derive(Default)]
struct Capture {
    frames: Vec<Frame>,
    observations: Vec<BorrowFrameObservation>,
}
thread_local! { static CAPTURE: RefCell<Option<Capture>> = const { RefCell::new(None) }; }
/// Capture only actual core-checker frame observations during this call.
pub fn capture_borrow_frames<T>(run: impl FnOnce() -> T) -> (T, Vec<BorrowFrameObservation>) {
    struct Restore(Option<Capture>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CAPTURE.with(|cell| *cell.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(CAPTURE.with(|cell| cell.replace(Some(Capture::default()))));
    let value = run();
    let observations =
        CAPTURE.with(|cell| std::mem::take(&mut cell.borrow_mut().as_mut().unwrap().observations));
    (value, observations)
}
pub(crate) fn is_capturing() -> bool {
    CAPTURE.with(|cell| cell.borrow().is_some())
}

pub(crate) struct Start<S> {
    entry: S,
    identity: Rc<()>,
}
impl<S> Drop for Start<S> {
    fn drop(&mut self) {
        CAPTURE.with(|cell| {
            if let Some(c) = cell.borrow_mut().as_mut()
                && c.frames
                    .last()
                    .is_some_and(|f| Rc::ptr_eq(&f.identity, &self.identity))
            {
                c.frames.pop();
            }
        });
    }
}
pub(crate) fn begin<S: Clone>(
    context: (BorrowFrameChecker, Option<SourceUnitId>),
    parsed: &ParsedFile,
    id: StatementId,
    state: &S,
    simple_entry: bool,
    direct_call: impl Fn(ExpressionId, bool) -> bool,
    resolve: impl Fn(Name) -> Option<Binding>,
) -> Option<Start<S>> {
    CAPTURE.with(|cell| {
        let mut cell = cell.borrow_mut();
        let c = cell.as_mut()?;
        let node = parsed.ast().statements().get(id).ok()?;
        let (elements, eligible) = match node.payload() {
            Statement::Block { elements } => {
                let valid = simple_entry
                    && elements.iter().all(|id| {
                        parsed.ast().statements().get(*id).is_ok_and(|n| {
                            let (expression, initializer) = match n.payload() {
                                Statement::LocalVariable { declaration } => match parsed
                                    .ast()
                                    .items()
                                    .get(*declaration)
                                    .map(|n| n.payload())
                                {
                                    Ok(Item::Variable {
                                        kind: VariableKind::BorrowVal(_),
                                        initializer,
                                        ..
                                    }) => (*initializer, true),
                                    _ => return false,
                                },
                                Statement::Expression { expression } => (*expression, false),
                                _ => return false,
                            };
                            let Ok(node) = parsed.ast().expressions().get(expression) else {
                                return false;
                            };
                            let Expression::Call {
                                callee, arguments, ..
                            } = node.payload()
                            else {
                                return false;
                            };
                            parsed
                                .ast()
                                .expressions()
                                .get(*callee)
                                .is_ok_and(|n| matches!(n.payload(), Expression::Name))
                                && arguments.iter().all(|arg| {
                                    parsed
                                        .ast()
                                        .expressions()
                                        .get(arg.value)
                                        .is_ok_and(|n| matches!(n.payload(), Expression::Name))
                                })
                                && direct_call(expression, initializer)
                        })
                    });
                (elements.clone(), valid)
            }
            _ => (Vec::new(), false),
        };
        let identity = Rc::new(());
        let plan = consumer::plan(parsed, &elements, resolve);
        c.frames.push(Frame {
            identity: identity.clone(),
            checker: context.0,
            source_unit: context.1,
            span: node.span(),
            elements,
            eligible,
            valid: true,
            acquired: Vec::new(),
            ends: Vec::new(),
            consumers: consumer::Tape::new(plan),
        });
        Some(Start {
            entry: state.clone(),
            identity,
        })
    })
}
pub(crate) fn acquired<B: Copy + Into<Binding>, P: Clone + 'static, L: Clone + 'static>(
    binding: B,
    place: Option<&P>,
    marker: Option<&Span>,
    loans: Vec<L>,
) {
    CAPTURE.with(|cell| {
        if let Some(f) = cell.borrow_mut().as_mut().and_then(|c| c.frames.last_mut()) {
            let binding = binding.into();
            if f.acquired.iter().any(|a| a.binding == binding)
                || place.is_none()
                || marker.is_none()
                || loans.is_empty()
            {
                f.valid = false;
                return;
            }
            consumer::acquire(f, binding);
            f.acquired.push(Acquisition {
                binding,
                snapshot: Box::new((place.cloned(), marker.copied(), loans)),
                ended: false,
            });
        }
    });
}
pub(crate) struct Removal<B, P, L> {
    frame: Rc<()>,
    binding: B,
    bindings: BTreeMap<B, P>,
    nonowning: BTreeMap<B, Span>,
    loans: Vec<L>,
    valid: bool,
}
pub(crate) fn removing<
    B: Copy + Ord + Into<Binding>,
    P: Clone + PartialEq + 'static,
    L: Clone + PartialEq + 'static,
>(
    binding: B,
    bindings: &BTreeMap<B, P>,
    nonowning: &BTreeMap<B, Span>,
    loans: &[L],
    owned: impl Fn(&L) -> bool,
) -> Option<Removal<B, P, L>> {
    CAPTURE.with(|cell| {
        let cell = cell.borrow();
        let f = cell.as_ref()?.frames.last()?;
        let snapshot = (
            bindings.get(&binding).cloned(),
            nonowning.get(&binding).copied(),
            loans
                .iter()
                .filter(|l| owned(l))
                .cloned()
                .collect::<Vec<_>>(),
        );
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
        let mut bindings = bindings.clone();
        bindings.remove(&binding);
        let mut nonowning = nonowning.clone();
        nonowning.remove(&binding);
        Some(Removal {
            frame: f.identity.clone(),
            binding,
            bindings,
            nonowning,
            loans: loans.iter().filter(|l| !owned(l)).cloned().collect(),
            valid,
        })
    })
}
pub(crate) fn removed<B: Copy + Ord + Into<Binding>, P: PartialEq, L: PartialEq>(
    ticket: Option<Removal<B, P, L>>,
    statement: StatementId,
    bindings: &BTreeMap<B, P>,
    nonowning: &BTreeMap<B, Span>,
    loans: &[L],
) {
    let Some(t) = ticket else { return };
    CAPTURE.with(|cell| {
        if let Some(f) = cell.borrow_mut().as_mut().and_then(|c| c.frames.last_mut()) {
            let binding = t.binding.into();
            if !Rc::ptr_eq(&f.identity, &t.frame)
                || !t.valid
                || t.bindings != *bindings
                || t.nonowning != *nonowning
                || t.loans != loans
                || !f.elements.contains(&statement)
            {
                f.valid = false;
                return;
            }
            if let Some(a) = f.acquired.iter_mut().find(|a| a.binding == binding) {
                a.ended = true;
            } else {
                f.valid = false;
            }
            consumer::close_binding(f, binding, statement);
            f.ends.push((binding, statement));
        }
    });
}
pub(crate) fn finish<S: PartialEq, B: Copy + Into<Binding>>(
    start: Option<Start<S>>,
    exit: Option<&S>,
    clean: bool,
    actual_ends: &[(B, Option<StatementId>)],
) {
    let Some(start) = start else { return };
    CAPTURE.with(|cell| {
        let mut cell = cell.borrow_mut();
        let Some(c) = cell.as_mut() else { return };
        let Some(f) = c.frames.pop() else { return };
        if !Rc::ptr_eq(&f.identity, &start.identity) {
            return;
        }
        if !f.eligible || f.acquired.is_empty() {
            return;
        }
        let actual = actual_ends
            .iter()
            .map(|(b, s)| ((*b).into(), *s))
            .collect::<Vec<_>>();
        let expected = f
            .ends
            .iter()
            .map(|(b, s)| (*b, Some(*s)))
            .collect::<Vec<_>>();
        let closed = clean
            && exit.is_some()
            && f.valid
            && f.acquired.iter().all(|a| a.ended)
            && f.ends.len() == f.acquired.len()
            && actual == expected;
        let returned_and_closed = consumer::complete_tape(&f);
        c.observations.push(BorrowFrameObservation {
            checker: f.checker,
            scope_span: f.span,
            local_binding_count: f.acquired.len(),
            end_count: f.ends.len(),
            external_unchanged: exit == Some(&start.entry),
            closed,
            identity: f.identity,
            returns: f.consumers.returns,
            returned_and_closed,
        });
    });
}
