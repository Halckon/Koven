//! SPEC-0005 至 SPEC-0007 的语言 fixture 发现、执行与失败保护。

mod support;

#[path = "support/fixture_codes.rs"]
mod fixture_codes;

use std::{
    collections::BTreeMap,
    env, fs, io,
    path::{Component, Path, PathBuf},
};

use fixture_codes::phase0_fixture_code;
use lang_frontend::{
    ast::AstFile,
    diagnostic::{Diagnostic, DiagnosticCodeCatalog, Severity, codes, ordered_diagnostics},
    lexer::{LexedFile, LexemeKind, lex},
    parser::{
        Expression, FunctionBody, FunctionForm, Item, Statement, SyntaxAst, TypeRef, parse_block,
        parse_declaration, parse_expression, parse_file,
    },
    source::{SourceId, SourceMap},
};

const FIXTURE_MESSAGE: &str = "Phase 0 fixture wiring";

struct FixtureCase {
    relative_path: String,
    disk_path: PathBuf,
}

struct LexerFailCase {
    relative_path: String,
    source_path: PathBuf,
    sidecar_path: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
struct ExpectedDiagnostic {
    code: String,
    start: usize,
    end: usize,
}

#[derive(Debug, PartialEq, Eq)]
enum SidecarError {
    Empty,
    EmptyLine { line: usize },
    BareCarriageReturn { line: usize },
    WrongColumnCount { line: usize },
    InvalidCode { line: usize },
    InvalidOffset { line: usize },
    InvalidSpan { line: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SidecarSpanPolicy {
    LexerDiagnostics,
    MergedParserDiagnostics,
}

impl SidecarSpanPolicy {
    fn permits_empty(self, code: &str) -> bool {
        matches!(self, Self::MergedParserDiagnostics)
            && !matches!(
                code,
                "L0001" | "L0002" | "L0003" | "L0004" | "L0005" | "L0006" | "L0007" | "L0008"
            )
    }
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
struct LexerCaseOutcome {
    relative_path: String,
    result: Result<LexerEvidence, LexerCaseFailure>,
}

#[derive(Debug, PartialEq, Eq)]
struct ParserCaseOutcome {
    relative_path: String,
    result: Result<ParserEvidence, ParserCaseFailure>,
}

#[derive(Debug, PartialEq, Eq)]
struct DeclarationCaseOutcome {
    relative_path: String,
    result: Result<DeclarationEvidence, ParserCaseFailure>,
}

#[derive(Debug, PartialEq, Eq)]
struct BlockCaseOutcome {
    relative_path: String,
    result: Result<BlockEvidence, ParserCaseFailure>,
}

#[derive(Debug, PartialEq, Eq)]
struct BlockEvidence {
    byte_len: usize,
    statement_count: usize,
    item_count: usize,
    expression_count: usize,
    diagnostic_count: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct DeclarationEvidence {
    byte_len: usize,
    item_count: usize,
    expression_count: usize,
    type_ref_count: usize,
    diagnostic_count: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct ParserEvidence {
    byte_len: usize,
    expression_count: usize,
    diagnostic_count: usize,
}

#[derive(Debug, PartialEq, Eq)]
enum ParserCaseFailure {
    ReadSource(io::ErrorKind),
    InvalidUtf8Source,
    ReadSidecar(io::ErrorKind),
    InvalidUtf8Sidecar,
    InvalidSidecar(SidecarError),
    SourceModel,
    LexerInternal,
    ParserInternal,
    RootInvariant,
    UnexpectedDiagnostics,
    DiagnosticModel,
    DiagnosticSeverity,
    DiagnosticOrder,
    DiagnosticMismatch,
}

#[derive(Debug, PartialEq, Eq)]
struct LexerEvidence {
    byte_len: usize,
    lexeme_count: usize,
    diagnostic_count: usize,
}

#[derive(Debug, PartialEq, Eq)]
enum LexerCaseFailure {
    ReadSource(io::ErrorKind),
    InvalidUtf8Source,
    ReadSidecar(io::ErrorKind),
    InvalidUtf8Sidecar,
    InvalidSidecar(SidecarError),
    SourceModel,
    LexerInternal,
    LexemeInvariant,
    UnexpectedDiagnostics,
    DiagnosticModel,
    DiagnosticSeverity,
    DiagnosticOrder,
    DiagnosticMismatch,
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
    MissingSidecar {
        relative_path: String,
    },
    OrphanSidecar {
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

fn phase0_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase0/source-pass")
}

fn lexer_pass_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase1/lexer-pass")
}

fn lexer_fail_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase1/lexer-fail")
}

fn parser_pass_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase1/parser-expression-pass")
}

fn parser_fail_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase1/parser-expression-fail")
}

fn parser_declaration_pass_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase1/parser-declaration-pass")
}

fn parser_declaration_fail_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase1/parser-declaration-fail")
}

fn parser_file_pass_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase1/parser-file-pass")
}

fn parser_file_fail_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase1/parser-file-fail")
}

fn parser_block_pass_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase1/parser-block-pass")
}

fn parser_block_fail_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase1/parser-block-fail")
}

fn parser_lambda_pass_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase1/parser-lambda-pass")
}

fn parser_lambda_fail_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase1/parser-lambda-fail")
}

fn parser_implicit_unit_pass_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase1/parser-implicit-unit-pass")
}

fn parser_implicit_unit_fail_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase1/parser-implicit-unit-fail")
}

fn discover_fixtures(root: &Path) -> Result<Vec<FixtureCase>, SuiteError> {
    discover_files(root, &["ko"])
}

fn discover_files(
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

fn discover_lexer_fail_cases(root: &Path) -> Result<Vec<LexerFailCase>, SuiteError> {
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

fn collect_entries(
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
        DiscoveryIssue::MissingSidecar { relative_path } => (4, relative_path, String::new()),
        DiscoveryIssue::OrphanSidecar { relative_path } => (5, relative_path, String::new()),
        DiscoveryIssue::UnsupportedEntryType { relative_path } => (6, relative_path, String::new()),
    }
}

fn parse_sidecar(
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

fn parse_decimal_offset(raw: &str) -> Option<usize> {
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok()
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

fn run_lexer_pass_suite(root: &Path) -> Result<Vec<LexerCaseOutcome>, SuiteError> {
    let cases = discover_fixtures(root)?;
    Ok(cases
        .iter()
        .map(|case| LexerCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_lexer_pass_case(case),
        })
        .collect())
}

fn run_lexer_pass_case(case: &FixtureCase) -> Result<LexerEvidence, LexerCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        lex_fixture_source(&case.relative_path, &case.disk_path)?;
    validate_lexemes(source_id, byte_len, &lexed)?;
    if !lexed.diagnostics().is_empty() {
        return Err(LexerCaseFailure::UnexpectedDiagnostics);
    }

    if sources
        .source_text(source_id)
        .map_err(|_| LexerCaseFailure::SourceModel)?
        .len()
        != byte_len
    {
        return Err(LexerCaseFailure::SourceModel);
    }
    Ok(LexerEvidence {
        byte_len,
        lexeme_count: lexed.lexemes().len(),
        diagnostic_count: 0,
    })
}

fn run_lexer_fail_suite(root: &Path) -> Result<Vec<LexerCaseOutcome>, SuiteError> {
    let cases = discover_lexer_fail_cases(root)?;
    Ok(cases
        .iter()
        .map(|case| LexerCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_lexer_fail_case(case),
        })
        .collect())
}

