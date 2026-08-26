//! 严格 version 1 project manifest 与本地 immutable source-set provider。

mod discovery;
mod error;
mod manifest;
mod model;

#[cfg(test)]
mod tests;

use std::{ffi::OsStr, fs, path::Path};

pub(crate) use error::ProjectLoadError;
use error::{IoOperation, Utf8Subject};
pub(crate) use model::ProjectSourceSet;

/// 从调用方显式提供的 `project.toml` 加载本地 base source-set。
///
/// 此入口只执行 manifest 与文件系统 IO，不运行任何 Koven frontend 阶段。
pub(crate) fn load_project_source_set(
    manifest_path: &Path,
) -> Result<ProjectSourceSet, ProjectLoadError> {
    if manifest_path.file_name() != Some(OsStr::new("project.toml")) {
        return Err(ProjectLoadError::InvalidManifestFileName {
            path: manifest_path.to_path_buf(),
        });
    }
    let bytes = fs::read(manifest_path).map_err(|error| {
        ProjectLoadError::io(
            IoOperation::ReadManifest,
            manifest_path.to_path_buf(),
            error,
        )
    })?;
    let text = String::from_utf8(bytes).map_err(|_| ProjectLoadError::InvalidUtf8 {
        subject: Utf8Subject::Manifest,
        path: manifest_path.to_path_buf(),
    })?;
    let manifest = manifest::parse_manifest(manifest_path, &text)?;
    let manifest_dir = manifest_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    discovery::discover_project(manifest_dir, manifest)
}
