use std::{error::Error, fmt, io, path::PathBuf};

/// project manifest 或 filesystem provider 的失败。
#[derive(Debug)]
pub(crate) enum ProjectLoadError {
    /// 调用方没有显式提供文件名为 `project.toml` 的 manifest。
    InvalidManifestFileName { path: PathBuf },
    /// manifest 或 source 内容不是 UTF-8。
    InvalidUtf8 { subject: Utf8Subject, path: PathBuf },
    /// TOML 文本不合法。
    InvalidToml { path: PathBuf, detail: String },
    /// 严格 schema 中缺少字段。
    MissingField { field: &'static str },
    /// 严格 schema 中出现未知字段。
    UnknownField {
        section: &'static str,
        field: String,
    },
    /// 字段类型不符合 version 1 schema。
    InvalidFieldType {
        field: &'static str,
        expected: &'static str,
    },
    /// schema identity 不是 `koven.project`。
    UnsupportedSchema { found: String },
    /// manifest version 不是整数 `1`。
    UnsupportedVersion { found: i64 },
    /// `project.name` 不满足 ASCII identity 约束。
    InvalidProjectName { name: String },
    /// `project.source-roots` 为空。
    EmptySourceRoots,
    /// source root 字符串不满足可移植相对路径约束。
    InvalidSourceRoot {
        root: String,
        reason: SourceRootError,
    },
    /// source root identity 在 manifest 中重复。
    DuplicateSourceRoot { root: String },
    /// 两个逻辑或物理 root 相同或互相嵌套。
    OverlappingSourceRoots { first: String, second: String },
    /// root 的任一物理路径段是 symlink。
    SymlinkSourceRoot { root: String, path: PathBuf },
    /// root 不存在。
    MissingSourceRoot { root: String, path: PathBuf },
    /// root 的最终路径不是目录。
    SourceRootNotDirectory { root: String, path: PathBuf },
    /// root 经物理解析后逃出 manifest 目录。
    SourceRootEscapesProject { root: String, path: PathBuf },
    /// 纳入遍历的目录段或 `.ko` 文件名不是 UTF-8。
    InvalidSourcePathEncoding { path: PathBuf },
    /// 同一个物理 `.ko` 文件通过两个 source key 被发现。
    DuplicatePhysicalSource { first: PathBuf, second: PathBuf },
    /// 宿主文件系统没有提供可靠的普通文件物理 identity。
    PhysicalSourceIdentityUnavailable { path: PathBuf },
    /// 具体文件系统操作失败。
    Io {
        operation: IoOperation,
        path: PathBuf,
        source: io::Error,
    },
}

/// UTF-8 错误所属的数据类别。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Utf8Subject {
    Manifest,
    Source,
}

/// source root 字符串的稳定拒绝原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceRootError {
    Empty,
    Absolute,
    EmptySegment,
    CurrentSegment,
    ParentSegment,
    Backslash,
    TrailingSeparator,
    WindowsPrefix,
}

/// project provider 可能失败的文件系统操作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IoOperation {
    ReadManifest,
    InspectRoot,
    CanonicalizeProject,
    CanonicalizeRoot,
    ReadDirectory,
    InspectEntry,
    CanonicalizeSource,
    ReadSource,
}

impl ProjectLoadError {
    pub(super) fn io(operation: IoOperation, path: PathBuf, source: io::Error) -> Self {
        Self::Io {
            operation,
            path,
            source,
        }
    }
}

impl fmt::Display for ProjectLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidManifestFileName { path } => write!(
                formatter,
                "project manifest path must end with project.toml: {}",
                path.display()
            ),
            Self::InvalidUtf8 { subject, path } => write!(
                formatter,
                "{} {} is not valid UTF-8",
                subject.description(),
                path.display()
            ),
            Self::InvalidToml { path, detail } => {
                write!(formatter, "invalid TOML in {}: {detail}", path.display())
            }
            Self::MissingField { field } => write!(formatter, "missing manifest field {field}"),
            Self::UnknownField { section, field } => {
                write!(formatter, "unknown field {section}.{field}")
            }
            Self::InvalidFieldType { field, expected } => {
                write!(formatter, "manifest field {field} must be {expected}")
            }
            Self::UnsupportedSchema { found } => {
                write!(formatter, "unsupported project schema {found:?}")
            }
            Self::UnsupportedVersion { found } => {
                write!(formatter, "unsupported project manifest version {found}")
            }
            Self::InvalidProjectName { name } => {
                write!(formatter, "invalid project.name {name:?}")
            }
            Self::EmptySourceRoots => formatter.write_str("project.source-roots must not be empty"),
            Self::InvalidSourceRoot { root, reason } => {
                write!(formatter, "invalid source root {root:?}: {reason}")
            }
            Self::DuplicateSourceRoot { root } => {
                write!(formatter, "duplicate source root {root:?}")
            }
            Self::OverlappingSourceRoots { first, second } => {
                write!(formatter, "source roots {first:?} and {second:?} overlap")
            }
            Self::SymlinkSourceRoot { root, path } => write!(
                formatter,
                "source root {root:?} contains symlink path {}",
                path.display()
            ),
            Self::MissingSourceRoot { root, path } => write!(
                formatter,
                "source root {root:?} does not exist at {}",
                path.display()
            ),
            Self::SourceRootNotDirectory { root, path } => write!(
                formatter,
                "source root {root:?} is not a directory at {}",
                path.display()
            ),
            Self::SourceRootEscapesProject { root, path } => write!(
                formatter,
                "source root {root:?} escapes the project directory through {}",
                path.display()
            ),
            Self::InvalidSourcePathEncoding { path } => write!(
                formatter,
                "source path is not valid UTF-8: {}",
                path.display()
            ),
            Self::DuplicatePhysicalSource { first, second } => write!(
                formatter,
                "physical source appears more than once: {} and {}",
                first.display(),
                second.display()
            ),
            Self::PhysicalSourceIdentityUnavailable { path } => write!(
                formatter,
                "physical source identity is unavailable for {}",
                path.display()
            ),
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "could not {} {}: {source}",
                operation.description(),
                path.display()
            ),
        }
    }
}

impl Error for ProjectLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl Utf8Subject {
    const fn description(self) -> &'static str {
        match self {
            Self::Manifest => "project manifest",
            Self::Source => "Koven source",
        }
    }
}

impl fmt::Display for SourceRootError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "path is empty",
            Self::Absolute => "path is absolute",
            Self::EmptySegment => "path contains an empty segment",
            Self::CurrentSegment => "path contains a current-directory segment",
            Self::ParentSegment => "path contains a parent-directory segment",
            Self::Backslash => "path contains a backslash",
            Self::TrailingSeparator => "path has a trailing separator",
            Self::WindowsPrefix => "path contains a Windows drive prefix",
        })
    }
}

impl IoOperation {
    const fn description(self) -> &'static str {
        match self {
            Self::ReadManifest => "read project manifest",
            Self::InspectRoot => "inspect source root",
            Self::CanonicalizeProject => "resolve project directory",
            Self::CanonicalizeRoot => "resolve source root",
            Self::ReadDirectory => "read source directory",
            Self::InspectEntry => "inspect source entry",
            Self::CanonicalizeSource => "resolve source file",
            Self::ReadSource => "read Koven source",
        }
    }
}
