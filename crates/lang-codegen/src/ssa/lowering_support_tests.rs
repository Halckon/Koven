//! SPEC-0255: 先在原 helper 路径锁定字节、拒绝与错误优先级。

use lang_frontend::{
    ast::ExpressionId,
    parser::{Expression, ParsedFile, StringPart},
    source::{SourceMap, Span},
};

use super::{
    LoweringError, LoweringErrorKind, lower_frontend::string_literal::decode_plain,
    unit_lower_test_support::parsed,
};

fn expression(text: &str) -> (SourceMap, ParsedFile, ExpressionId) {
    let mut sources = SourceMap::new();
    let (_, file) = parsed(
        &mut sources,
        "support/literal.ko",
        &format!("fun entry(): String = {text}"),
    );
    let id = file.ast().expressions().iter().last().unwrap().0;
    (sources, file, id)
}

fn source_text(sources: &SourceMap, parsed: &ParsedFile) -> String {
    sources.source_text(parsed.source_id()).unwrap().to_owned()
}

fn text_span(file: &ParsedFile) -> Span {
    file.ast()
        .expressions()
        .iter()
        .find_map(|(_, node)| match node.payload() {
            Expression::String { parts } => parts.iter().find_map(|part| match part {
                StringPart::Text(span) => Some(*span),
                _ => None,
            }),
            _ => None,
        })
        .unwrap()
}

fn normalized_error(
    sources: &SourceMap,
    result: Result<Option<Vec<u8>>, LoweringError>,
) -> (LoweringErrorKind, Option<(String, usize, usize)>) {
    let error = result.expect_err("expected exact helper error");
    (
        error.kind,
        error.span.map(|span| {
            (
                sources.source_name(span.source_id()).unwrap().to_owned(),
                span.start(),
                span.end(),
            )
        }),
    )
}

fn assert_span_error(
    sources: &SourceMap,
    result: Result<Option<Vec<u8>>, LoweringError>,
    kind: LoweringErrorKind,
    span: Span,
) {
    assert_eq!(
        normalized_error(sources, result),
        (
            kind,
            Some(("support/literal.ko".to_owned(), span.start(), span.end()))
        )
    );
}

#[test]
fn plain_literal_bytes_preserve_utf8_nul_all_escapes_and_nested_groups() {
    for (literal, expected) in [
        (r#""""#, b"".as_slice()),
        (r#""plain""#, b"plain".as_slice()),
        (r#""中文🙂""#, "中文🙂".as_bytes()),
        (r#""a\0b""#, b"a\0b".as_slice()),
        (r#""\\\'\"\n\r\t\0\$""#, b"\\'\"\n\r\t\0$".as_slice()),
        (r#"((("中文\0🙂")))"#, "中文\0🙂".as_bytes()),
    ] {
        let (sources, file, id) = expression(literal);
        assert_eq!(
            decode_plain(&file, &source_text(&sources, &file), id).unwrap(),
            Some(expected.to_vec()),
            "{literal}"
        );
    }
}

#[test]
fn nonliteral_and_interpolation_remain_absent_even_when_grouped() {
    for text in ["17", "value", "(value)", r#""${17}""#, r#"(("a${17}b"))"#] {
        let (sources, file, id) = expression(text);
        assert_eq!(
            decode_plain(&file, &source_text(&sources, &file), id),
            Ok(None)
        );
    }
}

#[test]
fn missing_ast_node_remains_missing_fact_without_span_before_text_access() {
    let (mut sources, file, id) = expression(r#"((("x")))"#);
    let (_, empty) = parsed(&mut sources, "support/empty.ko", "");
    assert!(id.index() >= empty.ast().expressions().len());
    assert_eq!(
        normalized_error(&sources, decode_plain(&empty, "", id)),
        (LoweringErrorKind::MissingFact, None)
    );
    assert!(decode_plain(&file, &source_text(&sources, &file), id).is_ok());
}

#[test]
fn invalid_text_range_and_utf8_boundary_keep_original_text_span() {
    let (sources, file, id) = expression(r#""x""#);
    let span = text_span(&file);
    assert_span_error(
        &sources,
        decode_plain(&file, "", id),
        LoweringErrorKind::MismatchedSource,
        span,
    );
    let mut text = " ".repeat(span.start() - 1);
    text.push('界');
    text.push_str("padding");
    assert_span_error(
        &sources,
        decode_plain(&file, &text, id),
        LoweringErrorKind::MismatchedSource,
        span,
    );
}

#[test]
fn invalid_escape_and_trailing_slash_keep_original_text_span() {
    let (sources, file, id) = expression(r#""\n""#);
    let span = text_span(&file);
    for replacement in ["\\q", "a\\"] {
        let mut text = source_text(&sources, &file);
        text.replace_range(span.start()..span.end(), replacement);
        assert_span_error(
            &sources,
            decode_plain(&file, &text, id),
            LoweringErrorKind::InvalidLiteral,
            span,
        );
    }
}

#[test]
fn text_before_interpolation_is_checked_but_interpolation_stops_later_text() {
    let (sources, file, id) = expression(r#""\n${17}""#);
    let span = text_span(&file);
    let mut text = source_text(&sources, &file);
    text.replace_range(span.start()..span.end(), "\\q");
    assert_span_error(
        &sources,
        decode_plain(&file, &text, id),
        LoweringErrorKind::InvalidLiteral,
        span,
    );
    assert_span_error(
        &sources,
        decode_plain(&file, "", id),
        LoweringErrorKind::MismatchedSource,
        span,
    );
    let (_, file, id) = expression(r#""${17}\n""#);
    assert_eq!(decode_plain(&file, "", id), Ok(None));
}
