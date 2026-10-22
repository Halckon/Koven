//! 编译器自带 Koven 源码资产的加载边界；授权只绑定宿主新建的 SourceId。
use crate::unit_build::UnitBuildError;
use lang_frontend::{
    analysis::UnitSourceDescriptor,
    source::{SourceError, SourceMap},
    type_checking::TypeEnvironment,
};

const RANGE_PATH: &str = "koven/algorithms/ranges.ko";
const RANGE_TEXT: &str = include_str!("../../lang-std/koven/algorithms/ranges.ko");
const STANDARD_ROOT: &str = "koven-std";

// CLI caller 必须为 map 中每份既有 source 登记一个 descriptor；后续 single adapter 也须保持。
// N 份既有 source 最多占用 N 个候选展示名；不把该私有边界推广为任意 SourceMap 的计数 API。
pub(crate) fn append_standard_sources(
    sources: &mut SourceMap,
    descriptors: &mut Vec<UnitSourceDescriptor>,
    types: &mut TypeEnvironment,
) -> Result<(), UnitBuildError> {
    let display_name = format!("{STANDARD_ROOT}/{RANGE_PATH}");
    let mut registered = sources.add_source(&display_name, RANGE_TEXT);
    for suffix in 1..=descriptors.len() {
        if !matches!(&registered, Err(SourceError::DuplicateSourceName { .. })) {
            break;
        }
        registered = sources.add_source(
            format!("{display_name} [standard source {suffix}]"),
            RANGE_TEXT,
        );
    }
    // 若私有 caller 没有完整登记 descriptor，保留最后真实错误，不无界尝试或伪造成功。
    let source = registered.map_err(UnitBuildError::Source)?;
    types
        .authorize_range_source(sources, source)
        .map_err(UnitBuildError::StandardSource)?;
    types
        .authorize_range_extension_source(sources, source)
        .map_err(UnitBuildError::StandardSource)?;
    descriptors.push(UnitSourceDescriptor::new(STANDARD_ROOT, RANGE_PATH, source));
    Ok(())
}

#[cfg(test)]
mod tests;
