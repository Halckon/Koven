//! SPEC-0253 冻结旧宿主手工链：独立于 single-file façade 与生产 analyze。
//!
//! 顺序来自迁移前 e6dfea4：lex/parse/name/type、DefinitionIndex、ownership；
//! parsed 已携带 lexer diagnostics，故 aggregate 不再次收集 lexer。

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail, ordered_diagnostics},
    lexer::lex,
    name_resolution::resolve_names,
    ownership_checking::check_ownership,
    parser::parse_file,
    source::{SourceMap, Span},
    type_checking::{check_types, standard_environments},
};
use lsp_types::{Position, Range, Uri};

use crate::{
    analysis,
    definition::DefinitionIndex,
    diagnostic_adapter::convert_diagnostics,
    position_adapter::{byte_offset, span_range},
};

pub(crate) const RECOVERY: &str = "#\r\nval = 1\r\n\
    const val BAD: Byte = 128\r\n\
    class Resource()\r\n\
    fun take(own resource: Resource): Unit {}\r\n\
    fun moves(own resource: Resource): Unit {\r\n\
        val first = take(resource)\r\n\
        val second = take(resource)\r\n\
    }\r\n\
    fun types(): Unit { val item: String = 1 }\r\n\
    fun names(): Unit { /* 界é😀 */ missing }\r\n\
    fun target(): Int = 1\r\n\
    fun use(): Int = /* 界é😀 */ target()\r\n";

pub(crate) struct ManualAnalysis {
    pub(crate) sources: SourceMap,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) definitions: DefinitionIndex,
}

/// 不调用生产 analyze 或新 façade；每个旧入口与诊断聚合点显式冻结。
pub(crate) fn manual(source_name: &str, text: &str) -> ManualAnalysis {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(source_name, text)
        .expect("manual source");
    let lexed = lex(&sources, source).expect("manual lex");
    let parsed = parse_file(&sources, &lexed).expect("manual parse");
    let (name_environment, type_environment) = standard_environments();
    let names = resolve_names(&sources, &parsed, &name_environment).expect("manual names");
    let typed = check_types(&sources, &parsed, &names, &type_environment).expect("manual types");
    let definitions = DefinitionIndex::build(&parsed, &names, &typed).expect("manual definitions");
    let owned = check_ownership(&sources, &parsed, &names, &typed).expect("manual ownership");
    let mut diagnostics = parsed.diagnostics().to_vec();
    diagnostics.extend_from_slice(names.diagnostics());
    diagnostics.extend_from_slice(typed.diagnostics());
    diagnostics.extend_from_slice(owned.diagnostics());
    let diagnostics = ordered_diagnostics(&sources, &diagnostics)
        .expect("manual ordering")
        .into_iter()
        .cloned()
        .collect();
    ManualAnalysis {
        sources,
        diagnostics,
        definitions,
    }
}

#[test]
fn recovering_single_file_matches_frozen_manual_diagnostics_and_every_definition_offset() {
    for (label, source) in [
        ("recovery", RECOVERY),
        (
            "lexer",
            include_str!("../../lang-cli/tests/fixtures/single_file_analysis/lexer.ko"),
        ),
        (
            "parser",
            include_str!("../../lang-cli/tests/fixtures/single_file_analysis/parser.ko"),
        ),
        (
            "names",
            include_str!("../../lang-cli/tests/fixtures/single_file_analysis/names.ko"),
        ),
        (
            "typed",
            include_str!("../../lang-cli/tests/fixtures/single_file_analysis/typed.ko"),
        ),
        (
            "ownership",
            include_str!("../../lang-cli/tests/fixtures/single_file_analysis/ownership.ko"),
        ),
        (
            "const_typed",
            include_str!("../../lang-cli/tests/fixtures/single_file_analysis/const_typed.ko"),
        ),
        (
            "const_move",
            include_str!("../../lang-cli/tests/fixtures/single_file_analysis/const_move.ko"),
        ),
        (
            "unicode",
            include_str!("../../lang-cli/tests/fixtures/single_file_analysis/unicode.ko"),
        ),
        (
            "success",
            include_str!("../../lang-cli/tests/fixtures/single_file_analysis/success.ko"),
        ),
        (
            "typed_definitions",
            "value class Pair(val field: Int)\nfun pick(value: Int): Int = value\nfun pick(value: String): String = value\nfun use(pair: Pair): Int = pick(pair.field)\n",
        ),
    ] {
        let uri: Uri = format!("file:///{label}.ko").parse().expect("URI");
        let expected = manual(uri.as_str(), source);
        let actual = analysis::analyze(uri.as_str(), source).expect("production analysis");
        assert_diagnostics(
            &actual.sources,
            &actual.diagnostics,
            &expected.sources,
            &expected.diagnostics,
            uri.as_str(),
        );
        assert_eq!(
            actual
                .sources
                .source_name(actual.definitions.source_id())
                .expect("actual source identity"),
            uri.as_str()
        );
        assert_eq!(
            expected
                .sources
                .source_name(expected.definitions.source_id())
                .expect("manual source identity"),
            uri.as_str()
        );
        assert_eq!(
            actual
                .sources
                .source_text(actual.definitions.source_id())
                .expect("source"),
            source
        );
        assert_eq!(
            convert_diagnostics(&actual.sources, &uri, &actual.diagnostics)
                .expect("actual LSP diagnostics"),
            convert_diagnostics(&expected.sources, &uri, &expected.diagnostics)
                .expect("manual LSP diagnostics"),
            "{label}: complete mapped diagnostics"
        );
        for offset in (0..=source.len()).filter(|offset| source.is_char_boundary(*offset)) {
            let targets = actual.definitions.targets_at(offset);
            assert_eq!(
                target_snapshot(&actual.sources, targets),
                target_snapshot(&expected.sources, expected.definitions.targets_at(offset)),
                "{label}: definition byte {offset}"
            );
            for target in targets {
                let range = span_range(&actual.sources, *target).expect("target range");
                assert_eq!(
                    range,
                    Range::new(
                        position(source, target.start()),
                        position(source, target.end())
                    ),
                    "{label}: independent UTF-16 target"
                );
                assert_eq!(
                    byte_offset(&actual.sources, target.source_id(), range.start)
                        .expect("target start offset"),
                    Some(target.start())
                );
            }
        }
    }
}