fn run_lexer_fail_case(case: &LexerFailCase) -> Result<LexerEvidence, LexerCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        lex_fixture_source(&case.relative_path, &case.source_path)?;
    validate_lexemes(source_id, byte_len, &lexed)?;

    let sidecar_bytes = fs::read(&case.sidecar_path)
        .map_err(|error| LexerCaseFailure::ReadSidecar(error.kind()))?;
    let sidecar =
        String::from_utf8(sidecar_bytes).map_err(|_| LexerCaseFailure::InvalidUtf8Sidecar)?;
    let catalog = codes::catalog().map_err(|_| LexerCaseFailure::DiagnosticModel)?;
    let expected = parse_sidecar(
        &sources,
        source_id,
        &sidecar,
        &catalog,
        SidecarSpanPolicy::LexerDiagnostics,
    )
    .map_err(LexerCaseFailure::InvalidSidecar)?;

    if lexed
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.severity() != Severity::Error)
    {
        return Err(LexerCaseFailure::DiagnosticSeverity);
    }
    let actual = diagnostic_expectations(lexed.diagnostics().iter());
    let ordered = ordered_diagnostics(&sources, lexed.diagnostics())
        .map_err(|_| LexerCaseFailure::DiagnosticModel)?;
    let ordered_actual = diagnostic_expectations(ordered.into_iter());
    if actual != ordered_actual {
        return Err(LexerCaseFailure::DiagnosticOrder);
    }
    if actual != expected {
        return Err(LexerCaseFailure::DiagnosticMismatch);
    }

    Ok(LexerEvidence {
        byte_len,
        lexeme_count: lexed.lexemes().len(),
        diagnostic_count: actual.len(),
    })
}

fn run_parser_pass_suite(root: &Path) -> Result<Vec<ParserCaseOutcome>, SuiteError> {
    let cases = discover_fixtures(root)?;
    Ok(cases
        .iter()
        .map(|case| ParserCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_parser_pass_case(case),
        })
        .collect())
}

fn run_parser_pass_case(case: &FixtureCase) -> Result<ParserEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.disk_path)?;
    let parsed =
        parse_expression(&sources, &lexed).map_err(|_| ParserCaseFailure::ParserInternal)?;
    if !parsed.diagnostics().is_empty() {
        return Err(ParserCaseFailure::UnexpectedDiagnostics);
    }
    if parsed.ast().source_id() != source_id
        || parsed.ast().expressions().get(parsed.root()).is_err()
    {
        return Err(ParserCaseFailure::RootInvariant);
    }

    Ok(ParserEvidence {
        byte_len,
        expression_count: parsed.ast().expressions().len(),
        diagnostic_count: 0,
    })
}

fn run_parser_fail_suite(root: &Path) -> Result<Vec<ParserCaseOutcome>, SuiteError> {
    let cases = discover_lexer_fail_cases(root)?;
    Ok(cases
        .iter()
        .map(|case| ParserCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_parser_fail_case(case),
        })
        .collect())
}

fn run_parser_fail_case(case: &LexerFailCase) -> Result<ParserEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.source_path)?;
    let parsed =
        parse_expression(&sources, &lexed).map_err(|_| ParserCaseFailure::ParserInternal)?;
    if parsed.ast().source_id() != source_id
        || parsed.ast().expressions().get(parsed.root()).is_err()
    {
        return Err(ParserCaseFailure::RootInvariant);
    }

    let sidecar_bytes = fs::read(&case.sidecar_path)
        .map_err(|error| ParserCaseFailure::ReadSidecar(error.kind()))?;
    let sidecar =
        String::from_utf8(sidecar_bytes).map_err(|_| ParserCaseFailure::InvalidUtf8Sidecar)?;
    let catalog = codes::catalog().map_err(|_| ParserCaseFailure::DiagnosticModel)?;
    let expected = parse_sidecar(
        &sources,
        source_id,
        &sidecar,
        &catalog,
        SidecarSpanPolicy::MergedParserDiagnostics,
    )
    .map_err(ParserCaseFailure::InvalidSidecar)?;

    if parsed
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.severity() != Severity::Error)
    {
        return Err(ParserCaseFailure::DiagnosticSeverity);
    }
    let actual = diagnostic_expectations(parsed.diagnostics().iter());
    let ordered = ordered_diagnostics(&sources, parsed.diagnostics())
        .map_err(|_| ParserCaseFailure::DiagnosticModel)?;
    if actual != diagnostic_expectations(ordered.into_iter()) {
        return Err(ParserCaseFailure::DiagnosticOrder);
    }
    if actual != expected {
        return Err(ParserCaseFailure::DiagnosticMismatch);
    }

    Ok(ParserEvidence {
        byte_len,
        expression_count: parsed.ast().expressions().len(),
        diagnostic_count: actual.len(),
    })
}

fn validate_lambda_root(
    ast: &SyntaxAst,
    root: lang_frontend::ast::ExpressionId,
) -> Result<(), ParserCaseFailure> {
    let Expression::Lambda { body, .. } = ast
        .expressions()
        .get(root)
        .map_err(|_| ParserCaseFailure::RootInvariant)?
        .payload()
    else {
        return Err(ParserCaseFailure::RootInvariant);
    };
    if !matches!(
        ast.statements()
            .get(*body)
            .map_err(|_| ParserCaseFailure::RootInvariant)?
            .payload(),
        Statement::LambdaBody { .. }
    ) {
        return Err(ParserCaseFailure::RootInvariant);
    }
    Ok(())
}

fn run_lambda_pass_suite(root: &Path) -> Result<Vec<ParserCaseOutcome>, SuiteError> {
    let cases = discover_fixtures(root)?;
    Ok(cases
        .iter()
        .map(|case| ParserCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_lambda_pass_case(case),
        })
        .collect())
}

