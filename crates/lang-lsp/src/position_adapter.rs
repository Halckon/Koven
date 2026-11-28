//! `SourceMap` UTF-8 byte position与 LSP UTF-16 position 的集中适配。

use std::{error::Error, fmt};

use lang_frontend::source::{SourceError, SourceId, SourceMap, Span, Utf16Offset};
use lsp_types::{Position, Range};

pub(crate) fn span_range(sources: &SourceMap, span: Span) -> Result<Range, PositionMappingError> {
    Ok(Range::new(
        utf16_position(sources, span.source_id(), span.start())?,
        utf16_position(sources, span.source_id(), span.end())?,
    ))
}

/// 将 UTF-8 字节偏移转换为 LSP 0-based UTF-16 位置（按 LSP 规范统一识别 CR、LF、CRLF 行尾）。
pub(crate) fn utf16_position(
    sources: &SourceMap,
    source_id: SourceId,
    offset: usize,
) -> Result<Position, PositionMappingError> {
    let (line, column) = sources.position_utf16(source_id, offset)?;
    let text = sources.source_text(source_id)?;
    let (line, column) = if has_standalone_cr(text.as_bytes()) {
        lsp_position_utf16(text, offset)
    } else {
        (line, column)
    };
    Ok(Position::new(
        u32::try_from(line).map_err(|_| PositionMappingError::PositionOverflow)?,
        u32::try_from(column).map_err(|_| PositionMappingError::PositionOverflow)?,
    ))
}

/// 把打开 buffer 中的 0-based UTF-16 cursor 转回 UTF-8 byte boundary。
///
/// 不存在的行或超过行内容的 character 返回 `Ok(None)`；surrogate pair 中间位置是无效 LSP
/// position，返回具体错误。CR、LF 和 CRLF terminator 均不属于可寻址行内容。
pub(crate) fn byte_offset(
    sources: &SourceMap,
    source_id: SourceId,
    position: Position,
) -> Result<Option<usize>, PositionMappingError> {
    let line =
        usize::try_from(position.line).map_err(|_| PositionMappingError::PositionOverflow)?;
    let column =
        usize::try_from(position.character).map_err(|_| PositionMappingError::PositionOverflow)?;
    let text = sources.source_text(source_id)?;
    let mapped = if has_standalone_cr(text.as_bytes()) {
        lsp_byte_offset_utf16(text, line, column)
    } else {
        sources.byte_offset_utf16(source_id, line, column)?
    };
    match mapped {
        Utf16Offset::Byte(offset) => Ok(Some(offset)),
        Utf16Offset::OutOfRange => Ok(None),
        Utf16Offset::InsideSurrogatePair => Err(PositionMappingError::InsideSurrogatePair),
    }
}

fn has_standalone_cr(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .enumerate()
        .any(|(idx, &b)| b == b'\r' && bytes.get(idx + 1) != Some(&b'\n'))
}

/// 按 LSP 行尾规则（`\r\n`、`\r`、`\n`）迭代每一行的 `(line_start, content_end, next_line_start)`。
fn next_lsp_line(bytes: &[u8], start: usize) -> (usize, usize) {
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

fn lsp_position_utf16(text: &str, offset: usize) -> (usize, usize) {
    let bytes = text.as_bytes();
    let mut line = 0usize;
    let mut line_start = 0usize;
    loop {
        let (content_end, next_start) = next_lsp_line(bytes, line_start);
        if next_start == content_end || offset < next_start {
            let column = text[line_start..offset].encode_utf16().count();
            return (line, column);
        }
        line += 1;
        line_start = next_start;
    }
}

fn lsp_byte_offset_utf16(text: &str, target_line: usize, target_column: usize) -> Utf16Offset {
    let bytes = text.as_bytes();
    let mut line_start = 0usize;
    for _ in 0..target_line {
        let (content_end, next_start) = next_lsp_line(bytes, line_start);
        if next_start == content_end {
            return Utf16Offset::OutOfRange;
        }
        line_start = next_start;
    }
    let (content_end, _) = next_lsp_line(bytes, line_start);
    let mut utf16 = 0usize;
    for (relative, character) in text[line_start..content_end].char_indices() {
        if utf16 == target_column {
            return Utf16Offset::Byte(line_start + relative);
        }
        utf16 += character.len_utf16();
        if utf16 > target_column {
            return Utf16Offset::InsideSurrogatePair;
        }
    }
    if utf16 == target_column {
        Utf16Offset::Byte(content_end)
    } else {
        Utf16Offset::OutOfRange
    }
}

#[derive(Debug)]
pub(crate) enum PositionMappingError {
    Source(SourceError),
    PositionOverflow,
    InsideSurrogatePair,
}

impl fmt::Display for PositionMappingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "invalid source position: {error}"),
            Self::PositionOverflow => formatter.write_str("source position exceeds LSP u32 range"),
            Self::InsideSurrogatePair => {
                formatter.write_str("LSP position splits a UTF-16 surrogate pair")
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

    use super::{PositionMappingError, byte_offset, span_range, utf16_position};

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
    fn maps_standalone_cr_mixed_with_crlf_and_lf_in_both_directions() {
        let mut sources = SourceMap::new();
        // Trailing lone CR: EOF is (1, 0), and (0, 2) is out of range (CR is a line terminator).
        let lone_cr = sources.add_source("lone-cr.ko", "x\r").expect("source");
        assert_eq!(
            utf16_position(&sources, lone_cr, 0).expect("pos 0"),
            Position::new(0, 0)
        );
        assert_eq!(
            utf16_position(&sources, lone_cr, 1).expect("pos 1"),
            Position::new(0, 1)
        );
        assert_eq!(
            utf16_position(&sources, lone_cr, 2).expect("EOF"),
            Position::new(1, 0)
        );
        assert_eq!(
            byte_offset(&sources, lone_cr, Position::new(0, 1)).expect("line 0 end"),
            Some(1)
        );
        assert_eq!(
            byte_offset(&sources, lone_cr, Position::new(0, 2)).expect("past CR"),
            None
        );
        assert_eq!(
            byte_offset(&sources, lone_cr, Position::new(1, 0)).expect("EOF"),
            Some(2)
        );

        // Mixed CR, CRLF, LF with surrogate pair:
        // "a😀\r" -> [0..6], "b\r\n" -> [6..9], "c\n" -> [9..11], EOF -> 11
        let mixed = sources
            .add_source("mixed.ko", "a😀\rb\r\nc\n")
            .expect("source");
        for (offset, pos) in [
            (0, Position::new(0, 0)),
            (1, Position::new(0, 1)),
            (5, Position::new(0, 3)),
            (6, Position::new(1, 0)),
            (7, Position::new(1, 1)),
            (9, Position::new(2, 0)),
            (10, Position::new(2, 1)),
            (11, Position::new(3, 0)),
        ] {
            assert_eq!(
                utf16_position(&sources, mixed, offset).expect("forward"),
                pos,
                "offset {offset}"
            );
            assert_eq!(
                byte_offset(&sources, mixed, pos).expect("reverse"),
                Some(offset),
                "pos {pos:?}"
            );
        }
        assert!(matches!(
            byte_offset(&sources, mixed, Position::new(0, 2)),
            Err(PositionMappingError::InsideSurrogatePair)
        ));
        assert_eq!(
            byte_offset(&sources, mixed, Position::new(0, 4)).expect("CR column"),
            None
        );
        assert_eq!(
            byte_offset(&sources, mixed, Position::new(4, 0)).expect("past EOF line"),
            None
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
