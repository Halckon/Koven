//! Test-facing observation surface before the independent child receipt consumer exists.
pub(crate) fn observe<T>(lower: impl FnOnce() -> T) -> (T, Vec<()>) {
    (lower(), Vec::new())
}
