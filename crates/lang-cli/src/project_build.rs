//! Project 宿主输入、路径保护与共同 Unit 编排的薄适配。

use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use lang_frontend::{
    analysis::{UnitSourceDescriptor, analyze_unit_names},
    source::SourceMap,
    type_checking::standard_environments,
};

use crate::{
    project::{ProjectLoadError, ProjectSourceSet, load_project_source_set},
    unit_build::{UnitBuildError, UnitEntrySelection, emit_unit_object},
};

/// Project 专有的宿主失败；共同阶段只包装唯一 Unit 错误。
#[derive(Debug)]
pub(crate) enum ProjectBuildError {
    Project(ProjectLoadError),
    Unit(UnitBuildError),
    InvalidPaths(String),
    InspectPath { path: PathBuf, kind: io::ErrorKind },
}

impl fmt::Display for ProjectBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project(error) => write!(formatter, "project load failed: {error}"),
            Self::Unit(UnitBuildError::ProjectEntry(error)) => {
                write!(formatter, "project {error}")
            }
            Self::Unit(error) => error.fmt(formatter),
            Self::InvalidPaths(detail) => formatter.write_str(detail),
            Self::InspectPath { path, kind } => write!(
                formatter,
                "cannot inspect project output path {}: {kind}",
                path.display()
            ),
        }
    }
}

impl Error for ProjectBuildError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Unit(error) => Some(error),
            _ => None,
        }
    }
}

impl From<UnitBuildError> for ProjectBuildError {
    fn from(error: UnitBuildError) -> Self {
        Self::Unit(error)
    }
}

/// 加载显式 project、完成 validated unit frontend，并生成一个 project-owned object。
pub(crate) fn emit_project_object(
    manifest: &Path,
    selector: &[String],
    object: &Path,
    protected_outputs: &[&Path],
) -> Result<(), ProjectBuildError> {
    let project = load_project_source_set(manifest).map_err(ProjectBuildError::Project)?;
    validate_project_paths(manifest, &project, protected_outputs)?;
    let mut sources = SourceMap::new();
    let mut descriptors = Vec::with_capacity(project.sources().len());
    for source in project.sources() {
        let source_name = format!("{}/{}", source.root_identity(), source.logical_path());
        let source_id = sources
            .add_source(source_name, source.text())
            .map_err(UnitBuildError::Source)?;
        descriptors.push(UnitSourceDescriptor::new(
            source.root_identity(),
            source.logical_path(),
            source_id,
        ));
    }
    let (name_environment, mut type_environment) = standard_environments();
    crate::standard_sources::append_standard_sources(
        &mut sources,
        &mut descriptors,
        &mut type_environment,
    )?;
    let snapshot =
        analyze_unit_names(sources, descriptors, name_environment).map_err(UnitBuildError::from)?;
    emit_unit_object(
        &snapshot,
        &type_environment,
        UnitEntrySelection::Project(selector),
        object,
    )
    .map_err(ProjectBuildError::Unit)
}

fn validate_project_paths(
    manifest: &Path,
    project: &ProjectSourceSet,
    outputs: &[&Path],
) -> Result<(), ProjectBuildError> {
    let manifest = fs::canonicalize(manifest).map_err(|error| ProjectBuildError::InspectPath {
        path: manifest.to_path_buf(),
        kind: error.kind(),
    })?;
    let sources = project
        .sources()
        .iter()
        .map(|source| {
            fs::canonicalize(source.presentation_path()).map_err(|error| {
                ProjectBuildError::InspectPath {
                    path: source.presentation_path().to_path_buf(),
                    kind: error.kind(),
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let roots = project
        .roots()
        .iter()
        .map(|root| {
            fs::canonicalize(root.presentation_path()).map_err(|error| {
                ProjectBuildError::InspectPath {
                    path: root.presentation_path().to_path_buf(),
                    kind: error.kind(),
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    for output in outputs {
        if let Ok(output) = fs::canonicalize(output)
            && (output == manifest || sources.contains(&output))
        {
            return Err(ProjectBuildError::InvalidPaths(format!(
                "project output overlaps manifest or source: {}",
                output.display()
            )));
        }
        if output
            .extension()
            .is_some_and(|extension| extension == "ko")
        {
            let parent = output.parent().unwrap_or_else(|| Path::new("."));
            let canonical_parent =
                fs::canonicalize(parent).map_err(|error| ProjectBuildError::InspectPath {
                    path: parent.to_path_buf(),
                    kind: error.kind(),
                })?;
            if roots.iter().any(|root| canonical_parent.starts_with(root)) {
                return Err(ProjectBuildError::InvalidPaths(format!(
                    "project output would enter the next source discovery: {}",
                    output.display()
                )));
            }
        }
    }
    Ok(())
}
