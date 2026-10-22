//! 固定共同 Unit 恢复观测；原迁移前观测保留在提交历史和独立证据中。

mod observation;

use lsp_types::{DiagnosticRelatedInformation, Location, Position, Range, Uri};

use crate::{analysis, diagnostic_adapter::convert_unit_diagnostics};

pub(crate) use observation::{assert_recovery_snapshot, recovery_publication};

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

fn fixtures() -> [(&'static str, &'static str); 11] {
    [
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
    ]
}

#[test]
fn frozen_recovery_adapter_preserves_all_fields_and_every_definition_offset() {
    // 只审计固定期望的坐标变换，不再重复运行十一源生产分析。
    observation::assert_recovery_rebasing();
}

#[test]
fn recovery_keeps_all_stage_diagnostics_once_and_still_navigates_after_errors() {
    let (source_id, analysis) =
        analysis::analyze("file:///recovery.ko", RECOVERY).expect("recovery");
    let sources = analysis.name_snapshot().sources();
    let source_unit = observation::source_unit(&analysis, source_id);
    let codes = analysis
        .diagnostics()
        .iter()
        .map(|d| d.code().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        codes,
        ["L0001", "L0018", "L0090", "L0131", "L0084", "L0080"]
    );
    let declaration = RECOVERY.find("target").expect("target declaration");
    let reference = RECOVERY.rfind("target").expect("target reference");
    let targets = analysis.definitions().targets_at(source_unit, reference);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].span.start(), declaration);
    assert_eq!(targets[0].span.end(), declaration + "target".len());
    let uri: Uri = "file:///recovery.ko".parse().expect("URI");
    assert_eq!(targets[0].source_unit, source_unit);
    assert_eq!(targets[0].span.source_id(), source_id);
    let mut publications =
        convert_unit_diagnostics(sources, &[(source_id, uri.clone())], analysis.diagnostics())
            .expect("diagnostics");
    assert_eq!(publications.len(), 1);
    let mapped = publications.pop().expect("document diagnostics");
    let missing = RECOVERY.find("missing").expect("missing token");
    assert_eq!(
        mapped.last().expect("name diagnostic").range,
        Range::new(position(RECOVERY, missing), position(RECOVERY, missing + 7))
    );
    let moved = RECOVERY.find("take(resource)").expect("first consume") + "take(".len();
    let parameter = RECOVERY
        .find("own resource: Resource")
        .expect("selected parameter");
    assert_eq!(
        mapped[3].related_information.as_ref().expect("move labels"),
        &vec![
            DiagnosticRelatedInformation {
                location: Location::new(
                    uri.clone(),
                    Range::new(
                        position(RECOVERY, moved),
                        position(RECOVERY, moved + "resource".len()),
                    ),
                ),
                message: "value was moved here".to_owned(),
            },
            DiagnosticRelatedInformation {
                location: Location::new(
                    uri,
                    Range::new(
                        position(RECOVERY, parameter),
                        position(RECOVERY, parameter + "own resource: Resource".len()),
                    ),
                ),
                message: "selected parameter declared here".to_owned(),
            },
        ]
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
