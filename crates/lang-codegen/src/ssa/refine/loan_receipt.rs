//! Production protocol-4 verification and its read-only test observation.
#[cfg(test)]
thread_local! {
    static RECEIPTS: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn observe<T>(lower: impl FnOnce() -> T) -> (T, Vec<String>) {
    RECEIPTS.with(|receipts| {
        assert!(receipts.borrow().is_none());
        *receipts.borrow_mut() = Some(Vec::new());
    });
    let result = lower();
    let receipts = RECEIPTS.with(|receipts| receipts.borrow_mut().take().unwrap());
    (result, receipts)
}
