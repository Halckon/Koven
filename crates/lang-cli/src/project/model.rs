use std::path::{Path, PathBuf};

/// 从 version 1 manifest 加载的不可变本地项目 source-set snapshot。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ProjectSourceSet {
    name: String,
    roots: Vec<ProjectSourceRoot>,
    sources: Vec<ProjectSource>,
}

impl ProjectSourceSet {
    pub(super) fn new(
        name: String,
        roots: Vec<ProjectSourceRoot>,
        sources: Vec<ProjectSource>,
    ) -> Self {
        Self {
            name,
            roots,
            sources,
        }
    }

    /// 返回 manifest 中的展示名称；它不参与 Koven package identity。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// 返回按 root identity 排序的 source roots。
    pub(crate) fn roots(&self) -> &[ProjectSourceRoot] {
        &self.roots
    }

    /// 返回按 `(root identity, logical path)` 排序的源码输入。
    pub(crate) fn sources(&self) -> &[ProjectSource] {
        &self.sources
    }
}

/// 一个 manifest-relative source root。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ProjectSourceRoot {
    identity: String,
    presentation_path: PathBuf,
}

impl ProjectSourceRoot {
    pub(super) fn new(identity: String, presentation_path: PathBuf) -> Self {
        Self {
            identity,
            presentation_path,
        }
    }

    /// 返回可直接交给 compilation-unit Stage 1 的稳定 root identity。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn identity(&self) -> &str {
        &self.identity
    }

    /// 返回用于错误和工具展示的宿主路径；它不参与 source identity。
    pub(crate) fn presentation_path(&self) -> &Path {
        &self.presentation_path
    }
}

/// 一份已经读取且通过 UTF-8 校验的 Koven 源码输入。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ProjectSource {
    root_identity: String,
    logical_path: String,
    presentation_path: PathBuf,
    text: String,
}

impl ProjectSource {
    pub(super) fn new(
        root_identity: String,
        logical_path: String,
        presentation_path: PathBuf,
        text: String,
    ) -> Self {
        Self {
            root_identity,
            logical_path,
            presentation_path,
            text,
        }
    }

    /// 返回可直接交给 compilation-unit Stage 1 的稳定 root identity。
    pub(crate) fn root_identity(&self) -> &str {
        &self.root_identity
    }

    /// 返回 root-relative、使用 `/` 分隔的逻辑源码路径。
    pub(crate) fn logical_path(&self) -> &str {
        &self.logical_path
    }

    /// 返回用于错误和工具展示的宿主路径；它不参与 source identity。
    pub(crate) fn presentation_path(&self) -> &Path {
        &self.presentation_path
    }

    /// 返回已经验证为 UTF-8 的完整源码文本。
    pub(crate) fn text(&self) -> &str {
        &self.text
    }
}
