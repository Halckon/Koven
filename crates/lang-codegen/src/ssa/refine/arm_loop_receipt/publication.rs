//! Test-only atomic publication and CI-gated one-shot production-boundary faults.
use super::{Emitted, Receipt};
use crate::ssa::model::Program;
use std::cell::RefCell;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fault {
    ReceiptMissing,
    CounterReceiptMissing,
    BreakReceiptMissing,
    BreakCaptureMissing,
    FinalVerifier,
}
#[derive(Default)]
struct Observations {
    pending: Vec<Receipt>,
    published: Vec<Receipt>,
    recorded: usize,
    before_verifier: usize,
    rolled_back: bool,
    fault: Option<Fault>,
}
#[derive(Debug)]
pub(crate) struct Trace {
    pub(crate) recorded: usize,
    pub(crate) before_verifier: usize,
    pub(crate) rolled_back: bool,
    pub(crate) pending_after: usize,
}
thread_local! { static STATE: RefCell<Option<Observations>> = const { RefCell::new(None) }; }
pub(super) fn require_ci() {
    assert_eq!(
        std::env::var("GITHUB_ACTIONS").as_deref(),
        Ok("true"),
        "faults are CI-only; local compile/list only"
    );
}
pub(crate) fn observe<T>(lower: impl FnOnce() -> T) -> (T, Vec<Receipt>, Trace) {
    capture(None, lower)
}
pub(super) fn with_fault<T>(fault: Fault, lower: impl FnOnce() -> T) -> (T, Vec<Receipt>, Trace) {
    require_ci();
    capture(Some(fault), lower)
}
fn capture<T>(fault: Option<Fault>, lower: impl FnOnce() -> T) -> (T, Vec<Receipt>, Trace) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            STATE.with(|s| {
                s.borrow_mut().take();
            });
        }
    }
    STATE.with(|s| {
        assert!(s.borrow().is_none());
        *s.borrow_mut() = Some(Observations {
            fault,
            ..Observations::default()
        });
    });
    let _reset = Reset;
    let result = lower();
    let state = STATE.with(|s| s.borrow_mut().take().unwrap());
    assert!(state.fault.is_none(), "actual boundary must consume fault");
    (
        result,
        state.published,
        Trace {
            recorded: state.recorded,
            before_verifier: state.before_verifier,
            rolled_back: state.rolled_back,
            pending_after: state.pending.len(),
        },
    )
}
pub(super) fn record(receipt: Receipt) {
    STATE.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            s.recorded += 1;
            s.pending.push(receipt);
        }
    });
}
pub(super) fn before_finish(evidence: &mut Emitted) {
    STATE.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut()
            && !evidence.selected.is_empty()
            && s.fault == Some(Fault::ReceiptMissing)
        {
            require_ci();
            s.fault = None;
            evidence.selected.clear();
        }
        if let Some(s) = s.borrow_mut().as_mut()
            && !evidence.counter_selected.is_empty()
            && s.fault == Some(Fault::CounterReceiptMissing)
        {
            require_ci();
            s.fault = None;
            evidence.counter_selected.clear();
        }
        if let Some(s) = s.borrow_mut().as_mut()
            && !evidence.break_selected.is_empty()
            && s.fault == Some(Fault::BreakReceiptMissing)
        {
            require_ci();
            s.fault = None;
            evidence.break_selected.clear();
        }
    });
}
pub(in crate::ssa) fn before_final_verifier(program: &mut Program) {
    STATE.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            s.before_verifier = s.pending.len();
            if s.fault == Some(Fault::FinalVerifier) {
                require_ci();
                s.fault = None;
                assert_eq!(s.pending.len(), 2, "two actual pending callers");
                assert!(s.published.is_empty());
                let caller = s.pending[0].caller;
                let f = program
                    .modules
                    .iter_mut()
                    .flat_map(|m| &mut m.functions)
                    .find(|f| f.id == caller)
                    .unwrap();
                let entry = f.entry_block().unwrap();
                f.blocks[entry.index()].terminator = None;
            }
        }
    });
}
pub(in crate::ssa) struct UnitPublication {
    committed: bool,
    start: usize,
}
impl UnitPublication {
    pub(in crate::ssa) fn begin() -> Self {
        Self {
            committed: false,
            start: STATE.with(|s| s.borrow().as_ref().map_or(0, |s| s.pending.len())),
        }
    }
    pub(in crate::ssa) fn commit(mut self) {
        STATE.with(|s| {
            if let Some(s) = s.borrow_mut().as_mut() {
                s.published.extend(s.pending.drain(self.start..));
            }
        });
        self.committed = true;
    }
}
impl Drop for UnitPublication {
    fn drop(&mut self) {
        if !self.committed {
            STATE.with(|s| {
                if let Some(s) = s.borrow_mut().as_mut() {
                    s.pending.truncate(self.start);
                    s.rolled_back = true;
                }
            });
        }
    }
}

pub(in crate::ssa::refine) fn skip_break_capture() -> bool {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let Some(s) = s.as_mut() else { return false };
        if s.fault != Some(Fault::BreakCaptureMissing) {
            return false;
        }
        require_ci();
        s.fault = None;
        true
    })
}
