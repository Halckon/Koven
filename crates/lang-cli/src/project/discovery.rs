use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
};

use super::{
    error::{IoOperation, ProjectLoadError, Utf8Subject},
    manifest::ProjectManifest,
    model::{ProjectSource, ProjectSourceRoot, ProjectSourceSet},
};

struct CanonicalRoot {
    identity: String,
    presentation_path: PathBuf,
    physical_path: PathBuf,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum PhysicalFileIdentity {
    #[cfg(unix)]
    Unix { device: u64, inode: u64 },
    #[cfg(windows)]
    Windows { volume: u32, index: u64 },
}

pub(super) fn discover_project(
    manifest_dir: &Path,
    manifest: ProjectManifest,
) -> Result<ProjectSourceSet, ProjectLoadError> {
    let project_directory = fs::canonicalize(manifest_dir).map_err(|error| {
        ProjectLoadError::io(
            IoOperation::CanonicalizeProject,
            manifest_dir.to_path_buf(),
            error,
        )
    })?;
    reject_logical_root_overlap(&manifest.source_roots)?;

    let mut roots = Vec::with_capacity(manifest.source_roots.len());
    for identity in manifest.source_roots {
        roots.push(canonicalize_root(
            manifest_dir,
            &project_directory,
            identity,
        )?);
    }
    reject_physical_root_overlap(&roots)?;

    let mut physical_sources = BTreeMap::new();
    let mut sources = Vec::new();
    for root in &roots {
        discover_root(
            root,
            &project_directory,
            &mut physical_sources,
            &mut sources,
        )?;
    }
    sources.sort_by(|left, right| {
        (left.root_identity(), left.logical_path())
            .cmp(&(right.root_identity(), right.logical_path()))
    });

    let roots = roots
        .into_iter()
        .map(|root| ProjectSourceRoot::new(root.identity, root.presentation_path))
        .collect();
    Ok(ProjectSourceSet::new(manifest.name, roots, sources))
}

fn canonicalize_root(
    manifest_dir: &Path,
    project_directory: &Path,
    identity: String,
) -> Result<CanonicalRoot, ProjectLoadError> {
    let mut current = manifest_dir.to_path_buf();
    for segment in identity.split('/') {
        current.push(segment);
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(ProjectLoadError::MissingSourceRoot {
                    root: identity,
                    path: current,
                });
            }
            Err(error) => {
                return Err(ProjectLoadError::io(
                    IoOperation::InspectRoot,
                    current,
                    error,
                ));
            }
        };
        if metadata.file_type().is_symlink() {
            return Err(ProjectLoadError::SymlinkSourceRoot {
                root: identity,
                path: current,
            });
        }
        if !metadata.is_dir() {
            return Err(ProjectLoadError::SourceRootNotDirectory {
                root: identity,
                path: current,
            });
        }
    }

    let presentation_path = manifest_dir.join(&identity);
    let physical_path = fs::canonicalize(&presentation_path).map_err(|error| {
        ProjectLoadError::io(
            IoOperation::CanonicalizeRoot,
            presentation_path.clone(),
            error,
        )
    })?;
    if !physical_path.starts_with(project_directory) {
        return Err(ProjectLoadError::SourceRootEscapesProject {
            root: identity,
            path: physical_path,
        });
    }
    Ok(CanonicalRoot {
        identity,
        presentation_path,
        physical_path,
    })
}

fn reject_logical_root_overlap(roots: &[String]) -> Result<(), ProjectLoadError> {
    for (index, left) in roots.iter().enumerate() {
        for right in &roots[index + 1..] {
            if is_logical_ancestor(left, right) {
                return Err(ProjectLoadError::OverlappingSourceRoots {
                    first: left.clone(),
                    second: right.clone(),
                });
            }
        }
    }
    Ok(())
}

fn reject_physical_root_overlap(roots: &[CanonicalRoot]) -> Result<(), ProjectLoadError> {
    for (index, left) in roots.iter().enumerate() {
        for right in &roots[index + 1..] {
            if left.physical_path.starts_with(&right.physical_path)
                || right.physical_path.starts_with(&left.physical_path)
            {
                return Err(ProjectLoadError::OverlappingSourceRoots {
                    first: left.identity.clone(),
                    second: right.identity.clone(),
                });
            }
        }
    }
    Ok(())
}

fn is_logical_ancestor(left: &str, right: &str) -> bool {
    right
        .strip_prefix(left)
        .is_some_and(|suffix| suffix.starts_with('/'))
}

