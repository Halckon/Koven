//! 编译器自带 Koven 源码资产的加载边界；授权只绑定宿主新建的 SourceId。
use crate::unit_build::UnitBuildError;
use lang_frontend::{
    analysis::UnitSourceDescriptor, source::SourceMap, type_checking::TypeEnvironment,
};

const RANGE_PATH: &str = "koven/algorithms/ranges.ko";
const RANGE_TEXT: &str = include_str!("../../lang-std/koven/algorithms/ranges.ko");
const STANDARD_ROOT: &str = "koven-std";

pub(crate) fn append_standard_sources(
    sources: &mut SourceMap,
    descriptors: &mut Vec<UnitSourceDescriptor>,
    types: &mut TypeEnvironment,
) -> Result<(), UnitBuildError> {
    let source = sources
        .add_source(format!("{STANDARD_ROOT}/{RANGE_PATH}"), RANGE_TEXT)
        .map_err(UnitBuildError::Source)?;
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
