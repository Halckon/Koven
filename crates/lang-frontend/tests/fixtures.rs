//! SPEC-0005 的语言 fixture 发现、执行与失败保护。

mod support;

#[path = "support/fixture_codes.rs"]
mod fixture_codes;

use std::{
    env, fs, io,
    path::{Component, Path, PathBuf},
};

use fixture_codes::phase0_fixture_code;
use lang_frontend::{
    ast::AstFile,
    diagnostic::{Diagnostic, Severity},
    source::SourceMap,
};

const FIXTURE_MESSAGE: &str = "Phase 0 fixture wiring";

struct FixtureCase {
    relative_path: String,
    disk_path: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
struct CaseOutcome {
    relative_path: String,
    result: Result<SourcePassEvidence, CaseFailure>,
}

#[derive(Debug, PartialEq, Eq)]
struct SourcePassEvidence {
    byte_len: usize,
    ast_node_count: usize,
    diagnostic_code: String,
}

#[derive(Debug, PartialEq, Eq)]
enum CaseFailure {
    Read(io::ErrorKind),
    InvalidUtf8Content,
    SourceModel,
    AstModel,
    DiagnosticModel,
    WiringInvariant,
}

#[derive(Debug, PartialEq, Eq)]
enum SuiteError {
    RootIo(io::ErrorKind),
    RootSymlink,
    RootNotDirectory,
    InvalidEntries(Vec<DiscoveryIssue>),
    NoFixtures,
}

#[derive(Debug, PartialEq, Eq)]
enum DiscoveryIssue {
    NonUtf8RelativePath,
    Io {
        relative_path: String,
        kind: io::ErrorKind,
    },
    Symlink {
        relative_path: String,
    },
    UnknownExtension {
        relative_path: String,
    },
    UnsupportedEntryType {
        relative_path: String,
    },
}

#[derive(Debug)]
struct FixtureExpression {
    byte_len: usize,
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase0/source-pass")
}

fn discover_fixtures(root: &Path) -> Result<Vec<FixtureCase>, SuiteError> {
    let metadata = fs::symlink_metadata(root).map_err(|error| SuiteError::RootIo(error.kind()))?;
    if metadata.file_type().is_symlink() {
        return Err(SuiteError::RootSymlink);
    }
    if !metadata.is_dir() {
        return Err(SuiteError::RootNotDirectory);
    }

    let mut cases = Vec::new();
    let mut issues = Vec::new();
    collect_entries(root, Path::new(""), &mut cases, &mut issues);
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

fn collect_entries(
    root: &Path,
    relative_dir: &Path,
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
            collect_entries(root, &relative_path, cases, issues);
        } else if file_type.is_file() {
            if relative_path
                .extension()
                .and_then(|extension| extension.to_str())
                != Some("ko")
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

fn checked_relative_path(path: &Path) -> Result<String, DiscoveryIssue> {
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

fn issue_sort_key(issue: &DiscoveryIssue) -> (u8, &str, String) {
    match issue {
        DiscoveryIssue::NonUtf8RelativePath => (0, "", String::new()),
        DiscoveryIssue::Io {
            relative_path,
            kind,
        } => (1, relative_path, format!("{kind:?}")),
        DiscoveryIssue::Symlink { relative_path } => (2, relative_path, String::new()),
        DiscoveryIssue::UnknownExtension { relative_path } => (3, relative_path, String::new()),
        DiscoveryIssue::UnsupportedEntryType { relative_path } => (4, relative_path, String::new()),
    }
}

fn run_suite(root: &Path) -> Result<Vec<CaseOutcome>, SuiteError> {
    let cases = discover_fixtures(root)?;
    Ok(cases
        .iter()
        .map(|case| CaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_case(case),
        })
        .collect())
}

fn run_case(case: &FixtureCase) -> Result<SourcePassEvidence, CaseFailure> {
    let bytes = fs::read(&case.disk_path).map_err(|error| CaseFailure::Read(error.kind()))?;
    let text = String::from_utf8(bytes).map_err(|_| CaseFailure::InvalidUtf8Content)?;
    let byte_len = text.len();
    let expected_text = text.clone();
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source(case.relative_path.clone(), text)
        .map_err(|_| CaseFailure::SourceModel)?;
    let full_span = sources
        .span(source_id, 0, byte_len)
        .map_err(|_| CaseFailure::SourceModel)?;
    if sources
        .source_name(source_id)
        .map_err(|_| CaseFailure::SourceModel)?
        != case.relative_path
        || sources
            .slice(full_span)
            .map_err(|_| CaseFailure::SourceModel)?
            != expected_text
    {
        return Err(CaseFailure::WiringInvariant);
    }

    let mut ast = AstFile::<(), (), FixtureExpression, ()>::new(source_id);
    let expression_id = ast
        .add_expression(full_span, FixtureExpression { byte_len })
        .map_err(|_| CaseFailure::AstModel)?;
    let expression = ast
        .expressions()
        .get(expression_id)
        .map_err(|_| CaseFailure::AstModel)?;
    if expression.span() != full_span || expression.payload().byte_len != byte_len {
        return Err(CaseFailure::WiringInvariant);
    }

    let code = phase0_fixture_code();
    let diagnostic = Diagnostic::new(
        &sources,
        Severity::Warning,
        code,
        FIXTURE_MESSAGE,
        full_span,
    )
    .map_err(|_| CaseFailure::DiagnosticModel)?;
    if diagnostic.primary_span() != full_span
        || diagnostic.severity() != Severity::Warning
        || diagnostic.message() != FIXTURE_MESSAGE
        || diagnostic.code() != code
    {
        return Err(CaseFailure::WiringInvariant);
    }

    Ok(SourcePassEvidence {
        byte_len,
        ast_node_count: ast.expressions().len(),
        diagnostic_code: code.to_string(),
    })
}

fn stable_report(outcomes: &[CaseOutcome]) -> String {
    let mut lines = Vec::with_capacity(outcomes.len());
    for outcome in outcomes {
        let status = match &outcome.result {
            Ok(evidence) => format!(
                "ok bytes={} ast-nodes={} diagnostic={}",
                evidence.byte_len, evidence.ast_node_count, evidence.diagnostic_code
            ),
            Err(failure) => match failure {
                CaseFailure::Read(kind) => format!("error read-{kind:?}"),
                CaseFailure::InvalidUtf8Content => "error invalid-utf8-content".to_owned(),
                CaseFailure::SourceModel => "error source-model".to_owned(),
                CaseFailure::AstModel => "error ast-model".to_owned(),
                CaseFailure::DiagnosticModel => "error diagnostic-model".to_owned(),
                CaseFailure::WiringInvariant => "error wiring-invariant".to_owned(),
            },
        };
        lines.push(format!(
            "{}\t{status}",
            escaped_report_path(&outcome.relative_path)
        ));
    }
    lines.join("\n")
}

fn escaped_report_path(path: &str) -> String {
    let mut escaped = String::with_capacity(path.len());
    for character in path.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '\t' => escaped.push_str("\\t"),
            '\r' => escaped.push_str("\\r"),
            '\n' => escaped.push_str("\\n"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            loop {
                let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
                let path = env::temp_dir().join(format!(
                    "koven-fixtures-{}-{sequence}-{label}",
                    std::process::id()
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self { path },
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("failed to create isolated fixture directory: {error}"),
                }
            }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn write(root: &Path, relative_path: &str, bytes: &[u8]) {
        let path = root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("fixture parent directories must be created");
        }
        fs::write(path, bytes).expect("fixture contents must be written");
    }

    fn relative_paths(cases: &[FixtureCase]) -> Vec<&str> {
        cases
            .iter()
            .map(|case| case.relative_path.as_str())
            .collect()
    }

    fn assert_discovery_error(root: &Path, expected: SuiteError) {
        match discover_fixtures(root) {
            Ok(cases) => panic!(
                "expected discovery to fail, but it found {:?}",
                relative_paths(&cases)
            ),
            Err(actual) => assert_eq!(actual, expected),
        }
    }

    #[test]
    fn checked_in_suite_executes_the_exact_source_loading_case() {
        let outcomes = run_suite(&fixture_root()).expect("the checked-in suite must be valid");

        assert_eq!(outcomes.len(), 1, "one checked-in .ko case must execute");
        assert_eq!(outcomes[0].relative_path, "unicode.ko");
        let evidence = outcomes[0]
            .result
            .as_ref()
            .expect("the checked-in source-loading case must pass");
        assert!(evidence.byte_len > 0);
        assert_eq!(evidence.ast_node_count, 1);
        assert_eq!(evidence.diagnostic_code, "L9000");
        assert_eq!(
            stable_report(&outcomes),
            format!(
                "unicode.ko\tok bytes={} ast-nodes=1 diagnostic=L9000",
                evidence.byte_len
            )
        );
    }

    #[test]
    fn discovery_recurses_and_sorts_complete_utf8_relative_paths() {
        let temp = TempDir::new("sort");
        write(temp.path(), "b/a.ko", b"b");
        write(temp.path(), "a/z.ko", b"z");
        write(temp.path(), "a.ko", b"a");
        write(temp.path(), "中文/β.ko", "你好".as_bytes());
        fs::create_dir_all(temp.path().join("nested.ko"))
            .expect("a directory may itself end in .ko");
        write(temp.path(), "nested.ko/case.ko", b"nested");

        let first = discover_fixtures(temp.path()).expect("all entries are valid fixtures");
        let second = discover_fixtures(temp.path()).expect("discovery must be repeatable");
        let expected = ["a.ko", "a/z.ko", "b/a.ko", "nested.ko/case.ko", "中文/β.ko"];

        assert_eq!(relative_paths(&first), expected);
        assert_eq!(relative_paths(&second), expected);
    }

    #[test]
    fn empty_and_recursively_empty_suites_are_configuration_errors() {
        let empty = TempDir::new("empty");
        assert_discovery_error(empty.path(), SuiteError::NoFixtures);

        let nested = TempDir::new("nested-empty");
        fs::create_dir_all(nested.path().join("one/two"))
            .expect("empty nested directories must be created");
        assert_discovery_error(nested.path(), SuiteError::NoFixtures);
    }

    #[test]
    fn every_unknown_extension_is_rejected_instead_of_becoming_an_empty_suite() {
        for (index, name) in ["case.txt", "case.KO", "case.ko.bak", "README", ".DS_Store"]
            .into_iter()
            .enumerate()
        {
            let temp = TempDir::new(&format!("unknown-{index}"));
            write(temp.path(), name, b"unknown");
            assert_discovery_error(
                temp.path(),
                SuiteError::InvalidEntries(vec![DiscoveryIssue::UnknownExtension {
                    relative_path: name.to_owned(),
                }]),
            );
        }
    }

    #[test]
    fn valid_and_invalid_utf8_contents_produce_sorted_structured_outcomes() {
        let temp = TempDir::new("content");
        write(temp.path(), "z-empty.ko", b"");
        write(temp.path(), "a-unicode.ko", "你好\r\nβ\n".as_bytes());
        write(temp.path(), "m-invalid.ko", &[0xff]);

        let outcomes = run_suite(temp.path()).expect("all fixture paths are valid");

        assert_eq!(
            outcomes
                .iter()
                .map(|outcome| outcome.relative_path.as_str())
                .collect::<Vec<_>>(),
            ["a-unicode.ko", "m-invalid.ko", "z-empty.ko"]
        );
        assert_eq!(outcomes[1].result, Err(CaseFailure::InvalidUtf8Content));
        assert_eq!(
            stable_report(&outcomes),
            concat!(
                "a-unicode.ko\tok bytes=11 ast-nodes=1 diagnostic=L9000\n",
                "m-invalid.ko\terror invalid-utf8-content\n",
                "z-empty.ko\tok bytes=0 ast-nodes=1 diagnostic=L9000",
            )
        );
        let temp_path = temp
            .path()
            .to_str()
            .expect("the test runtime's temporary root must be UTF-8");
        assert!(!stable_report(&outcomes).contains(temp_path));
    }

    #[cfg(unix)]
    #[test]
    fn unix_backslash_component_is_not_confused_with_a_directory_separator() {
        let temp = TempDir::new("backslash");
        write(temp.path(), "literal\\name.ko", b"component");
        write(temp.path(), "literal/name.ko", b"nested");

        let cases = discover_fixtures(temp.path()).expect("both Unix paths are valid");

        assert_eq!(
            relative_paths(&cases),
            ["literal/name.ko", "literal\\name.ko"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn stable_report_escapes_path_delimiters_from_real_file_names() {
        let temp = TempDir::new("report-escaping");
        for name in [
            "carriage\rreturn.ko",
            "line\nbreak.ko",
            "slash\\name.ko",
            "tab\tname.ko",
        ] {
            write(temp.path(), name, b"x");
        }

        let report = stable_report(&run_suite(temp.path()).expect("all UTF-8 paths are valid"));

        assert_eq!(
            report,
            concat!(
                "carriage\\rreturn.ko\tok bytes=1 ast-nodes=1 diagnostic=L9000\n",
                "line\\nbreak.ko\tok bytes=1 ast-nodes=1 diagnostic=L9000\n",
                "slash\\\\name.ko\tok bytes=1 ast-nodes=1 diagnostic=L9000\n",
                "tab\\tname.ko\tok bytes=1 ast-nodes=1 diagnostic=L9000",
            )
        );
        assert!(!report.contains('\r'));
        assert_eq!(report.lines().count(), 4);
    }

    #[cfg(unix)]
    #[test]
    fn file_directory_and_dangling_symlinks_are_rejected_without_following() {
        use std::os::unix::fs::symlink;

        for (label, target_setup, link_name) in [
            ("file", Some(("target.ko", false)), "alias.ko"),
            ("directory", Some(("target", true)), "alias"),
            ("dangling", None, "alias.ko"),
        ] {
            let temp = TempDir::new(label);
            let suite = temp.path().join("suite");
            fs::create_dir(&suite).expect("suite directory must be created");
            let target = temp.path().join("outside-target");
            if let Some((child, is_directory)) = target_setup {
                if is_directory {
                    fs::create_dir(&target).expect("target directory must be created");
                    write(&target, child, b"outside");
                } else {
                    fs::write(&target, b"outside").expect("target file must be created");
                }
            }
            symlink(&target, suite.join(link_name)).expect("symlink must be created");

            assert_discovery_error(
                &suite,
                SuiteError::InvalidEntries(vec![DiscoveryIssue::Symlink {
                    relative_path: link_name.to_owned(),
                }]),
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_discovery_rejects_non_utf8_file_and_directory_components() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};

        let file_temp = TempDir::new("non-utf8-file");
        let file_name = OsString::from_vec(vec![0xff, b'.', b'k', b'o']);
        fs::write(file_temp.path().join(file_name), b"invalid path")
            .expect("the filesystem must accept the non-UTF-8 test name");
        assert_discovery_error(
            file_temp.path(),
            SuiteError::InvalidEntries(vec![DiscoveryIssue::NonUtf8RelativePath]),
        );

        let directory_temp = TempDir::new("non-utf8-directory");
        let directory_name = OsString::from_vec(vec![0xfe]);
        let directory = directory_temp.path().join(directory_name);
        fs::create_dir(&directory).expect("the non-UTF-8 directory must be created");
        write(&directory, "case.ko", b"unreachable");
        assert_discovery_error(
            directory_temp.path(),
            SuiteError::InvalidEntries(vec![DiscoveryIssue::NonUtf8RelativePath]),
        );
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    #[test]
    fn unix_non_utf8_components_are_rejected_before_filesystem_io() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};

        let file_name = OsString::from_vec(vec![0xff, b'.', b'k', b'o']);
        assert_eq!(
            checked_relative_path(Path::new(&file_name)),
            Err(DiscoveryIssue::NonUtf8RelativePath)
        );

        let directory_name = OsString::from_vec(vec![0xfe]);
        assert_eq!(
            checked_relative_path(&PathBuf::from(directory_name).join("case.ko")),
            Err(DiscoveryIssue::NonUtf8RelativePath)
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_unpaired_surrogate_components_are_rejected() {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};

        let path = PathBuf::from(OsString::from_wide(&[0xd800])).join("case.ko");

        assert_eq!(
            checked_relative_path(&path),
            Err(DiscoveryIssue::NonUtf8RelativePath)
        );
    }
}
