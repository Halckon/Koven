//! Provider source capabilities: a value snapshot never carries a loan target.
/// Distinguishes owned value snapshots from borrowed provider sources.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IterationSource<T, E> {
    /// Existing container/View source with a shared access lifetime.
    BorrowedSource(T),
    /// Copyable value evaluated once before entering the loop.
    OwnedSnapshot(E),
}
impl<T, E> IterationSource<T, E> {
    /// A loan target exists only for borrowed providers.
    #[must_use]
    pub const fn borrowed(&self) -> Option<&T> {
        match self {
            Self::BorrowedSource(target) => Some(target),
            Self::OwnedSnapshot(_) => None,
        }
    }
}
