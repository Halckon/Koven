//! Phase 0 结构化诊断的人类可读纯文本 renderer。

use lang_frontend::{
    diagnostic::{
        Diagnostic, DiagnosticDetail, DiagnosticError, Severity, SpanRole, ordered_diagnostics,
    },
    source::{SourceMap, SourcePosition, Span},
};

/// 把结构化诊断确定性渲染为 Phase 0 无颜色文本。
///
/// 函数不读文件、不写 stdout / stderr，也不修改输入顺序。每条诊断及每项附加信息各占一行，
/// 每行以 `\n` 结束；空诊断集合返回空字符串。source 名称来自 [`SourceMap`]，只转义反斜杠、
/// CR 和 LF 以维持单行、无歧义的展示，不做路径发现、规范化或附加。
///
/// # Errors
///
/// 任一主范围或关联标签无法由给定 source map 解析时返回具体内部错误，不产生部分文本。
pub(super) fn render_diagnostics(
    sources: &SourceMap,
    diagnostics: &[Diagnostic],
) -> Result<String, DiagnosticError> {
    let ordered = ordered_diagnostics(sources, diagnostics)?;
    render_diagnostics_in_order(sources, &ordered)
}

/// 按调用方已经建立的稳定顺序渲染诊断，不再按展示路径重排。
pub(super) fn render_diagnostics_in_order(
    sources: &SourceMap,
    diagnostics: &[&Diagnostic],
) -> Result<String, DiagnosticError> {
    let mut rendered = String::new();

    for diagnostic in diagnostics {
        let primary = resolve_location(sources, SpanRole::Primary, diagnostic.primary_span())?;
        rendered.push_str(&format!(
            "{}[{}] {}:{}:{}-{}:{}: {}\n",
            severity_name(diagnostic.severity()),
            diagnostic.code(),
            primary.source_name,
            primary.start.line(),
            primary.start.column(),
            primary.end.line(),
            primary.end.column(),
            diagnostic.message(),
        ));

        for (detail_index, detail) in diagnostic.details().iter().enumerate() {
            match detail {
                DiagnosticDetail::Label(label) => {
                    let location =
                        resolve_location(sources, SpanRole::Label { detail_index }, label.span())?;
                    rendered.push_str(&format!(
                        "  label {}:{}:{}-{}:{}: {}\n",
                        location.source_name,
                        location.start.line(),
                        location.start.column(),
                        location.end.line(),
                        location.end.column(),
                        label.message(),
                    ));
                }
                DiagnosticDetail::Note(text) => {
                    rendered.push_str(&format!("  note: {}\n", text.as_str()));
                }
                DiagnosticDetail::Help(text) => {
                    rendered.push_str(&format!("  help: {}\n", text.as_str()));
                }
            }
        }
    }

    Ok(rendered)
}

struct RenderedLocation {
    source_name: String,
    start: SourcePosition,
    end: SourcePosition,
}

fn resolve_location(
    sources: &SourceMap,
    role: SpanRole,
    span: Span,
) -> Result<RenderedLocation, DiagnosticError> {
    let source_name = sources
        .source_name(span.source_id())
        .map_err(|source| DiagnosticError::InvalidSpan { role, source })?;
    let start = sources
        .position(span.source_id(), span.start())
        .map_err(|source| DiagnosticError::InvalidSpan { role, source })?;
    let end = sources
        .position(span.source_id(), span.end())
        .map_err(|source| DiagnosticError::InvalidSpan { role, source })?;

    Ok(RenderedLocation {
        source_name: escape_source_name(source_name),
        start,
        end,
    })
}

fn escape_source_name(name: &str) -> String {
    let mut escaped = String::with_capacity(name.len());
    for character in name.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '\r' => escaped.push_str("\\r"),
            '\n' => escaped.push_str("\\n"),
            other => escaped.push(other),
        }
    }
    escaped
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    }
}

#[cfg(test)]
mod tests {
    use lang_frontend::{
        diagnostic::{
            Diagnostic, DiagnosticCode, DiagnosticCodeCatalog, DiagnosticError, Severity, SpanRole,
        },
        source::{SourceError, SourceId, SourceMap, Span},
    };

    use super::render_diagnostics;

    fn code(raw: &str) -> DiagnosticCode {
        DiagnosticCodeCatalog::try_new(&[raw])
            .expect("the test catalog contains one valid code")
            .resolve(raw)
            .expect("the test code is registered")
    }

    fn add_source(sources: &mut SourceMap, name: &str, text: &str) -> SourceId {
        sources
            .add_source(name, text)
            .expect("test source names must be unique")
    }

    fn span(sources: &SourceMap, source_id: SourceId, start: usize, end: usize) -> Span {
        sources
            .span(source_id, start, end)
            .expect("test spans must be valid")
    }

    fn diagnostic(
        sources: &SourceMap,
        code: DiagnosticCode,
        message: &str,
        primary_span: Span,
    ) -> Diagnostic {
        Diagnostic::new(sources, Severity::Error, code, message, primary_span)
            .expect("test diagnostics must be valid")
    }

