//! Phase 3 变量所有权状态与 use-after-move 检查。

mod checker;
mod error;
mod model;

use crate::{
    name_resolution::NameResolution, parser::ParsedFile, source::SourceMap,
    type_checking::TypedFile,
};

pub use error::OwnershipCheckingError;
pub use model::{
    DropFact, DropPoint, DropTarget, LoanFact, LoanKind, LoanTarget, OwnershipBindingDescriptor,
    OwnershipBindingKind, OwnershipCheckedFile, OwnershipDeferredFact, OwnershipDeferredReason,
    OwnershipPlace,
};

/// 对同一源码的名称、类型产物执行变量所有权检查。
pub fn check_ownership(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
) -> Result<OwnershipCheckedFile, OwnershipCheckingError> {
    let source_id = parsed.source_id();
    if names.source_id() != source_id {
        return Err(OwnershipCheckingError::MismatchedNameSource);
    }
    if typed.source_id() != source_id {
        return Err(OwnershipCheckingError::MismatchedTypedSource);
    }
    checker::check(sources, parsed, names, typed)
}
