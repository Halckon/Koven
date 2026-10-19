//! Fixed scope groups are registered only after their corresponding bounded repair.
mod int_storage;
pub(crate) use int_storage::sources as int_storage_sources;

mod borrow_alias_observer;

pub(crate) use borrow_alias_observer::cases as borrow_alias_sources;
