//! Phase 2 单文件类型检查与可供所有权阶段消费的 typed facts。

mod call;
mod checker;
mod container;
mod error;
mod model;
mod parameter;
mod projection;

use std::{sync::Arc, thread};

use crate::{name_resolution::NameResolution, parser::ParsedFile, source::SourceMap};

pub use call::*;
pub use container::*;
pub use error::TypeCheckingError;
pub use model::*;
pub use parameter::*;
pub use projection::*;

/// 对已完成名称解析的单文件执行当前 Phase 2 类型检查流水线。
pub fn check_types(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    environment: &TypeEnvironment,
) -> Result<TypedFile, TypeCheckingError> {
    if parsed.source_id() != names.source_id() {
        return Err(TypeCheckingError::MismatchedNameSource);
    }
    if !Arc::ptr_eq(names.environment_owner(), environment.owner()) {
        return Err(TypeCheckingError::MismatchedNameEnvironment);
    }
    thread::scope(|scope| {
        thread::Builder::new()
            .name("koven-type-checker".to_owned())
            .stack_size(32 * 1024 * 1024)
            .spawn_scoped(scope, || {
                checker::check(sources, parsed, names, environment)
            })
            .map_err(|error| TypeCheckingError::CheckerThread(error.kind()))?
            .join()
            .map_err(|_| TypeCheckingError::CheckerThreadPanicked)?
    })
}
