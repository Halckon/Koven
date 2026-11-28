//! 编译器自带 Koven 源码资产的加载边界；授权只绑定宿主新建的 SourceId。
use std::collections::BTreeSet;

use crate::unit_build::UnitBuildError;
use lang_frontend::{
    analysis::{DISPLAY_PATH, UnitSourceDescriptor, append_builtin_display},
    name_resolution::NameEnvironment,
    source::{SourceError, SourceId, SourceMap},
    type_checking::TypeEnvironment,
};

const RANGE_PATH: &str = "koven/algorithms/ranges.ko";
const RANGE_TEXT: &str = include_str!("../../lang-std/koven/algorithms/ranges.ko");
const RESULT_PATH: &str = "koven/core/result.ko";
const RESULT_TEXT: &str = include_str!("../../lang-std/koven/core/result.ko");
const TEXT_PATH: &str = "koven/text/text.ko";
const TEXT_TEXT: &str = include_str!("../../lang-std/koven/text/text.ko");
const STANDARD_ROOT: &str = "koven-std";

// CLI caller 必须为 map 中每份既有 source 登记一个 descriptor；后续 single adapter 也须保持。
// N 份既有 source 最多占用 N 个候选展示名；不把该私有边界推广为任意 SourceMap 的计数 API。
// 依次追加 ranges、result、text、display，既有标准 descriptor 位置保持不变。
pub(crate) fn append_standard_sources(
    sources: &mut SourceMap,
    descriptors: &mut Vec<UnitSourceDescriptor>,
    names: &mut NameEnvironment,
    types: &mut TypeEnvironment,
) -> Result<(), UnitBuildError> {
    let root = available_standard_root(descriptors);
    let range = register(sources, descriptors.len(), RANGE_PATH, RANGE_TEXT)?;
    types
        .authorize_range_source(sources, range)
        .map_err(UnitBuildError::StandardSource)?;
    types
        .authorize_range_extension_source(sources, range)
        .map_err(UnitBuildError::StandardSource)?;
    descriptors.push(UnitSourceDescriptor::new(&root, RANGE_PATH, range));

    let result = register(sources, descriptors.len(), RESULT_PATH, RESULT_TEXT)?;
    names
        .authorize_builtin_source(sources, result)
        .map_err(UnitBuildError::Source)?;
    descriptors.push(UnitSourceDescriptor::new(&root, RESULT_PATH, result));
    let text = register(sources, descriptors.len(), TEXT_PATH, TEXT_TEXT)?;
    types
        .authorize_text_source(sources, text)
        .map_err(UnitBuildError::StandardSource)?;
    descriptors.push(UnitSourceDescriptor::new(&root, TEXT_PATH, text));
    append_builtin_display(sources, descriptors, names, &root).map_err(UnitBuildError::Source)?;
    Ok(())
}

// 用户 root/path 不变；只避让实际重合的完整键，标准源共享同一宿主 root。
fn available_standard_root(descriptors: &[UnitSourceDescriptor]) -> String {
    let occupied = descriptors
        .iter()
        .filter(|source| {
            matches!(
                source.logical_path(),
                Some(RANGE_PATH | RESULT_PATH | TEXT_PATH | DISPLAY_PATH)
            )
        })
        .filter_map(UnitSourceDescriptor::root_identity)
        .collect::<BTreeSet<_>>();
    let mut root = STANDARD_ROOT.to_owned();
    // N 个占用 root 至多阻挡 N 个候选；原 root 加 N 个后缀必有空闲项。
    for suffix in 1..=occupied.len() {
        if !occupied.contains(root.as_str()) {
            break;
        }
        root = format!("{STANDARD_ROOT}-{suffix}");
    }
    root
}

fn register(
    sources: &mut SourceMap,
    existing: usize,
    path: &str,
    text: &'static str,
) -> Result<SourceId, UnitBuildError> {
    let display_name = format!("{STANDARD_ROOT}/{path}");
    let mut registered = sources.add_source(&display_name, text);
    for suffix in 1..=existing {
        if !matches!(&registered, Err(SourceError::DuplicateSourceName { .. })) {
            break;
        }
        registered = sources.add_source(format!("{display_name} [standard source {suffix}]"), text);
    }
    // 若私有 caller 没有完整登记 descriptor，保留最后真实错误，不无界尝试或伪造成功。
    registered.map_err(UnitBuildError::Source)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod source_identity_tests;
