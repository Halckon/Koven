//! v0.21 单文件双命名空间、作用域与名称诊断。

mod compilation_unit;
mod error;
mod model;
mod resolver;

use std::thread;

use crate::{parser::ParsedFile, source::SourceMap};

pub use compilation_unit::*;
pub use error::NameResolutionError;
pub use model::*;

/// 对已解析文件执行单文件名称解析。
///
/// package/import 不在此入口展开；调用方必须显式提供不可变外部环境。
pub fn resolve_names(
    sources: &SourceMap,
    parsed: &ParsedFile,
    environment: &NameEnvironment,
) -> Result<NameResolution, NameResolutionError> {
    thread::scope(|scope| {
        thread::Builder::new()
            .name("koven-name-resolver".to_owned())
            .stack_size(32 * 1024 * 1024)
            .spawn_scoped(scope, || resolver::resolve(sources, parsed, environment))
            .map_err(|error| NameResolutionError::ResolverThread(error.kind()))?
            .join()
            .map_err(|_| NameResolutionError::ResolverThreadPanicked)?
    })
}