fn run_lambda_pass_case(case: &FixtureCase) -> Result<ParserEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.disk_path)?;
    let parsed =
        parse_expression(&sources, &lexed).map_err(|_| ParserCaseFailure::ParserInternal)?;
    if !parsed.diagnostics().is_empty() {
        return Err(ParserCaseFailure::UnexpectedDiagnostics);
    }
    if parsed.ast().source_id() != source_id {
        return Err(ParserCaseFailure::RootInvariant);
    }
    // 独立入口无 trailing 诊断意味着已消费全部非 trivia 输入；这里另锁定 typed child。
    validate_lambda_root(parsed.ast(), parsed.root())?;
    Ok(ParserEvidence {
        byte_len,
        expression_count: parsed.ast().expressions().len(),
        diagnostic_count: 0,
    })
}

fn run_lambda_fail_suite(root: &Path) -> Result<Vec<ParserCaseOutcome>, SuiteError> {
    let cases = discover_lexer_fail_cases(root)?;
    Ok(cases
        .iter()
        .map(|case| ParserCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_lambda_fail_case(case),
        })
        .collect())
}

fn run_lambda_fail_case(case: &LexerFailCase) -> Result<ParserEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.source_path)?;
    let parsed =
        parse_expression(&sources, &lexed).map_err(|_| ParserCaseFailure::ParserInternal)?;
    if parsed.ast().source_id() != source_id {
        return Err(ParserCaseFailure::RootInvariant);
    }
    validate_lambda_root(parsed.ast(), parsed.root())?;
    let expected = load_parser_sidecar(&sources, source_id, &case.sidecar_path)?;
    validate_parser_diagnostics(&sources, parsed.diagnostics(), &expected)?;
    Ok(ParserEvidence {
        byte_len,
        expression_count: parsed.ast().expressions().len(),
        diagnostic_count: expected.len(),
    })
}

fn run_declaration_pass_suite(root: &Path) -> Result<Vec<DeclarationCaseOutcome>, SuiteError> {
    let cases = discover_fixtures(root)?;
    Ok(cases
        .iter()
        .map(|case| DeclarationCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_declaration_pass_case(case),
        })
        .collect())
}

fn run_declaration_pass_case(case: &FixtureCase) -> Result<DeclarationEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.disk_path)?;
    let parsed =
        parse_declaration(&sources, &lexed).map_err(|_| ParserCaseFailure::ParserInternal)?;
    if !parsed.diagnostics().is_empty() {
        return Err(ParserCaseFailure::UnexpectedDiagnostics);
    }
    if parsed.ast().source_id() != source_id || parsed.ast().items().get(parsed.root()).is_err() {
        return Err(ParserCaseFailure::RootInvariant);
    }

    Ok(DeclarationEvidence {
        byte_len,
        item_count: parsed.ast().items().len(),
        expression_count: parsed.ast().expressions().len(),
        type_ref_count: parsed.ast().type_refs().len(),
        diagnostic_count: 0,
    })
}

fn run_declaration_fail_suite(root: &Path) -> Result<Vec<DeclarationCaseOutcome>, SuiteError> {
    let cases = discover_lexer_fail_cases(root)?;
    Ok(cases
        .iter()
        .map(|case| DeclarationCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_declaration_fail_case(case),
        })
        .collect())
}

fn run_declaration_fail_case(
    case: &LexerFailCase,
) -> Result<DeclarationEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.source_path)?;
    let parsed =
        parse_declaration(&sources, &lexed).map_err(|_| ParserCaseFailure::ParserInternal)?;
    if parsed.ast().source_id() != source_id || parsed.ast().items().get(parsed.root()).is_err() {
        return Err(ParserCaseFailure::RootInvariant);
    }

    let expected = load_parser_sidecar(&sources, source_id, &case.sidecar_path)?;
    validate_parser_diagnostics(&sources, parsed.diagnostics(), &expected)?;

    Ok(DeclarationEvidence {
        byte_len,
        item_count: parsed.ast().items().len(),
        expression_count: parsed.ast().expressions().len(),
        type_ref_count: parsed.ast().type_refs().len(),
        diagnostic_count: expected.len(),
    })
}

fn run_file_pass_suite(root: &Path) -> Result<Vec<DeclarationCaseOutcome>, SuiteError> {
    let cases = discover_fixtures(root)?;
    Ok(cases
        .iter()
        .map(|case| DeclarationCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_file_pass_case(case),
        })
        .collect())
}

fn run_file_pass_case(case: &FixtureCase) -> Result<DeclarationEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.disk_path)?;
    let parsed = parse_file(&sources, &lexed).map_err(|_| ParserCaseFailure::ParserInternal)?;
    if !parsed.diagnostics().is_empty() || parsed.ast().source_id() != source_id {
        return Err(ParserCaseFailure::UnexpectedDiagnostics);
    }
    if parsed
        .roots()
        .iter()
        .any(|root| parsed.ast().items().get(*root).is_err())
    {
        return Err(ParserCaseFailure::RootInvariant);
    }
    Ok(DeclarationEvidence {
        byte_len,
        item_count: parsed.ast().items().len(),
        expression_count: parsed.ast().expressions().len(),
        type_ref_count: parsed.ast().type_refs().len(),
        diagnostic_count: 0,
    })
}

fn run_file_fail_suite(root: &Path) -> Result<Vec<DeclarationCaseOutcome>, SuiteError> {
    let cases = discover_lexer_fail_cases(root)?;
    Ok(cases
        .iter()
        .map(|case| DeclarationCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_file_fail_case(case),
        })
        .collect())
}

fn run_file_fail_case(case: &LexerFailCase) -> Result<DeclarationEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.source_path)?;
    let parsed = parse_file(&sources, &lexed).map_err(|_| ParserCaseFailure::ParserInternal)?;
    if parsed.ast().source_id() != source_id
        || parsed
            .roots()
            .iter()
            .any(|root| parsed.ast().items().get(*root).is_err())
    {
        return Err(ParserCaseFailure::RootInvariant);
    }
    let expected = load_parser_sidecar(&sources, source_id, &case.sidecar_path)?;
    validate_parser_diagnostics(&sources, parsed.diagnostics(), &expected)?;
    Ok(DeclarationEvidence {
        byte_len,
        item_count: parsed.ast().items().len(),
        expression_count: parsed.ast().expressions().len(),
        type_ref_count: parsed.ast().type_refs().len(),
        diagnostic_count: expected.len(),
    })
}

