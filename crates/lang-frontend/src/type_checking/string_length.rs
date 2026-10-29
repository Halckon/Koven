//! Guide v0.47 的 String 字节长度不变量；独立于目标 usize 布局。

/// 所有 live String 的 UTF-8 字节数上限，供 frontend 与 codegen 共用。
pub const MAX_STRING_BYTE_LENGTH: u64 = i32::MAX as u64;

use super::{TypeCheckingError, constant_value::decoded_text_length};
use crate::{
    diagnostic::{Diagnostic, Severity, codes},
    parser::StringPart,
    source::{SourceMap, Span},
};

/// Check before materializing bytes, including constant-folded concatenations.
pub(super) fn checked_string_length(left: u64, right: u64) -> Option<u64> {
    left.checked_add(right)
        .filter(|length| *length <= MAX_STRING_BYTE_LENGTH)
}

fn length_diagnostic(
    sources: &SourceMap,
    span: Span,
    length: u64,
) -> Result<Option<Diagnostic>, TypeCheckingError> {
    if length <= MAX_STRING_BYTE_LENGTH {
        return Ok(None);
    }
    Ok(Some(Diagnostic::new(
        sources,
        Severity::Error,
        codes::catalog()?.resolve(codes::STRING_LITERAL_TOO_LONG)?,
        "String literal exceeds the maximum UTF-8 byte length (2147483647)",
        span,
    )?))
}

/// Static runs are literal owners; interpolation joins are checked by the runtime.
pub(super) fn check_literal(
    sources: &SourceMap,
    span: Span,
    parts: &[StringPart],
) -> Result<Option<Diagnostic>, TypeCheckingError> {
    let mut length = 0;
    for part in parts {
        match part {
            StringPart::Text(text_span) => {
                let Some(part_length) = decoded_text_length(sources.slice(*text_span)?) else {
                    // The lexer already diagnoses invalid escape sequences.
                    continue;
                };
                length += part_length;
                if let Some(diagnostic) = length_diagnostic(sources, span, length)? {
                    return Ok(Some(diagnostic));
                }
            }
            StringPart::Interpolation { .. } | StringPart::Error(_) => length = 0,
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_byte_limit_preserves_boundary_and_rejects_overflow() {
        assert_eq!(checked_string_length(0, 0), Some(0));
        assert_eq!(
            checked_string_length(MAX_STRING_BYTE_LENGTH, 0),
            Some(MAX_STRING_BYTE_LENGTH)
        );
        assert_eq!(
            checked_string_length(MAX_STRING_BYTE_LENGTH - 1, 1),
            Some(MAX_STRING_BYTE_LENGTH)
        );
        assert_eq!(checked_string_length(MAX_STRING_BYTE_LENGTH, 1), None);
        assert_eq!(checked_string_length(u64::MAX, 1), None);
    }

    #[test]
    fn oversize_literal_diagnostic_keeps_source_span() {
        let mut sources = SourceMap::new();
        let source = sources.add_source("literal.ko", "\"中\\n\"").unwrap();
        let span = sources.span(source, 0, 7).unwrap();
        assert!(
            length_diagnostic(&sources, span, MAX_STRING_BYTE_LENGTH)
                .unwrap()
                .is_none()
        );
        let diagnostic = length_diagnostic(&sources, span, MAX_STRING_BYTE_LENGTH + 1)
            .unwrap()
            .expect("oversize literal");
        assert_eq!(diagnostic.code().to_string(), "L0168");
        assert_eq!(diagnostic.primary_span(), span);
    }

    #[test]
    fn decoded_length_counts_utf8_bytes_and_escapes() {
        for text in ["", "中文", r#"中\n\0\\\$"#] {
            assert_eq!(
                super::super::constant_value::decoded_text_length(text),
                super::super::constant_value::decode_text(text).map(|value| value.len() as u64)
            );
        }
    }
}
