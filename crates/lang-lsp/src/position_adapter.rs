//! `SourceMap` UTF-8 byte position与 LSP UTF-16 position 的集中适配。

use std::{error::Error, fmt};

use lang_frontend::source::{SourceError, SourceId, SourceMap, Span, Utf16Offset};
use lsp_types::{Position, Range, TextDocumentContentChangeEvent};

pub(crate) fn span_range(sources: &SourceMap, span: Span) -> Result<Range, PositionMappingError> {
    Ok(Range::new(
        utf16_position(sources, span.source_id(), span.start())?,
        utf16_position(sources, span.source_id(), span.end())?,
    ))
}

pub(crate) fn utf16_position(
    sources: &SourceMap,
    source_id: SourceId,
    offset: usize,
) -> Result<Position, PositionMappingError> {
    let (line, column) = sources.position_utf16(source_id, offset)?;
    Ok(Position::new(
        u32::try_from(line).map_err(|_| PositionMappingError::PositionOverflow)?,
        u32::try_from(column).map_err(|_| PositionMappingError::PositionOverflow)?,
    ))
}

/// 把打开 buffer 中的 0-based UTF-16 cursor 转回 UTF-8 byte boundary。
///
/// 不存在的行或超过行内容的 character 返回 `Ok(None)`；surrogate pair 中间位置是无效 LSP
/// position，返回具体错误。LF/CRLF terminator 不属于可寻址行内容。
pub(crate) fn byte_offset(
    sources: &SourceMap,
    source_id: SourceId,
    position: Position,
) -> Result<Option<usize>, PositionMappingError> {
    let line =
        usize::try_from(position.line).map_err(|_| PositionMappingError::PositionOverflow)?;
    let column =
        usize::try_from(position.character).map_err(|_| PositionMappingError::PositionOverflow)?;
    match sources.byte_offset_utf16(source_id, line, column)? {
        Utf16Offset::Byte(offset) => Ok(Some(offset)),
        Utf16Offset::OutOfRange => Ok(None),
        Utf16Offset::InsideSurrogatePair => Err(PositionMappingError::InsideSurrogatePair),
    }
}

/// 按 LSP 3.17 顺序将一组全量或增量 `TextDocumentContentChangeEvent` 应用到文档文本。
pub(crate) fn apply_content_changes(
    initial: &str,
    changes: &[TextDocumentContentChangeEvent],
) -> Result<String, PositionMappingError> {
    let mut text = initial.to_owned();
    for change in changes {
        let Some(range) = change.range else {
            text.clone_from(&change.text);
            continue;
        };
        let start_byte = utf16_pos_in_text(&text, range.start)?;
        let end_byte = utf16_pos_in_text(&text, range.end)?;
        if start_byte > end_byte {
            return Err(PositionMappingError::InvalidChangeRange);
        }
        text.replace_range(start_byte..end_byte, &change.text);
    }
    Ok(text)
}

fn utf16_pos_in_text(text: &str, pos: Position) -> Result<usize, PositionMappingError> {
    let target_line =
        usize::try_from(pos.line).map_err(|_| PositionMappingError::PositionOverflow)?;
    let target_col =
        usize::try_from(pos.character).map_err(|_| PositionMappingError::PositionOverflow)?;
    let bytes = text.as_bytes();
    let mut line_start = 0usize;
    for _ in 0..target_line {
        let (content_end, next_start) = next_line_bounds(bytes, line_start);
        if next_start == content_end {
            return Err(PositionMappingError::InvalidChangeRange);
        }
        line_start = next_start;
    }
    let (content_end, _) = next_line_bounds(bytes, line_start);
    let mut utf16 = 0usize;
    for (rel, ch) in text[line_start..content_end].char_indices() {
        if utf16 == target_col {
            return Ok(line_start + rel);
        }
        utf16 += ch.len_utf16();
        if utf16 > target_col {
            return Err(PositionMappingError::InsideSurrogatePair);
        }
    }
    if utf16 == target_col {
        Ok(content_end)
    } else {
        Err(PositionMappingError::InvalidChangeRange)
    }
}

fn next_line_bounds(bytes: &[u8], start: usize) -> (usize, usize) {
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' => {
                let next = if bytes.get(i + 1) == Some(&b'\n') {
                    i + 2
                } else {
                    i + 1
                };
                return (i, next);
            }
            b'\n' => return (i, i + 1),
            _ => i += 1,
        }
    }
    (bytes.len(), bytes.len())
}

#[derive(Debug)]
pub(crate) enum PositionMappingError {
    Source(SourceError),
    PositionOverflow,
    InsideSurrogatePair,
    InvalidChangeRange,
}

impl fmt::Display for PositionMappingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "invalid source position: {error}"),
            Self::PositionOverflow => formatter.write_str("source position exceeds LSP u32 range"),
            Self::InsideSurrogatePair => {
                formatter.write_str("LSP position splits a UTF-16 surrogate pair")
            }
            Self::InvalidChangeRange => {
                formatter.write_str("incremental change range is outside document bounds")
            }
        }
    }
}

impl Error for PositionMappingError {}

impl From<SourceError> for PositionMappingError {
    fn from(error: SourceError) -> Self {
        Self::Source(error)
    }
}

#[cfg(test)]
mod tests {
    use lang_frontend::source::SourceMap;
    use lsp_types::{Position, Range};

    use super::{PositionMappingError, byte_offset, span_range};

    #[test]
    fn maps_utf8_and_utf16_both_directions_across_crlf_and_eof() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source("unicode.ko", "a😀b\r\nnext\n")
            .expect("source");
        let emoji_to_b = sources.span(source, 1, 6).expect("span");
        assert_eq!(
            span_range(&sources, emoji_to_b).expect("range"),
            Range::new(Position::new(0, 1), Position::new(0, 4))
        );
        assert_eq!(
            byte_offset(&sources, source, Position::new(0, 3)).expect("offset"),
            Some(5)
        );
        assert_eq!(
            byte_offset(&sources, source, Position::new(1, 4)).expect("offset"),
            Some(12)
        );
        assert_eq!(
            byte_offset(&sources, source, Position::new(2, 0)).expect("EOF line"),
            Some(13)
        );
    }

    #[test]
    fn rejects_surrogate_middle_and_returns_none_for_out_of_range_positions() {
        let mut sources = SourceMap::new();
        let source = sources.add_source("unicode.ko", "😀\r\n").expect("source");
        assert!(matches!(
            byte_offset(&sources, source, Position::new(0, 1)),
            Err(PositionMappingError::InsideSurrogatePair)
        ));
        assert_eq!(
            byte_offset(&sources, source, Position::new(0, 3)).expect("column"),
            None
        );
        assert_eq!(
            byte_offset(&sources, source, Position::new(2, 0)).expect("line"),
            None
        );
    }
}
