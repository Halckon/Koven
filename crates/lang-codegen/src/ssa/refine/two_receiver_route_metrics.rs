//! Test-only deterministic work counts for the exact paired receiver route.
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ssa) struct Counts {
    pub(in crate::ssa) bindings: usize,
    pub(in crate::ssa) ends: usize,
    pub(in crate::ssa) drops: usize,
    pub(in crate::ssa) probes: usize,
}
thread_local! {
    static COUNTS: Cell<Option<Counts>> = const { Cell::new(None) };
}
pub(super) fn visit(record: impl FnOnce(&mut Counts)) {
    COUNTS.with(|counts| {
        if let Some(mut current) = counts.get() {
            record(&mut current);
            counts.set(Some(current));
        }
    });
}
pub(in crate::ssa) fn observe<T>(run: impl FnOnce() -> T) -> (T, Counts) {
    COUNTS.with(|counts| assert!(counts.replace(Some(Counts::default())).is_none()));
    let result = run();
    (result, COUNTS.with(|counts| counts.take().unwrap()))
}
