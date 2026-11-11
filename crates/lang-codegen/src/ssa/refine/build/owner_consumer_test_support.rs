//! Private CI-only mutations and ordinary observation of actual production transitions.
use std::cell::{Cell, RefCell};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum OwnerFault {
    MissingReturnReplaceReceipt,
    DuplicateReturnReplaceReceipt,
    ReturnReplaceOldIsRoot,
    ReturnReplaceDropIsOld,
    MissingReturnReplaceDropReceipt,
    DuplicateReturnReplaceDropReceipt,
    DuplicateTake,
    DuplicateBind,
    DuplicateDrop,
    WrongSlot,
    WrongIndex,
    MissingFinish,
    MissingIncoming,
    ForeignIncoming,
    ReversedIncomingRoles,
    DuplicateOwnCall,
    WrongOwnCallSlot,
    MissingOwnCall,
    DuplicateOwnerReturn,
    WrongReturnPayload,
    MissingOwnerReturn,
    ExtraHeaderAlias,
}
thread_local! {
    static FAULT: Cell<Option<OwnerFault>> = const { Cell::new(None) };
    static ACTIONS: RefCell<Option<Vec<(&'static str, usize)>>> = const { RefCell::new(None) };
}
pub(super) fn take_fault(expected: OwnerFault) -> bool {
    FAULT.with(|fault| {
        if fault.get() == Some(expected) {
            fault.set(None);
            true
        } else {
            false
        }
    })
}
pub(crate) fn with_owner_fault<T>(fault: OwnerFault, run: impl FnOnce() -> T) -> T {
    struct Reset(Option<OwnerFault>);
    impl Drop for Reset {
        fn drop(&mut self) {
            FAULT.with(|fault| fault.set(self.0));
        }
    }
    let _reset = Reset(FAULT.with(|state| state.replace(Some(fault))));
    run()
}
pub(super) fn record_action(action: &'static str, slot: usize) {
    ACTIONS.with(|events| {
        if let Some(events) = events.borrow_mut().as_mut() {
            events.push((action, slot));
        }
    });
}
pub(crate) fn observe_owner_actions<T>(run: impl FnOnce() -> T) -> (T, Vec<(&'static str, usize)>) {
    ACTIONS.with(|events| *events.borrow_mut() = Some(Vec::new()));
    let result = run();
    let events = ACTIONS.with(|events| events.borrow_mut().take().unwrap_or_default());
    (result, events)
}