fn validate_implicit_unit_root(
    ast: &SyntaxAst,
    root: lang_frontend::ast::ItemId,
    relative_path: &str,
) -> Result<(), ParserCaseFailure> {
    let Item::Function { form, .. } = ast
        .items()
        .get(root)
        .map_err(|_| ParserCaseFailure::RootInvariant)?
        .payload()
    else {
        return Err(ParserCaseFailure::RootInvariant);
    };
    let valid = match relative_path {
        "absent.ko" => matches!(form, FunctionForm::ImplicitUnitAbsent),
        "empty-block.ko" | "nonempty-block.ko" => {
            matches!(form, FunctionForm::ImplicitUnitBlock(_))
        }
        "explicit-unit.ko" | "explicit-other.ko" => matches!(
            form,
            FunctionForm::Explicit {
                type_ref,
                body: FunctionBody::Absent | FunctionBody::Block(_),
                ..
            } if matches!(
                ast.type_refs()
                    .get(*type_ref)
                    .map_err(|_| ParserCaseFailure::RootInvariant)?
                    .payload(),
                TypeRef::Qualified { .. }
            )
        ),
        "missing-expression-return.ko" | "missing-type.ko" => matches!(
            form,
            FunctionForm::Explicit {
                type_ref,
                body: FunctionBody::Expression { .. },
                ..
            } if matches!(
                ast.type_refs()
                    .get(*type_ref)
                    .map_err(|_| ParserCaseFailure::RootInvariant)?
                    .payload(),
                TypeRef::Error
            )
        ),
        _ => false,
    };
    valid.then_some(()).ok_or(ParserCaseFailure::RootInvariant)
}

fn run_implicit_unit_pass_suite(root: &Path) -> Result<Vec<DeclarationCaseOutcome>, SuiteError> {
    let cases = discover_fixtures(root)?;
    Ok(cases
        .iter()
        .map(|case| DeclarationCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: (|| {
                let (sources, source_id, lexed, byte_len) =
                    parser_fixture_source(&case.relative_path, &case.disk_path)?;
                let parsed = parse_declaration(&sources, &lexed)
                    .map_err(|_| ParserCaseFailure::ParserInternal)?;
                if !parsed.diagnostics().is_empty() {
                    return Err(ParserCaseFailure::UnexpectedDiagnostics);
                }
                if parsed.source_id() != source_id {
                    return Err(ParserCaseFailure::RootInvariant);
                }
                validate_implicit_unit_root(parsed.ast(), parsed.root(), &case.relative_path)?;
                Ok(DeclarationEvidence {
                    byte_len,
                    item_count: parsed.ast().items().len(),
                    expression_count: parsed.ast().expressions().len(),
                    type_ref_count: parsed.ast().type_refs().len(),
                    diagnostic_count: 0,
                })
            })(),
        })
        .collect())
}

fn run_implicit_unit_fail_suite(root: &Path) -> Result<Vec<DeclarationCaseOutcome>, SuiteError> {
    let cases = discover_lexer_fail_cases(root)?;
    Ok(cases
        .iter()
        .map(|case| DeclarationCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: (|| {
                let (sources, source_id, lexed, byte_len) =
                    parser_fixture_source(&case.relative_path, &case.source_path)?;
                let parsed = parse_declaration(&sources, &lexed)
                    .map_err(|_| ParserCaseFailure::ParserInternal)?;
                if parsed.source_id() != source_id {
                    return Err(ParserCaseFailure::RootInvariant);
                }
                validate_implicit_unit_root(parsed.ast(), parsed.root(), &case.relative_path)?;
                let expected = load_parser_sidecar(&sources, source_id, &case.sidecar_path)?;
                validate_parser_diagnostics(&sources, parsed.diagnostics(), &expected)?;
                Ok(DeclarationEvidence {
                    byte_len,
                    item_count: parsed.ast().items().len(),
                    expression_count: parsed.ast().expressions().len(),
                    type_ref_count: parsed.ast().type_refs().len(),
                    diagnostic_count: expected.len(),
                })
            })(),
        })
        .collect())
}

fn run_block_pass_suite(root: &Path) -> Result<Vec<BlockCaseOutcome>, SuiteError> {
    let cases = discover_fixtures(root)?;
    Ok(cases
        .iter()
        .map(|case| BlockCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_block_pass_case(case),
        })
        .collect())
}

fn run_block_pass_case(case: &FixtureCase) -> Result<BlockEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.disk_path)?;
    let parsed = parse_block(&sources, &lexed).map_err(|_| ParserCaseFailure::ParserInternal)?;
    if !parsed.diagnostics().is_empty() {
        return Err(ParserCaseFailure::UnexpectedDiagnostics);
    }
    if parsed.ast().source_id() != source_id
        || parsed.ast().statements().get(parsed.root()).is_err()
    {
        return Err(ParserCaseFailure::RootInvariant);
    }
    Ok(BlockEvidence {
        byte_len,
        statement_count: parsed.ast().statements().len(),
        item_count: parsed.ast().items().len(),
        expression_count: parsed.ast().expressions().len(),
        diagnostic_count: 0,
    })
}

fn run_block_fail_suite(root: &Path) -> Result<Vec<BlockCaseOutcome>, SuiteError> {
    let cases = discover_lexer_fail_cases(root)?;
    Ok(cases
        .iter()
        .map(|case| BlockCaseOutcome {
            relative_path: case.relative_path.clone(),
            result: run_block_fail_case(case),
        })
        .collect())
}

fn run_block_fail_case(case: &LexerFailCase) -> Result<BlockEvidence, ParserCaseFailure> {
    let (sources, source_id, lexed, byte_len) =
        parser_fixture_source(&case.relative_path, &case.source_path)?;
    let parsed = parse_block(&sources, &lexed).map_err(|_| ParserCaseFailure::ParserInternal)?;
    if parsed.ast().source_id() != source_id
        || parsed.ast().statements().get(parsed.root()).is_err()
    {
        return Err(ParserCaseFailure::RootInvariant);
    }
    let expected = load_parser_sidecar(&sources, source_id, &case.sidecar_path)?;
    validate_parser_diagnostics(&sources, parsed.diagnostics(), &expected)?;
    Ok(BlockEvidence {
        byte_len,
        statement_count: parsed.ast().statements().len(),
        item_count: parsed.ast().items().len(),
        expression_count: parsed.ast().expressions().len(),
        diagnostic_count: expected.len(),
    })
}

fn load_parser_sidecar(
    sources: &SourceMap,
    source_id: SourceId,
    sidecar_path: &Path,
) -> Result<Vec<ExpectedDiagnostic>, ParserCaseFailure> {
    let sidecar_bytes =
        fs::read(sidecar_path).map_err(|error| ParserCaseFailure::ReadSidecar(error.kind()))?;
    let sidecar =
        String::from_utf8(sidecar_bytes).map_err(|_| ParserCaseFailure::InvalidUtf8Sidecar)?;
    let catalog = codes::catalog().map_err(|_| ParserCaseFailure::DiagnosticModel)?;
    parse_sidecar(
        sources,
        source_id,
        &sidecar,
        &catalog,
        SidecarSpanPolicy::MergedParserDiagnostics,
    )
    .map_err(ParserCaseFailure::InvalidSidecar)
}

