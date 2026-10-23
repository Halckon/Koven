//! Export the existing project loader's immutable base without invoking language analysis.
use std::{ffi::OsString, fs, path::Path};

use fluent_uri::enc::{EString, encoder::PathEncoder};
use lang_frontend::name_resolution::validate_logical_path;
use serde_json::json;

use crate::{format::CommandOutput, project::load_project_source_set};

const USAGE: &str = "usage: kovenc source-set --project <project.toml>\n";

/// Publish one complete ADR-0021 object only after all loader and URI conversions succeed.
pub(super) fn execute(arguments: &[OsString]) -> CommandOutput {
    let manifest = match arguments {
        [flag, path] if flag == "--project" && !path.to_string_lossy().starts_with('-') => {
            Path::new(path)
        }
        _ => {
            return CommandOutput::error(format!(
                "error: expected explicit project manifest\n{USAGE}"
            ));
        }
    };
    match export(manifest) {
        Ok(bytes) => CommandOutput::success(bytes),
        Err(error) => CommandOutput {
            status: 1,
            stdout: Vec::new(),
            stderr: format!("error: {error}\n").into_bytes(),
        },
    }
}

fn export(manifest: &Path) -> Result<Vec<u8>, String> {
    let project = load_project_source_set(manifest).map_err(|error| error.to_string())?;
    let roots: Vec<_> = project.roots().iter().map(|root| root.identity()).collect();
    let mut sources = Vec::with_capacity(project.sources().len());
    for source in project.sources() {
        validate_logical_path(source.logical_path()).map_err(|error| error.to_string())?;
        // ADR-0021 further excludes backslashes, including POSIX filename characters.
        if source.logical_path().contains('\\') {
            return Err(format!(
                "source-set logical path contains a backslash: {}",
                source.logical_path()
            ));
        }
        sources.push(json!({
            "root": source.root_identity(),
            "logicalPath": source.logical_path(),
            "uri": file_uri(source.presentation_path())?,
            "text": source.text(),
        }));
    }
    let snapshot = json!({
        "schema": "koven.lsp.source-set", "version": 1, "roots": roots, "sources": sources,
    });
    let mut output = serde_json::to_vec(&snapshot).map_err(|error| error.to_string())?;
    output.push(b'\n');
    Ok(output)
}

/// Physical paths are presentation locators; logical source identity remains loader-owned.
fn file_uri(path: &Path) -> Result<String, String> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("cannot resolve source URI for {}: {error}", path.display()))?;
    let text = canonical
        .to_str()
        .ok_or_else(|| "source URI path must be valid UTF-8".to_owned())?;
    // The supported macOS/Linux hosts use absolute POSIX paths. Do not emit a malformed
    // file URI on an unverified platform with drive-letter or UNC path semantics.
    if !text.starts_with('/') {
        return Err("source-set export requires an absolute POSIX file path".to_owned());
    }
    let mut encoded = EString::<PathEncoder>::new();
    encoded.push(text);
    Ok(format!("file://{encoded}"))
}
