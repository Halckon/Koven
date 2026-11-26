//! 显式 `--workspace-manifest` 模式下的最小 `project.toml` 发现与 ADR-0021 source-set 构造。
//! 默认无参数启动的 `lang-lsp` 不调用本模块，精确保留 ADR-0021 不读磁盘的既有合同。

use std::{
    collections::BTreeSet,
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

use lsp_types::{InitializeParams, Uri};
use serde_json::json;

use crate::source_set::SourceSetConfig;

#[derive(Debug)]
pub(crate) struct WorkspaceManifestError(String);

impl fmt::Display for WorkspaceManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for WorkspaceManifestError {}

/// 当显式启用 `--workspace-manifest` 且未提供 `koven.sourceSet` 时，
/// 从 LSP `workspace_folders` 或 `root_uri` 查找 `project.toml` 并加载为 `SourceSetConfig`。
/// 目录无 `project.toml` 时返回 `Ok(None)`，由调用方回退到单文档模式。
pub(crate) fn discover_source_set(
    params: &InitializeParams,
) -> Result<Option<SourceSetConfig>, WorkspaceManifestError> {
    let Some(workspace_root) = workspace_root_path(params) else {
        return Ok(None);
    };
    load_from_workspace_root(&workspace_root)
}

pub(crate) fn load_from_workspace_root(
    workspace_root: &Path,
) -> Result<Option<SourceSetConfig>, WorkspaceManifestError> {
    let manifest_path = workspace_root.join("project.toml");
    if !manifest_path.is_file() {
        return Ok(None);
    }
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|e| WorkspaceManifestError(format!("failed to read project.toml: {e}")))?;
    let source_roots = parse_minimal_manifest(&manifest_text)?;
    let canonical_workspace = fs::canonicalize(workspace_root)
        .map_err(|e| WorkspaceManifestError(format!("failed to canonicalize workspace: {e}")))?;

    let mut roots_json = Vec::with_capacity(source_roots.len());
    let mut sources_json = Vec::new();
    let mut seen_roots = BTreeSet::new();

    for root_rel in source_roots {
        if !seen_roots.insert(root_rel.clone()) {
            return Err(WorkspaceManifestError(format!(
                "duplicate source root in project.toml: {root_rel:?}"
            )));
        }
        let root_dir = canonical_workspace.join(&root_rel);
        let canonical_root = fs::canonicalize(&root_dir).map_err(|e| {
            WorkspaceManifestError(format!("failed to resolve source root {root_rel:?}: {e}"))
        })?;
        if !canonical_root.starts_with(&canonical_workspace) {
            return Err(WorkspaceManifestError(format!(
                "source root {root_rel:?} escapes workspace directory"
            )));
        }
        roots_json.push(json!(root_rel));
        let mut ko_files = Vec::new();
        collect_ko_files(&canonical_root, &canonical_root, &mut ko_files)?;
        ko_files.sort();
        for (logical_path, abs_path) in ko_files {
            let text = fs::read_to_string(&abs_path).map_err(|e| {
                WorkspaceManifestError(format!("failed to read {}: {e}", abs_path.display()))
            })?;
            let uri = path_to_file_uri(&abs_path)?;
            sources_json.push(json!({
                "root": root_rel,
                "logicalPath": logical_path,
                "uri": uri,
                "text": text,
            }));
        }
    }

    let payload = json!({
        "schema": "koven.lsp.source-set",
        "version": 1,
        "roots": roots_json,
        "sources": sources_json,
    });
    SourceSetConfig::parse_value(&payload)
        .map(Some)
        .map_err(|e| WorkspaceManifestError(e.to_string()))
}

fn workspace_root_path(params: &InitializeParams) -> Option<PathBuf> {
    if let Some(folders) = &params.workspace_folders {
        for folder in folders {
            if let Some(path) = file_uri_to_path(&folder.uri) {
                return Some(path);
            }
        }
    }
    #[allow(deprecated)]
    if let Some(root_uri) = &params.root_uri
        && let Some(path) = file_uri_to_path(root_uri)
    {
        return Some(path);
    }
    None
}