fn validate_parser_diagnostics(
    sources: &SourceMap,
    diagnostics: &[Diagnostic],
    expected: &[ExpectedDiagnostic],
) -> Result<(), ParserCaseFailure> {
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity() != Severity::Error)
    {
        return Err(ParserCaseFailure::DiagnosticSeverity);
    }
    let actual = diagnostic_expectations(diagnostics.iter());
    let ordered = ordered_diagnostics(sources, diagnostics)
        .map_err(|_| ParserCaseFailure::DiagnosticModel)?;
    if actual != diagnostic_expectations(ordered.into_iter()) {
        return Err(ParserCaseFailure::DiagnosticOrder);
    }
    if actual != expected {
        return Err(ParserCaseFailure::DiagnosticMismatch);
    }
    Ok(())
}

fn parser_fixture_source(
    relative_path: &str,
    disk_path: &Path,
) -> Result<(SourceMap, SourceId, LexedFile, usize), ParserCaseFailure> {
    let bytes = fs::read(disk_path).map_err(|error| ParserCaseFailure::ReadSource(error.kind()))?;
    let text = String::from_utf8(bytes).map_err(|_| ParserCaseFailure::InvalidUtf8Source)?;
    let byte_len = text.len();
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source(relative_path.to_owned(), text)
        .map_err(|_| ParserCaseFailure::SourceModel)?;
    let lexed = lex(&sources, source_id).map_err(|_| ParserCaseFailure::LexerInternal)?;
    Ok((sources, source_id, lexed, byte_len))
}

fn lex_fixture_source(
    relative_path: &str,
    disk_path: &Path,
) -> Result<(SourceMap, SourceId, LexedFile, usize), LexerCaseFailure> {
    let bytes = fs::read(disk_path).map_err(|error| LexerCaseFailure::ReadSource(error.kind()))?;
    let text = String::from_utf8(bytes).map_err(|_| LexerCaseFailure::InvalidUtf8Source)?;
    let byte_len = text.len();
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source(relative_path.to_owned(), text)
        .map_err(|_| LexerCaseFailure::SourceModel)?;
    let lexed = lex(&sources, source_id).map_err(|_| LexerCaseFailure::LexerInternal)?;
    Ok((sources, source_id, lexed, byte_len))
}

fn validate_lexemes(
    source_id: SourceId,
    source_len: usize,
    lexed: &LexedFile,
) -> Result<(), LexerCaseFailure> {
    if lexed.source_id() != source_id || lexed.lexemes().is_empty() {
        return Err(LexerCaseFailure::LexemeInvariant);
    }

    let mut expected_start = 0;
    let mut saw_eof = false;
    for (index, lexeme) in lexed.lexemes().iter().enumerate() {
        let span = lexeme.span();
        let is_eof = matches!(lexeme.kind(), LexemeKind::Eof);
        if span.source_id() != source_id || span.start() != expected_start {
            return Err(LexerCaseFailure::LexemeInvariant);
        }
        if is_eof {
            if index + 1 != lexed.lexemes().len() || !span.is_empty() || span.start() != source_len
            {
                return Err(LexerCaseFailure::LexemeInvariant);
            }
            saw_eof = true;
        } else {
            if span.is_empty() || span.end() > source_len {
                return Err(LexerCaseFailure::LexemeInvariant);
            }
            expected_start = span.end();
        }
    }
    if !saw_eof || expected_start != source_len {
        return Err(LexerCaseFailure::LexemeInvariant);
    }
    Ok(())
}

fn diagnostic_expectations<'a>(
    diagnostics: impl Iterator<Item = &'a Diagnostic>,
) -> Vec<ExpectedDiagnostic> {
    diagnostics
        .map(|diagnostic| ExpectedDiagnostic {
            code: diagnostic.code().to_string(),
            start: diagnostic.primary_span().start(),
            end: diagnostic.primary_span().end(),
        })
        .collect()
}

