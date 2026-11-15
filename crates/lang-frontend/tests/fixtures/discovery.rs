//! Deterministic fixture discovery, relative paths and diagnostic sidecars.
use super::*;

pub(super) fn discover_fixtures(root: &Path) -> Result<Vec<FixtureCase>, SuiteError> {
    discover_files(root, &["ko"])
}

pub(super) fn discover_files(
    root: &Path,
    allowed_extensions: &[&str],
) -> Result<Vec<FixtureCase>, SuiteError> {
    let metadata = fs::symlink_metadata(root).map_err(|error| SuiteError::RootIo(error.kind()))?;
    if metadata.file_type().is_symlink() {
        return Err(SuiteError::RootSymlink);
    }
    if !metadata.is_dir() {
        return Err(SuiteError::RootNotDirectory);
    }

    let mut cases = Vec::new();
    let mut issues = Vec::new();
    collect_entries(
        root,
        Path::new(""),
        allowed_extensions,
        &mut cases,
        &mut issues,
    );
    if !issues.is_empty() {
        issues.sort_by(|left, right| issue_sort_key(left).cmp(&issue_sort_key(right)));
        return Err(SuiteError::InvalidEntries(issues));
    }

    cases.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    if cases.is_empty() {
        return Err(SuiteError::NoFixtures);
    }

    Ok(cases)
}

pub(super) fn discover_lexer_fail_cases(root: &Path) -> Result<Vec<LexerFailCase>, SuiteError> {
    let files = discover_files(root, &["ko", "diag"])?;
    let mut sources = BTreeMap::new();
    let mut sidecars = BTreeMap::new();

    for file in files {
        if let Some(stem) = file.relative_path.strip_suffix(".ko") {
            sources.insert(stem.to_owned(), file);
        } else if let Some(stem) = file.relative_path.strip_suffix(".diag") {
            sidecars.insert(stem.to_owned(), file);
        }
    }

    let mut cases = Vec::new();
    let mut issues = Vec::new();
    for (stem, source) in sources {
        let Some(sidecar) = sidecars.remove(&stem) else {
            issues.push(DiscoveryIssue::MissingSidecar {
                relative_path: source.relative_path,
            });
            continue;
        };
        cases.push(LexerFailCase {
            relative_path: source.relative_path,
            source_path: source.disk_path,
            sidecar_path: sidecar.disk_path,
        });
    }
    for sidecar in sidecars.into_values() {
        issues.push(DiscoveryIssue::OrphanSidecar {
            relative_path: sidecar.relative_path,
        });
    }

    if !issues.is_empty() {
        issues.sort_by(|left, right| issue_sort_key(left).cmp(&issue_sort_key(right)));
        return Err(SuiteError::InvalidEntries(issues));
    }
    if cases.is_empty() {
        return Err(SuiteError::NoFixtures);
    }
    Ok(cases)
}

pub(super) fn collect_entries(
    root: &Path,
    relative_dir: &Path,
    allowed_extensions: &[&str],
    cases: &mut Vec<FixtureCase>,
    issues: &mut Vec<DiscoveryIssue>,
) {
    let disk_dir = root.join(relative_dir);
    let directory_key = match checked_relative_path(relative_dir) {
        Ok(path) => path,
        Err(issue) => {
            issues.push(issue);
            return;
        }
    };
    let entries = match fs::read_dir(&disk_dir) {
        Ok(entries) => entries,
        Err(error) => {
            issues.push(DiscoveryIssue::Io {
                relative_path: directory_key,
                kind: error.kind(),
            });
            return;
        }
    };

    let mut entries = entries.collect::<Vec<_>>();
    entries.sort_by(|left, right| match (left, right) {
        (Ok(left), Ok(right)) => left.file_name().cmp(&right.file_name()),
        (Err(_), Ok(_)) => std::cmp::Ordering::Less,
        (Ok(_), Err(_)) => std::cmp::Ordering::Greater,
        (Err(left), Err(right)) => format!("{:?}", left.kind()).cmp(&format!("{:?}", right.kind())),
    });

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                issues.push(DiscoveryIssue::Io {
                    relative_path: directory_key.clone(),
                    kind: error.kind(),
                });
                continue;
            }
        };
        let relative_path = relative_dir.join(entry.file_name());
        let normalized_path = match checked_relative_path(&relative_path) {
            Ok(path) => path,
            Err(issue) => {
                issues.push(issue);
                continue;
            }
        };
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                issues.push(DiscoveryIssue::Io {
                    relative_path: normalized_path,
                    kind: error.kind(),
                });
                continue;
            }
        };

        if file_type.is_symlink() {
            issues.push(DiscoveryIssue::Symlink {
                relative_path: normalized_path,
            });
        } else if file_type.is_dir() {
            collect_entries(root, &relative_path, allowed_extensions, cases, issues);
        } else if file_type.is_file() {
            let extension = relative_path
                .extension()
                .and_then(|extension| extension.to_str());
            if !allowed_extensions
                .iter()
                .any(|allowed| Some(*allowed) == extension)
            {
                issues.push(DiscoveryIssue::UnknownExtension {
                    relative_path: normalized_path,
                });
                continue;
            }
            cases.push(FixtureCase {
                relative_path: normalized_path,
                disk_path: entry.path(),
            });
        } else {
            issues.push(DiscoveryIssue::UnsupportedEntryType {
                relative_path: normalized_path,
            });
        }
    }
}

