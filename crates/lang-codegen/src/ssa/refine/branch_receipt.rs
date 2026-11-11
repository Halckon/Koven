//! Test-only empty observation shell for the branch production-receipt capability red.
//! No production consumer, certificate reconstruction, or receipt publisher exists yet.
#[cfg(test)]
pub(crate) struct Receipt {
    /// Placeholder cardinality only; real path identities must replace this in production.
    pub(crate) paths: Vec<()>,
}

#[cfg(test)]
pub(crate) fn observe<T>(lower: impl FnOnce() -> T) -> (T, Vec<Receipt>) {
    (lower(), Vec::new())
}