#[test]
fn recovery_keeps_all_stage_diagnostics_once_and_still_navigates_after_errors() {
    let analysis = analysis::analyze("file:///recovery.ko", RECOVERY).expect("recovery");
    let codes = analysis
        .diagnostics
        .iter()
        .map(|d| d.code().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        codes,
        ["L0001", "L0018", "L0090", "L0131", "L0084", "L0080"]
    );
    let declaration = RECOVERY.find("target").expect("target declaration");
    let reference = RECOVERY.rfind("target").expect("target reference");
    let targets = analysis.definitions.targets_at(reference);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].start(), declaration);
    assert_eq!(targets[0].end(), declaration + "target".len());
    let uri: Uri = "file:///recovery.ko".parse().expect("URI");
    let mapped =
        convert_diagnostics(&analysis.sources, &uri, &analysis.diagnostics).expect("diagnostics");
    let missing = RECOVERY.find("missing").expect("missing token");
    assert_eq!(
        mapped.last().expect("name diagnostic").range,
        Range::new(position(RECOVERY, missing), position(RECOVERY, missing + 7))
    );
    assert_eq!(
        mapped[3]
            .related_information
            .as_ref()
            .expect("move label")
            .len(),
        1
    );
}

/// 独立于 SourceMap/position_adapter 的测试 UTF-16 换算，仅用于 token 边界。
pub(crate) fn position(source: &str, offset: usize) -> Position {
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let start = prefix.rfind('\n').map_or(0, |newline| newline + 1);
    Position::new(
        u32::try_from(line).expect("line"),
        u32::try_from(source[start..offset].encode_utf16().count()).expect("UTF-16 column"),
    )
}

/// 两次分析拥有不同 SourceMap identity：逐一验证 provenance 后比较完整稳定 raw fields。
pub(crate) fn assert_diagnostics(
    actual_sources: &SourceMap,
    actual: &[Diagnostic],
    expected_sources: &SourceMap,
    expected: &[Diagnostic],
    uri: &str,
) {
    for (sources, diagnostics) in [(actual_sources, actual), (expected_sources, expected)] {
        for diagnostic in diagnostics {
            assert_span(sources, diagnostic.primary_span(), uri);
            for detail in diagnostic.details() {
                if let DiagnosticDetail::Label(label) = detail {
                    assert_span(sources, label.span(), uri);
                }
            }
        }
    }
    // Diagnostic Debug 包含 severity/code/message/primary source-local range 与所有有序 details；
    // 仅 SourceMap 的私有 run identity 不参与稳定行为比较。
    assert_eq!(
        format!("{actual:?}"),
        format!("{expected:?}"),
        "complete raw diagnostic facts"
    );
}

fn assert_span(sources: &SourceMap, span: Span, uri: &str) {
    assert_eq!(
        sources
            .source_name(span.source_id())
            .expect("span belongs to its own source map"),
        uri
    );
    sources
        .slice(span)
        .expect("valid span in its own source map");
}

pub(crate) fn target_snapshot(
    sources: &SourceMap,
    spans: &[Span],
) -> Vec<(String, usize, usize, String)> {
    spans
        .iter()
        .map(|span| {
            (
                sources
                    .source_name(span.source_id())
                    .expect("target belongs to source map")
                    .to_owned(),
                span.start(),
                span.end(),
                sources.slice(*span).expect("valid target span").to_owned(),
            )
        })
        .collect()
}