pub(super) fn checked_relative_path(path: &Path) -> Result<String, DiscoveryIssue> {
    let mut components = Vec::new();
    for component in path.components() {
        let Component::Normal(component) = component else {
            return Err(DiscoveryIssue::NonUtf8RelativePath);
        };
        components.push(
            component
                .to_str()
                .ok_or(DiscoveryIssue::NonUtf8RelativePath)?,
        );
    }
    Ok(components.join("/"))
}

pub(super) fn issue_sort_key(issue: &DiscoveryIssue) -> (u8, &str, String) {
    match issue {
        DiscoveryIssue::NonUtf8RelativePath => (0, "", String::new()),
        DiscoveryIssue::Io {
            relative_path,
            kind,
        } => (1, relative_path, format!("{kind:?}")),
        DiscoveryIssue::Symlink { relative_path } => (2, relative_path, String::new()),
        DiscoveryIssue::UnknownExtension { relative_path } => (3, relative_path, String::new()),
        DiscoveryIssue::MissingSidecar { relative_path } => (4, relative_path, String::new()),
        DiscoveryIssue::OrphanSidecar { relative_path } => (5, relative_path, String::new()),
        DiscoveryIssue::UnsupportedEntryType { relative_path } => (6, relative_path, String::new()),
    }
}

pub(super) fn parse_sidecar(
    sources: &SourceMap,
    source_id: SourceId,
    text: &str,
    catalog: &DiagnosticCodeCatalog,
    span_policy: SidecarSpanPolicy,
) -> Result<Vec<ExpectedDiagnostic>, SidecarError> {
    if text.is_empty() {
        return Err(SidecarError::Empty);
    }

    let mut diagnostics = Vec::new();
    for (line_index, raw_line) in text.split_inclusive('\n').enumerate() {
        let line_number = line_index + 1;
        let line = if let Some(line) = raw_line.strip_suffix('\n') {
            line.strip_suffix('\r').unwrap_or(line)
        } else {
            raw_line
        };
        if line.contains('\r') {
            return Err(SidecarError::BareCarriageReturn { line: line_number });
        }
        if line.is_empty() {
            return Err(SidecarError::EmptyLine { line: line_number });
        }

        let columns = line.split('\t').collect::<Vec<_>>();
        if columns.len() != 3 {
            return Err(SidecarError::WrongColumnCount { line: line_number });
        }
        if catalog.resolve(columns[0]).is_err() {
            return Err(SidecarError::InvalidCode { line: line_number });
        }
        let start = parse_decimal_offset(columns[1])
            .ok_or(SidecarError::InvalidOffset { line: line_number })?;
        let end = parse_decimal_offset(columns[2])
            .ok_or(SidecarError::InvalidOffset { line: line_number })?;
        let span = sources
            .span(source_id, start, end)
            .map_err(|_| SidecarError::InvalidSpan { line: line_number })?;
        if span.is_empty() && !span_policy.permits_empty(columns[0]) {
            return Err(SidecarError::InvalidSpan { line: line_number });
        }
        diagnostics.push(ExpectedDiagnostic {
            code: columns[0].to_owned(),
            start,
            end,
        });
    }

    if diagnostics.is_empty() {
        return Err(SidecarError::Empty);
    }
    Ok(diagnostics)
}

pub(super) fn parse_decimal_offset(raw: &str) -> Option<usize> {
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok()
}
