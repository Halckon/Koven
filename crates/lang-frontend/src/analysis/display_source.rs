//! Registration of the compiler's embedded Display declaration.
use super::UnitSourceDescriptor;
use crate::{
    name_resolution::NameEnvironment,
    source::{SourceError, SourceMap},
};

/// Logical path of the embedded Display source.
pub const DISPLAY_PATH: &str = "koven/core/display.ko";
const TEXT: &str = include_str!("../../../lang-std/koven/core/display.ko");

/// Adds the canonical Display source to a host's source set and authorizes its exact SourceId.
pub fn append_builtin_display(
    sources: &mut SourceMap,
    descriptors: &mut Vec<UnitSourceDescriptor>,
    environment: &mut NameEnvironment,
    standard_root: &str,
) -> Result<(), SourceError> {
    let mut root = standard_root.to_owned();
    let mut suffix = 0;
    while descriptors.iter().any(|source| {
        source.root_identity() == Some(&root) && source.logical_path() == Some(DISPLAY_PATH)
    }) {
        suffix += 1;
        root = format!("{standard_root}-{suffix}");
    }
    let display_name = format!("koven-std/{DISPLAY_PATH}");
    let mut source = sources.add_source(&display_name, TEXT);
    for suffix in 1..=descriptors.len() {
        if !matches!(source, Err(SourceError::DuplicateSourceName { .. })) {
            break;
        }
        source = sources.add_source(format!("{display_name} [standard source {suffix}]"), TEXT);
    }
    let source = source?;
    environment.authorize_display_source(sources, source)?;
    descriptors.push(UnitSourceDescriptor::embedded(root, DISPLAY_PATH, source));
    Ok(())
}
