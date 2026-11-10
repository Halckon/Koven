//! Protocol-2 production receipt observer; initially empty to expose the capability gap.
#[cfg(test)]
pub(crate) fn observe<T>(lower: impl FnOnce() -> T) -> (T, Vec<()>) {
    (lower(), Vec::new())
}
