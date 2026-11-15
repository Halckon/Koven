//! SPEC-0005 至 SPEC-0007 / SPEC-0115 的语言 fixture 发现、执行与失败保护。

#[path = "fixtures/discovery.rs"]
mod discovery;
mod support;
use discovery::*;
#[path = "fixtures/execution.rs"]
mod execution;
use execution::*;

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
    diagnostic::{
        Diagnostic, DiagnosticCodeCatalog, DiagnosticDetail, Severity, codes, ordered_diagnostics,
    },
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
    LexerOutputInvariant,
    ParserInternal,
    ParserOutputInvariant,
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

fn parse_fixture_twice<T, E>(
    sources: &SourceMap,
    lexed: &LexedFile,
    parse: impl Fn(&SourceMap, &LexedFile) -> Result<T, E>,
) -> Result<T, ParserCaseFailure>
where
    T: std::fmt::Debug,
{
    let first = parse(sources, lexed).map_err(|_| ParserCaseFailure::ParserInternal)?;
    let repeated = parse(sources, lexed).map_err(|_| ParserCaseFailure::ParserInternal)?;
    if format!("{first:?}") != format!("{repeated:?}") {
        return Err(ParserCaseFailure::ParserOutputInvariant);
    }
    Ok(first)
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
    let first = lex(&sources, source_id).map_err(|_| ParserCaseFailure::LexerInternal)?;
    let repeated = lex(&sources, source_id).map_err(|_| ParserCaseFailure::LexerInternal)?;
    validate_lexemes(source_id, byte_len, &first)
        .map_err(|_| ParserCaseFailure::LexerOutputInvariant)?;
    validate_lexemes(source_id, byte_len, &repeated)
        .map_err(|_| ParserCaseFailure::LexerOutputInvariant)?;
    if format!("{first:?}") != format!("{repeated:?}") {
        return Err(ParserCaseFailure::LexerOutputInvariant);
    }
    Ok((sources, source_id, first, byte_len))
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
    let first = lex(&sources, source_id).map_err(|_| LexerCaseFailure::LexerInternal)?;
    let repeated = lex(&sources, source_id).map_err(|_| LexerCaseFailure::LexerInternal)?;
    validate_lexemes(source_id, byte_len, &first)?;
    validate_lexemes(source_id, byte_len, &repeated)?;
    if format!("{first:?}") != format!("{repeated:?}") {
        return Err(LexerCaseFailure::LexemeInvariant);
    }
    Ok((sources, source_id, first, byte_len))
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
    for diagnostic in lexed.diagnostics() {
        let primary = diagnostic.primary_span();
        if primary.source_id() != source_id
            || primary.start() > primary.end()
            || primary.end() > source_len
        {
            return Err(LexerCaseFailure::LexemeInvariant);
        }
        for detail in diagnostic.details() {
            if let DiagnosticDetail::Label(label) = detail {
                let span = label.span();
                if span.source_id() != source_id
                    || span.start() > span.end()
                    || span.end() > source_len
                {
                    return Err(LexerCaseFailure::LexemeInvariant);
                }
            }
        }
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
#[path = "fixtures/contracts.rs"]
mod tests;