fn discover_root(
    root: &CanonicalRoot,
    project_directory: &Path,
    physical_sources: &mut BTreeMap<PhysicalFileIdentity, PathBuf>,
    sources: &mut Vec<ProjectSource>,
) -> Result<(), ProjectLoadError> {
    let mut pending = vec![(root.presentation_path.clone(), Vec::<String>::new())];
    while let Some((directory, logical_parent)) = pending.pop() {
        let read_directory = fs::read_dir(&directory).map_err(|error| {
            ProjectLoadError::io(IoOperation::ReadDirectory, directory.clone(), error)
        })?;
        let mut entries = Vec::new();
        let mut entry_errors = Vec::new();
        for entry in read_directory {
            match entry {
                Ok(entry) => entries.push(entry),
                Err(error) => entry_errors.push(error),
            }
        }
        if let Some(error) = select_directory_error(entry_errors) {
            return Err(ProjectLoadError::io(
                IoOperation::ReadDirectory,
                directory,
                error,
            ));
        }
        entries.sort_by_key(fs::DirEntry::file_name);

        let mut child_directories = Vec::new();
        for entry in entries {
            let path = entry.path();
            let file_type = entry.file_type().map_err(|error| {
                ProjectLoadError::io(IoOperation::InspectEntry, path.clone(), error)
            })?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                let name = utf8_entry_name(&entry)?;
                let mut logical_path = logical_parent.clone();
                logical_path.push(name.to_owned());
                child_directories.push((path, logical_path));
                continue;
            }
            if !file_type.is_file() || path.extension() != Some(OsStr::new("ko")) {
                continue;
            }

            let name = utf8_entry_name(&entry)?;
            let mut logical_path = logical_parent.clone();
            logical_path.push(name.to_owned());
            let logical_path = logical_path.join("/");
            let canonical_path = fs::canonicalize(&path).map_err(|error| {
                ProjectLoadError::io(IoOperation::CanonicalizeSource, path.clone(), error)
            })?;
            if !canonical_path.starts_with(project_directory)
                || !canonical_path.starts_with(&root.physical_path)
            {
                return Err(ProjectLoadError::SourceRootEscapesProject {
                    root: root.identity.clone(),
                    path: canonical_path,
                });
            }
            let metadata = entry.metadata().map_err(|error| {
                ProjectLoadError::io(IoOperation::InspectEntry, path.clone(), error)
            })?;
            let physical_identity = physical_file_identity(&metadata, canonical_path)?;
            if let Some(first) = physical_sources.insert(physical_identity, path.clone()) {
                return Err(ProjectLoadError::DuplicatePhysicalSource {
                    first,
                    second: path,
                });
            }

            let bytes = fs::read(&path).map_err(|error| {
                ProjectLoadError::io(IoOperation::ReadSource, path.clone(), error)
            })?;
            let text = String::from_utf8(bytes).map_err(|_| ProjectLoadError::InvalidUtf8 {
                subject: Utf8Subject::Source,
                path: path.clone(),
            })?;
            sources.push(ProjectSource::new(
                root.identity.clone(),
                logical_path,
                path,
                text,
            ));
        }

        // LIFO stack 反向压入，使目录处理顺序保持 UTF-8/OsString 升序。
        child_directories.reverse();
        pending.extend(child_directories);
    }
    Ok(())
}

pub(super) fn select_directory_error(errors: Vec<io::Error>) -> Option<io::Error> {
    errors.into_iter().min_by_key(|error| {
        (
            format!("{:?}", error.kind()),
            error.raw_os_error().unwrap_or(i32::MAX),
            error.to_string(),
        )
    })
}

fn utf8_entry_name(entry: &fs::DirEntry) -> Result<String, ProjectLoadError> {
    utf8_path_segment(entry.file_name(), entry.path())
}

pub(super) fn utf8_path_segment(
    segment: OsString,
    path: PathBuf,
) -> Result<String, ProjectLoadError> {
    segment
        .into_string()
        .map_err(|_| ProjectLoadError::InvalidSourcePathEncoding { path })
}

#[cfg(unix)]
fn physical_file_identity(
    metadata: &fs::Metadata,
    _canonical_path: PathBuf,
) -> Result<PhysicalFileIdentity, ProjectLoadError> {
    use std::os::unix::fs::MetadataExt;
    Ok(PhysicalFileIdentity::Unix {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(windows)]
fn physical_file_identity(
    metadata: &fs::Metadata,
    canonical_path: PathBuf,
) -> Result<PhysicalFileIdentity, ProjectLoadError> {
    use std::os::windows::fs::MetadataExt;
    if let (Some(volume), Some(index)) = (metadata.volume_serial_number(), metadata.file_index()) {
        return Ok(PhysicalFileIdentity::Windows { volume, index });
    }
    Err(ProjectLoadError::PhysicalSourceIdentityUnavailable {
        path: canonical_path,
    })
}

#[cfg(not(any(unix, windows)))]
fn physical_file_identity(
    _metadata: &fs::Metadata,
    canonical_path: PathBuf,
) -> Result<PhysicalFileIdentity, ProjectLoadError> {
    Err(ProjectLoadError::PhysicalSourceIdentityUnavailable {
        path: canonical_path,
    })
}