    #[test]
    fn empty_collection_renders_empty_text() {
        assert_eq!(
            render_diagnostics(&SourceMap::new(), &[]),
            Ok(String::new())
        );
    }

    #[test]
    fn renders_primary_only_and_complete_cross_source_diagnostics() {
        let code = code("L9000");
        let mut sources = SourceMap::new();
        let related_id = add_source(&mut sources, "related.ko", "xy\nz");
        let primary_id = add_source(&mut sources, "primary.ko", "a界d");
        let primary_span = span(&sources, primary_id, 1, 4);
        let related_first = span(&sources, related_id, 0, 2);
        let related_second = span(&sources, related_id, 3, 4);
        let mut complete = diagnostic(&sources, code, "complete message", primary_span);
        complete
            .add_label(&sources, related_first, "first relation")
            .expect("the first label is valid");
        complete
            .add_note("note after first label")
            .expect("the note is valid");
        complete
            .add_label(&sources, related_second, "second relation")
            .expect("the second label is valid");
        complete
            .add_help("apply this suggestion")
            .expect("the help is valid");
        let primary_only = diagnostic(
            &sources,
            code,
            "primary only",
            span(&sources, primary_id, 0, 0),
        );

        let rendered = render_diagnostics(&sources, &[complete, primary_only])
            .expect("all diagnostic spans resolve");

        assert_eq!(
            rendered,
            concat!(
                "error[L9000] primary.ko:1:1-1:1: primary only\n",
                "error[L9000] primary.ko:1:2-1:3: complete message\n",
                "  label related.ko:1:1-1:3: first relation\n",
                "  note: note after first label\n",
                "  label related.ko:2:1-2:2: second relation\n",
                "  help: apply this suggestion\n",
            )
        );
    }

    #[test]
    fn renders_unicode_crlf_multiline_and_eof_positions() {
        let code = code("L9000");
        let mut sources = SourceMap::new();
        let source_id = add_source(&mut sources, "unicode.ko", "a界\r\nβ\n");
        let mut diagnostic = diagnostic(
            &sources,
            code,
            "unicode range",
            span(&sources, source_id, 1, 8),
        );
        diagnostic
            .add_label(&sources, span(&sources, source_id, 9, 9), "end of file")
            .expect("the EOF label is valid");

        assert_eq!(
            render_diagnostics(&sources, &[diagnostic]),
            Ok(concat!(
                "error[L9000] unicode.ko:1:2-2:2: unicode range\n",
                "  label unicode.ko:3:1-3:1: end of file\n",
            )
            .to_owned())
        );
    }

    #[test]
    fn rendering_is_independent_of_input_and_source_load_order() {
        let code = code("L9000");
        let mut first = SourceMap::new();
        let first_b = add_source(&mut first, "b.ko", "b");
        let first_a = add_source(&mut first, "a.ko", "a");
        let first_diagnostics = [
            diagnostic(&first, code, "from b", span(&first, first_b, 0, 1)),
            diagnostic(&first, code, "from a", span(&first, first_a, 0, 1)),
        ];

        let mut second = SourceMap::new();
        let second_a = add_source(&mut second, "a.ko", "a");
        let second_b = add_source(&mut second, "b.ko", "b");
        let second_diagnostics = [
            diagnostic(&second, code, "from a", span(&second, second_a, 0, 1)),
            diagnostic(&second, code, "from b", span(&second, second_b, 0, 1)),
        ];

        let first_rendered =
            render_diagnostics(&first, &first_diagnostics).expect("the first set is valid");
        let second_rendered =
            render_diagnostics(&second, &second_diagnostics).expect("the second set is valid");

        assert_eq!(first_rendered, second_rendered);
        assert_eq!(
            first_rendered,
            concat!(
                "error[L9000] a.ko:1:1-1:2: from a\n",
                "error[L9000] b.ko:1:1-1:2: from b\n",
            )
        );
    }

    #[test]
    fn source_names_escape_backslashes_and_line_breaks_without_path_normalization() {
        let code = code("L9000");
        let mut sources = SourceMap::new();
        let source_id = add_source(&mut sources, "virtual\\name\r\n.ko", "x");
        let diagnostic = diagnostic(&sources, code, "message", span(&sources, source_id, 0, 1));

        assert_eq!(
            render_diagnostics(&sources, &[diagnostic]),
            Ok("error[L9000] virtual\\\\name\\r\\n.ko:1:1-1:2: message\n".to_owned())
        );
    }

    #[test]
    fn unresolved_primary_span_returns_internal_error_without_output() {
        let code = code("L9000");
        let mut owning_sources = SourceMap::new();
        let source_id = add_source(&mut owning_sources, "source.ko", "text");
        let diagnostic = diagnostic(
            &owning_sources,
            code,
            "message",
            span(&owning_sources, source_id, 0, 1),
        );
        let other_sources = SourceMap::new();

        assert_eq!(
            render_diagnostics(&other_sources, &[diagnostic]),
            Err(DiagnosticError::InvalidSpan {
                role: SpanRole::Primary,
                source: SourceError::InvalidSourceId { source_id },
            })
        );
    }
}
