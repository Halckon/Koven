use std::path::Path;

use toml::{Table, Value};

use super::error::{ProjectLoadError, SourceRootError};

#[derive(Debug)]
pub(super) struct ProjectManifest {
    pub(super) name: String,
    pub(super) source_roots: Vec<String>,
}

pub(super) fn parse_manifest(path: &Path, text: &str) -> Result<ProjectManifest, ProjectLoadError> {
    let mut table = text
        .parse::<Table>()
        .map_err(|error| ProjectLoadError::InvalidToml {
            path: path.to_path_buf(),
            detail: error.to_string(),
        })?;
    reject_unknown_fields("manifest", &table, &["schema", "version", "project"])?;

    let schema = take_string(&mut table, "schema", "schema")?;
    if schema != "koven.project" {
        return Err(ProjectLoadError::UnsupportedSchema { found: schema });
    }
    let version = take_integer(&mut table, "version", "version")?;
    if version != 1 {
        return Err(ProjectLoadError::UnsupportedVersion { found: version });
    }
    let mut project = take_table(&mut table, "project", "project")?;
    reject_unknown_fields("project", &project, &["name", "source-roots"])?;

    let name = take_string(&mut project, "name", "project.name")?;
    if !valid_project_name(&name) {
        return Err(ProjectLoadError::InvalidProjectName { name });
    }
    let roots = take_array(&mut project, "source-roots", "project.source-roots")?;
    if roots.is_empty() {
        return Err(ProjectLoadError::EmptySourceRoots);
    }

    let mut source_roots = Vec::with_capacity(roots.len());
    for value in roots {
        let Value::String(root) = value else {
            return Err(ProjectLoadError::InvalidFieldType {
                field: "project.source-roots[]",
                expected: "a string",
            });
        };
        source_roots.push(root);
    }
    source_roots.sort();
    for root in &source_roots {
        validate_source_root(root)?;
    }
    if let Some(duplicate) = source_roots.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(ProjectLoadError::DuplicateSourceRoot {
            root: duplicate[0].clone(),
        });
    }

    Ok(ProjectManifest { name, source_roots })
}

fn reject_unknown_fields(
    section: &'static str,
    table: &Table,
    expected: &[&str],
) -> Result<(), ProjectLoadError> {
    if let Some(field) = table
        .keys()
        .find(|field| !expected.contains(&field.as_str()))
    {
        return Err(ProjectLoadError::UnknownField {
            section,
            field: field.clone(),
        });
    }
    Ok(())
}

fn take_string(
    table: &mut Table,
    key: &'static str,
    field: &'static str,
) -> Result<String, ProjectLoadError> {
    match table
        .remove(key)
        .ok_or(ProjectLoadError::MissingField { field })?
    {
        Value::String(value) => Ok(value),
        _ => Err(ProjectLoadError::InvalidFieldType {
            field,
            expected: "a string",
        }),
    }
}

fn take_integer(
    table: &mut Table,
    key: &'static str,
    field: &'static str,
) -> Result<i64, ProjectLoadError> {
    match table
        .remove(key)
        .ok_or(ProjectLoadError::MissingField { field })?
    {
        Value::Integer(value) => Ok(value),
        _ => Err(ProjectLoadError::InvalidFieldType {
            field,
            expected: "an integer",
        }),
    }
}

fn take_table(
    table: &mut Table,
    key: &'static str,
    field: &'static str,
) -> Result<Table, ProjectLoadError> {
    match table
        .remove(key)
        .ok_or(ProjectLoadError::MissingField { field })?
    {
        Value::Table(value) => Ok(value),
        _ => Err(ProjectLoadError::InvalidFieldType {
            field,
            expected: "a table",
        }),
    }
}

fn take_array(
    table: &mut Table,
    key: &'static str,
    field: &'static str,
) -> Result<Vec<Value>, ProjectLoadError> {
    match table
        .remove(key)
        .ok_or(ProjectLoadError::MissingField { field })?
    {
        Value::Array(value) => Ok(value),
        _ => Err(ProjectLoadError::InvalidFieldType {
            field,
            expected: "an array",
        }),
    }
}

fn valid_project_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn validate_source_root(root: &str) -> Result<(), ProjectLoadError> {
    let invalid = |reason| ProjectLoadError::InvalidSourceRoot {
        root: root.to_owned(),
        reason,
    };
    if root.is_empty() {
        return Err(invalid(SourceRootError::Empty));
    }
    if root.starts_with('/') || has_windows_drive_prefix(root) {
        return Err(invalid(SourceRootError::Absolute));
    }
    if root.contains('\\') {
        return Err(invalid(SourceRootError::Backslash));
    }
    if root.ends_with('/') {
        return Err(invalid(SourceRootError::TrailingSeparator));
    }
    for segment in root.split('/') {
        match segment {
            "" => return Err(invalid(SourceRootError::EmptySegment)),
            "." => return Err(invalid(SourceRootError::CurrentSegment)),
            ".." => return Err(invalid(SourceRootError::ParentSegment)),
            segment if has_windows_drive_prefix(segment) => {
                return Err(invalid(SourceRootError::WindowsPrefix));
            }
            _ => {}
        }
    }
    Ok(())
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}