fn lexer_stable_report(outcomes: &[LexerCaseOutcome]) -> String {
    let mut lines = Vec::with_capacity(outcomes.len());
    for outcome in outcomes {
        let status = match &outcome.result {
            Ok(evidence) => format!(
                "ok bytes={} lexemes={} diagnostics={}",
                evidence.byte_len, evidence.lexeme_count, evidence.diagnostic_count
            ),
            Err(failure) => format!("error {failure:?}"),
        };
        lines.push(format!(
            "{}\t{status}",
            escaped_report_path(&outcome.relative_path)
        ));
    }
    lines.join("\n")
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

    fn fail_relative_paths(cases: &[LexerFailCase]) -> Vec<&str> {
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

    fn assert_fail_discovery_error(root: &Path, expected: SuiteError) {
        match discover_lexer_fail_cases(root) {
            Ok(cases) => panic!(
                "expected fail discovery to fail, but it found {:?}",
                fail_relative_paths(&cases)
            ),
            Err(actual) => assert_eq!(actual, expected),
        }
    }

    #[test]
    fn checked_in_suite_executes_the_exact_source_loading_case() {
        let outcomes =
            run_suite(&phase0_fixture_root()).expect("the checked-in suite must be valid");

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
    fn checked_in_lexer_suites_execute_exact_pass_and_fail_cases() {
        let pass = run_lexer_pass_suite(&lexer_pass_fixture_root())
            .expect("the checked-in lexer pass suite must be valid");
        let fail = run_lexer_fail_suite(&lexer_fail_fixture_root())
            .expect("the checked-in lexer fail suite must be valid");

        assert_eq!(pass.len(), 1);
        assert_eq!(pass[0].relative_path, "basic.ko");
        let pass_evidence = pass[0]
            .result
            .as_ref()
            .expect("the checked-in lexer pass case must pass");
        assert!(pass_evidence.lexeme_count > 1);
        assert_eq!(pass_evidence.diagnostic_count, 0);

        assert_eq!(fail.len(), 1);
        assert_eq!(fail[0].relative_path, "invalid-character.ko");
        let fail_evidence = fail[0]
            .result
            .as_ref()
            .expect("the checked-in lexer fail case must match its sidecar");
        assert!(fail_evidence.lexeme_count > 1);
        assert_eq!(fail_evidence.diagnostic_count, 1);

        assert_eq!(
            lexer_stable_report(&pass),
            format!(
                "basic.ko\tok bytes={} lexemes={} diagnostics=0",
                pass_evidence.byte_len, pass_evidence.lexeme_count
            )
        );
        assert_eq!(
            lexer_stable_report(&fail),
            format!(
                "invalid-character.ko\tok bytes={} lexemes={} diagnostics=1",
                fail_evidence.byte_len, fail_evidence.lexeme_count
            )
        );
    }

    #[test]
    fn checked_in_parser_suites_execute_real_pass_and_fail_cases() {
        let pass = run_parser_pass_suite(&parser_pass_fixture_root())
            .expect("the checked-in parser pass suite must be valid");
        let fail = run_parser_fail_suite(&parser_fail_fixture_root())
            .expect("the checked-in parser fail suite must be valid");

        assert_eq!(pass.len(), 2);
        assert_eq!(pass[0].relative_path, "call-arguments.ko");
        let call_evidence = pass[0]
            .result
            .as_ref()
            .expect("the checked-in parser pass case must parse without diagnostics");
        assert!(call_evidence.expression_count > 1);
        assert_eq!(call_evidence.diagnostic_count, 0);
        assert_eq!(pass[1].relative_path, "precedence.ko");
        let pass_evidence = pass[1]
            .result
            .as_ref()
            .expect("the precedence pass case must parse without diagnostics");
        assert!(pass_evidence.expression_count > 1);
        assert_eq!(pass_evidence.diagnostic_count, 0);

        assert_eq!(fail.len(), 2);
        assert_eq!(fail[0].relative_path, "call-missing-value.ko");
        let call_fail_evidence = fail[0]
            .result
            .as_ref()
            .expect("the checked-in parser fail case must match merged diagnostics");
        assert!(call_fail_evidence.expression_count > 0);
        assert_eq!(call_fail_evidence.diagnostic_count, 1);
        assert_eq!(fail[1].relative_path, "trailing-token.ko");
        let fail_evidence = fail[1]
            .result
            .as_ref()
            .expect("the trailing-token fail case must match merged diagnostics");
        assert!(fail_evidence.expression_count > 0);
        assert_eq!(fail_evidence.diagnostic_count, 1);
    }

    #[test]
    fn checked_in_declaration_suites_execute_real_pass_and_fail_cases() {
        let pass = run_declaration_pass_suite(&parser_declaration_pass_fixture_root())
            .expect("the declaration pass suite must be valid");
        let fail = run_declaration_fail_suite(&parser_declaration_fail_fixture_root())
            .expect("the declaration fail suite must be valid");

        assert_eq!(pass.len(), 3);
        assert_eq!(pass[0].relative_path, "callable-parameters.ko");
        let callable_evidence = pass[0]
            .result
            .as_ref()
            .expect("the declaration pass case must parse without diagnostics");
        assert_eq!(callable_evidence.item_count, 1);
        assert_eq!(callable_evidence.diagnostic_count, 0);
        assert!(callable_evidence.type_ref_count > 0);
        assert_eq!(pass[1].relative_path, "constant.ko");
        let pass_evidence = pass[1]
            .result
            .as_ref()
            .expect("the constant declaration must parse without diagnostics");
        assert_eq!(pass_evidence.item_count, 1);
        assert!(pass_evidence.expression_count > 0);
        assert!(pass_evidence.type_ref_count > 0);
        assert_eq!(pass_evidence.diagnostic_count, 0);
        let function_evidence = pass[2]
            .result
            .as_ref()
            .expect("the function block fixture must parse without diagnostics");
        assert_eq!(pass[2].relative_path, "function-block.ko");
        assert!(function_evidence.item_count >= 2);
        assert!(function_evidence.expression_count > 0);

        assert_eq!(fail.len(), 2);
        assert_eq!(fail[0].relative_path, "duplicate-parameter-mode.ko");
        let duplicate_evidence = fail[0]
            .result
            .as_ref()
            .expect("the declaration fail case must match its sidecar");
        assert_eq!(duplicate_evidence.item_count, 1);
        assert_eq!(duplicate_evidence.diagnostic_count, 1);
        assert_eq!(fail[1].relative_path, "empty.ko");
        let fail_evidence = fail[1]
            .result
            .as_ref()
            .expect("the empty declaration fail case must match its sidecar");
        assert_eq!(fail_evidence.item_count, 1);
        assert_eq!(fail_evidence.diagnostic_count, 1);
    }

    #[test]
    fn checked_in_file_suites_execute_multiple_roots_and_cross_declaration_recovery() {
        let pass = run_file_pass_suite(&parser_file_pass_fixture_root())
            .expect("the file pass suite must be valid");
        let fail = run_file_fail_suite(&parser_file_fail_fixture_root())
            .expect("the file fail suite must be valid");
        assert_eq!(pass.len(), 3);
        for outcome in pass {
            let evidence = outcome.result.as_ref().expect("file pass fixture");
            assert_eq!(evidence.item_count, 2);
            assert_eq!(evidence.diagnostic_count, 0);
        }
        assert_eq!(fail.len(), 5);
        for outcome in fail {
            let evidence = outcome.result.as_ref().expect("file fail fixture");
            assert_eq!(evidence.item_count, 2);
            assert_eq!(evidence.diagnostic_count, 1);
        }
    }

    #[test]
    fn checked_in_block_suites_execute_real_typed_ast_and_diagnostics() {
        let pass = run_block_pass_suite(&parser_block_pass_fixture_root())
            .expect("the block pass suite must be valid");
        let fail = run_block_fail_suite(&parser_block_fail_fixture_root())
            .expect("the block fail suite must be valid");

        assert_eq!(pass.len(), 2);
        assert_eq!(pass[0].relative_path, "destructuring.ko");
        let destructuring = pass[0]
            .result
            .as_ref()
            .expect("the destructuring block fixture must parse without diagnostics");
        assert!(destructuring.statement_count >= 2);
        assert_eq!(destructuring.expression_count, 1);
        assert_eq!(destructuring.diagnostic_count, 0);
        assert_eq!(pass[1].relative_path, "sequence.ko");
        let pass = pass[1]
            .result
            .as_ref()
            .expect("the block pass fixture must parse without diagnostics");
        assert!(pass.statement_count >= 4);
        assert!(pass.item_count >= 2);
        assert!(pass.expression_count >= 2);
        assert_eq!(pass.diagnostic_count, 0);

        assert_eq!(fail.len(), 2);
        assert_eq!(fail[0].relative_path, "destructuring-trailing-comma.ko");
        let destructuring = fail[0]
            .result
            .as_ref()
            .expect("the destructuring block fail fixture must match its sidecar");
        assert!(destructuring.statement_count >= 2);
        assert_eq!(destructuring.diagnostic_count, 1);
        assert_eq!(fail[1].relative_path, "unsupported-local-function.ko");
        let fail = fail[1]
            .result
            .as_ref()
            .expect("the block fail fixture must match its sidecar");
        assert!(fail.statement_count >= 2);
        assert_eq!(fail.diagnostic_count, 1);
    }

    #[test]
    fn checked_in_lambda_suites_execute_real_typed_ast_and_diagnostics() {
        let pass = run_lambda_pass_suite(&parser_lambda_pass_fixture_root())
            .expect("the lambda pass suite must be valid");
        let fail = run_lambda_fail_suite(&parser_lambda_fail_fixture_root())
            .expect("the lambda fail suite must be valid");

        assert_eq!(pass.len(), 2);
        assert_eq!(pass[0].relative_path, "basic.ko");
        let basic = pass[0]
            .result
            .as_ref()
            .expect("the lambda pass fixture must parse to Lambda/LambdaBody");
        assert!(basic.expression_count >= 3);
        assert_eq!(basic.diagnostic_count, 0);
        assert_eq!(pass[1].relative_path, "destructuring.ko");
        let destructuring = pass[1]
            .result
            .as_ref()
            .expect("the destructuring lambda fixture must parse without diagnostics");
        assert!(destructuring.expression_count >= 2);
        assert_eq!(destructuring.diagnostic_count, 0);

        assert_eq!(fail.len(), 2);
        assert_eq!(
            fail[0].relative_path,
            "destructuring-missing-initializer.ko"
        );
        let destructuring = fail[0]
            .result
            .as_ref()
            .expect("the destructuring lambda fail fixture must match its sidecar");
        assert!(destructuring.expression_count >= 2);
        assert_eq!(destructuring.diagnostic_count, 1);
        assert_eq!(fail[1].relative_path, "expected-element.ko");
        let fail = fail[1]
            .result
            .as_ref()
            .expect("the lambda fail fixture must match its sidecar");
        assert!(fail.expression_count >= 1);
        assert_eq!(fail.diagnostic_count, 1);
    }

    #[test]
    fn checked_in_implicit_unit_suites_execute_each_closed_ast_form() {
        let pass = run_implicit_unit_pass_suite(&parser_implicit_unit_pass_fixture_root())
            .expect("the implicit Unit pass suite must be valid");
        let fail = run_implicit_unit_fail_suite(&parser_implicit_unit_fail_fixture_root())
            .expect("the implicit Unit fail suite must be valid");

        assert_eq!(
            pass.iter()
                .map(|case| case.relative_path.as_str())
                .collect::<Vec<_>>(),
            [
                "absent.ko",
                "empty-block.ko",
                "explicit-other.ko",
                "explicit-unit.ko",
                "nonempty-block.ko",
            ]
        );
        assert!(pass.iter().all(|case| case.result.is_ok()), "{pass:?}");
        assert_eq!(
            fail.iter()
                .map(|case| case.relative_path.as_str())
                .collect::<Vec<_>>(),
            ["missing-expression-return.ko", "missing-type.ko"]
        );
        assert!(
            fail.iter().all(|case| {
                case.result
                    .as_ref()
                    .is_ok_and(|evidence| evidence.diagnostic_count == 1)
            }),
            "{fail:?}"
        );
    }

    #[test]
    fn parser_fixture_discovery_rejects_zero_and_unpaired_fail_cases() {
        let empty_pass = TempDir::new("parser-pass-empty");
        assert_eq!(
            run_parser_pass_suite(empty_pass.path()),
            Err(SuiteError::NoFixtures)
        );

        let empty_fail = TempDir::new("parser-fail-empty");
        assert_eq!(
            run_parser_fail_suite(empty_fail.path()),
            Err(SuiteError::NoFixtures)
        );

        let unpaired = TempDir::new("parser-fail-unpaired");
        write(unpaired.path(), "missing.ko", b"a b");
        write(unpaired.path(), "orphan.diag", b"L0013\t2\t3\n");
        assert_eq!(
            run_parser_fail_suite(unpaired.path()),
            Err(SuiteError::InvalidEntries(vec![
                DiscoveryIssue::MissingSidecar {
                    relative_path: "missing.ko".to_owned(),
                },
                DiscoveryIssue::OrphanSidecar {
                    relative_path: "orphan.diag".to_owned(),
                },
            ]))
        );

        assert_eq!(
            run_declaration_pass_suite(empty_pass.path()),
            Err(SuiteError::NoFixtures)
        );
        assert_eq!(
            run_implicit_unit_pass_suite(empty_pass.path()),
            Err(SuiteError::NoFixtures)
        );
        assert_eq!(
            run_implicit_unit_fail_suite(empty_fail.path()),
            Err(SuiteError::NoFixtures)
        );
        assert_eq!(
            run_implicit_unit_fail_suite(unpaired.path()),
            Err(SuiteError::InvalidEntries(vec![
                DiscoveryIssue::MissingSidecar {
                    relative_path: "missing.ko".to_owned(),
                },
                DiscoveryIssue::OrphanSidecar {
                    relative_path: "orphan.diag".to_owned(),
                },
            ]))
        );
        assert_eq!(
            run_declaration_fail_suite(empty_fail.path()),
            Err(SuiteError::NoFixtures)
        );
        assert_eq!(
            run_declaration_fail_suite(unpaired.path()),
            Err(SuiteError::InvalidEntries(vec![
                DiscoveryIssue::MissingSidecar {
                    relative_path: "missing.ko".to_owned(),
                },
                DiscoveryIssue::OrphanSidecar {
                    relative_path: "orphan.diag".to_owned(),
                },
            ]))
        );

        assert_eq!(
            run_block_pass_suite(empty_pass.path()),
            Err(SuiteError::NoFixtures)
        );
        assert_eq!(
            run_block_fail_suite(empty_fail.path()),
            Err(SuiteError::NoFixtures)
        );
        assert_eq!(
            run_block_fail_suite(unpaired.path()),
            Err(SuiteError::InvalidEntries(vec![
                DiscoveryIssue::MissingSidecar {
                    relative_path: "missing.ko".to_owned(),
                },
                DiscoveryIssue::OrphanSidecar {
                    relative_path: "orphan.diag".to_owned(),
                },
            ]))
        );

        assert_eq!(
            run_lambda_pass_suite(empty_pass.path()),
            Err(SuiteError::NoFixtures)
        );
        assert_eq!(
            run_lambda_fail_suite(empty_fail.path()),
            Err(SuiteError::NoFixtures)
        );
        assert_eq!(
            run_lambda_fail_suite(unpaired.path()),
            Err(SuiteError::InvalidEntries(vec![
                DiscoveryIssue::MissingSidecar {
                    relative_path: "missing.ko".to_owned(),
                },
                DiscoveryIssue::OrphanSidecar {
                    relative_path: "orphan.diag".to_owned(),
                },
            ]))
        );
    }

    #[test]
    fn parser_fail_runner_rejects_invalid_sidecar_lines() {
        let temp = TempDir::new("parser-invalid-sidecar");
        write(temp.path(), "case.ko", b"a b");
        write(temp.path(), "case.diag", b"L0013\t2\n");

        let outcomes = run_parser_fail_suite(temp.path()).expect("the fixture pair is complete");

        assert_eq!(
            outcomes[0].result,
            Err(ParserCaseFailure::InvalidSidecar(
                SidecarError::WrongColumnCount { line: 1 }
            ))
        );
    }

    #[test]
    fn lexer_fail_discovery_pairs_nested_cases_and_sorts_sources() {
        let temp = TempDir::new("fail-pairs");
        write(temp.path(), "z.ko", b"z");
        write(temp.path(), "z.diag", b"L0001\t0\t1\n");
        write(temp.path(), "nested/a.diag", b"L0001\t0\t1\n");
        write(temp.path(), "nested/a.ko", b"a");

        let cases = discover_lexer_fail_cases(temp.path())
            .expect("every fail source has exactly one sidecar");

        assert_eq!(fail_relative_paths(&cases), ["nested/a.ko", "z.ko"]);
        assert!(cases[0].sidecar_path.ends_with("nested/a.diag"));
        assert!(cases[1].sidecar_path.ends_with("z.diag"));
    }

    #[test]
    fn lexer_fail_discovery_rejects_empty_missing_or_orphan_suites() {
        let empty = TempDir::new("fail-empty");
        assert_fail_discovery_error(empty.path(), SuiteError::NoFixtures);

        let unpaired = TempDir::new("fail-unpaired");
        write(unpaired.path(), "missing.ko", b"x");
        write(unpaired.path(), "orphan.diag", b"L0001\t0\t1\n");
        assert_fail_discovery_error(
            unpaired.path(),
            SuiteError::InvalidEntries(vec![
                DiscoveryIssue::MissingSidecar {
                    relative_path: "missing.ko".to_owned(),
                },
                DiscoveryIssue::OrphanSidecar {
                    relative_path: "orphan.diag".to_owned(),
                },
            ]),
        );
    }

    #[test]
    fn lexer_fail_discovery_rejects_unknown_extensions() {
        let temp = TempDir::new("fail-unknown");
        write(temp.path(), "case.ko", b"x");
        write(temp.path(), "case.diag", b"L0001\t0\t1\n");
        write(temp.path(), "README", b"unexpected");

        assert_fail_discovery_error(
            temp.path(),
            SuiteError::InvalidEntries(vec![DiscoveryIssue::UnknownExtension {
                relative_path: "README".to_owned(),
            }]),
        );
    }

    #[test]
    fn lexer_fail_runner_reports_invalid_utf8_source_and_sidecar_separately() {
        let invalid_source = TempDir::new("fail-invalid-source");
        write(invalid_source.path(), "case.ko", &[0xff]);
        write(invalid_source.path(), "case.diag", b"L0001\t0\t1\n");
        let source_outcomes =
            run_lexer_fail_suite(invalid_source.path()).expect("the source and sidecar are paired");
        assert_eq!(
            source_outcomes[0].result,
            Err(LexerCaseFailure::InvalidUtf8Source)
        );

        let invalid_sidecar = TempDir::new("fail-invalid-sidecar");
        write(invalid_sidecar.path(), "case.ko", "β".as_bytes());
        write(invalid_sidecar.path(), "case.diag", &[0xff]);
        let sidecar_outcomes = run_lexer_fail_suite(invalid_sidecar.path())
            .expect("the source and sidecar are paired");
        assert_eq!(
            sidecar_outcomes[0].result,
            Err(LexerCaseFailure::InvalidUtf8Sidecar)
        );
    }

    #[test]
    fn sidecar_parser_applies_suite_specific_empty_span_policy() {
        let mut sources = SourceMap::new();
        let source_id = sources
            .add_source("case.ko", "βx")
            .expect("test source name is unique");
        let catalog = DiagnosticCodeCatalog::try_new(&["L0001", "L0002", "L0009"])
            .expect("test codes are valid");

        assert_eq!(
            parse_sidecar(
                &sources,
                source_id,
                "L0001\t0\t2\r\nL0002\t2\t3",
                &catalog,
                SidecarSpanPolicy::LexerDiagnostics,
            ),
            Ok(vec![
                ExpectedDiagnostic {
                    code: "L0001".to_owned(),
                    start: 0,
                    end: 2,
                },
                ExpectedDiagnostic {
                    code: "L0002".to_owned(),
                    start: 2,
                    end: 3,
                },
            ])
        );

        assert_eq!(
            parse_sidecar(
                &sources,
                source_id,
                "L0009\t3\t3",
                &catalog,
                SidecarSpanPolicy::MergedParserDiagnostics,
            ),
            Ok(vec![ExpectedDiagnostic {
                code: "L0009".to_owned(),
                start: 3,
                end: 3,
            }])
        );
        assert_eq!(
            parse_sidecar(
                &sources,
                source_id,
                "L0001\t3\t3",
                &catalog,
                SidecarSpanPolicy::MergedParserDiagnostics,
            ),
            Err(SidecarError::InvalidSpan { line: 1 })
        );

        for (sidecar, expected) in [
            ("", SidecarError::Empty),
            ("\n", SidecarError::EmptyLine { line: 1 }),
            ("L0001\t0\t2\n\n", SidecarError::EmptyLine { line: 2 }),
            (
                "L0001\t0\t2\r",
                SidecarError::BareCarriageReturn { line: 1 },
            ),
            (
                "L0001\t0\t2\rx\n",
                SidecarError::BareCarriageReturn { line: 1 },
            ),
            ("L0001\t0\n", SidecarError::WrongColumnCount { line: 1 }),
            ("L9999\t0\t2\n", SidecarError::InvalidCode { line: 1 }),
            ("L0001\t+0\t2\n", SidecarError::InvalidOffset { line: 1 }),
            (
                "L0001\t999999999999999999999999999999\t2\n",
                SidecarError::InvalidOffset { line: 1 },
            ),
            ("L0001\t0\t0\n", SidecarError::InvalidSpan { line: 1 }),
            ("L0001\t0\t1\n", SidecarError::InvalidSpan { line: 1 }),
            ("L0001\t2\t1\n", SidecarError::InvalidSpan { line: 1 }),
            ("L0001\t0\t4\n", SidecarError::InvalidSpan { line: 1 }),
        ] {
            assert_eq!(
                parse_sidecar(
                    &sources,
                    source_id,
                    sidecar,
                    &catalog,
                    SidecarSpanPolicy::LexerDiagnostics,
                ),
                Err(expected),
                "unexpected result for sidecar {sidecar:?}",
            );
        }
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