fn parse_minimal_manifest(text: &str) -> Result<Vec<String>, WorkspaceManifestError> {
    let mut schema: Option<String> = None;
    let mut version: Option<u64> = None;
    let mut source_roots: Option<Vec<String>> = None;
    let mut section = String::new();

    for raw in text.lines() {
        let line = strip_toml_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_owned();
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let val = val.trim();
        if section.is_empty() {
            if key == "schema" {
                schema = Some(parse_toml_string(val)?);
            } else if key == "version" {
                version = val.parse::<u64>().ok();
            }
        } else if section == "project" && key == "source-roots" {
            source_roots = Some(parse_toml_string_array(val)?);
        }
    }

    if schema.as_deref() != Some("koven.project") {
        return Err(WorkspaceManifestError(
            "project.toml schema must be \"koven.project\"".to_owned(),
        ));
    }
    if version != Some(1) {
        return Err(WorkspaceManifestError(
            "project.toml version must be 1".to_owned(),
        ));
    }
    let roots = source_roots.ok_or_else(|| {
        WorkspaceManifestError("project.toml missing [project] source-roots".to_owned())
    })?;
    if roots.is_empty() {
        return Err(WorkspaceManifestError(
            "project.toml source-roots must not be empty".to_owned(),
        ));
    }
    Ok(roots)
}

fn strip_toml_comment(line: &str) -> &str {
    let mut in_quotes = false;
    let mut escape = false;
    for (idx, ch) in line.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_quotes => escape = true,
            '"' => in_quotes = !in_quotes,
            '#' if !in_quotes => return &line[..idx],
            _ => {}
        }
    }
    line
}

fn parse_toml_string(raw: &str) -> Result<String, WorkspaceManifestError> {
    serde_json::from_str::<String>(raw)
        .map_err(|e| WorkspaceManifestError(format!("invalid TOML string {raw:?}: {e}")))
}

fn parse_toml_string_array(raw: &str) -> Result<Vec<String>, WorkspaceManifestError> {
    serde_json::from_str::<Vec<String>>(raw)
        .map_err(|e| WorkspaceManifestError(format!("invalid TOML string array {raw:?}: {e}")))
}

fn collect_ko_files(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), WorkspaceManifestError> {
    let mut entries = fs::read_dir(dir)
        .map_err(|e| WorkspaceManifestError(format!("failed to read dir {}: {e}", dir.display())))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| WorkspaceManifestError(format!("failed to list dir {}: {e}", dir.display())))?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(|e| {
            WorkspaceManifestError(format!("failed to stat {}: {e}", path.display()))
        })?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            collect_ko_files(root, &path, out)?;
        } else if meta.is_file() && path.extension().and_then(|s| s.to_str()) == Some("ko") {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| WorkspaceManifestError(format!("strip_prefix failed: {e}")))?;
            let logical = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.push((logical, path));
        }
    }
    Ok(())
}

pub(crate) fn file_uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let raw = uri.as_str();
    let rest = raw.strip_prefix("file://")?;
    let path_part = rest.strip_prefix("localhost").unwrap_or(rest);
    if !path_part.starts_with('/') {
        return None;
    }
    let decoded = percent_decode(path_part)?;
    Some(PathBuf::from(decoded))
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = hex_val(bytes[i + 1])?;
            let lo = hex_val(bytes[i + 2])?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn path_to_file_uri(path: &Path) -> Result<String, WorkspaceManifestError> {
    let text = path.to_str().ok_or_else(|| {
        WorkspaceManifestError(format!("non-UTF-8 path: {}", path.display()))
    })?;
    let mut encoded = String::from("file://");
    for byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            encoded.push(*byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("koven-lsp-manifest-{label}-{nanos}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn loads_multifile_project_from_workspace_manifest() {
        let dir = temp_dir("multi");
        fs::create_dir_all(dir.join("src/demo")).expect("src dir");
        fs::write(
            dir.join("project.toml"),
            "schema = \"koven.project\"\nversion = 1\n\n[project]\nname = \"demo\"\nsource-roots = [\"src\"]\n",
        )
        .expect("manifest");
        fs::write(
            dir.join("src/demo/math.ko"),
            "package demo\npublic fun helper(): Int = 42\n",
        )
        .expect("math");
        fs::write(
            dir.join("src/demo/main.ko"),
            "package demo\npublic fun main(): Int = helper()\n",
        )
        .expect("main");

        let config = load_from_workspace_root(&dir)
            .expect("load")
            .expect("some config");
        assert_eq!(config.sources().len(), 2);
        assert_eq!(config.sources()[0].logical_path(), "demo/main.ko");
        assert_eq!(config.sources()[1].logical_path(), "demo/math.ko");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn returns_none_when_project_toml_is_absent() {
        let dir = temp_dir("none");
        let config = load_from_workspace_root(&dir).expect("load");
        assert!(config.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn decodes_percent_encoded_file_uri_with_spaces_and_unicode() {
        let uri: Uri = "file:///tmp/demo%20%E9%A1%B9%E7%9B%AE".parse().expect("uri");
        assert_eq!(
            file_uri_to_path(&uri),
            Some(PathBuf::from("/tmp/demo 项目"))
        );
    }
}
