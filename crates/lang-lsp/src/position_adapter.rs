//! `SourceMap` UTF-8 byte position与 LSP UTF-16 position 的集中适配。

use std::{error::Error, fmt};

use lang_frontend::source::{SourceError, SourceId, SourceMap, Span};
use lsp_types::{Position, Range};

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
    let source_position = sources.position(source_id, offset)?;
    let text = sources.source_text(source_id)?;
    let scalar_column = source_position.column() - 1;
    // SourceMap owns line/CRLF semantics. Walking exactly its reported scalar column only adapts
    // that column to the UTF-16 code units required by the LSP boundary.
    let utf16_column = text[..offset]
        .chars()
        .rev()
        .take(scalar_column)
        .map(char::len_utf16)
        .sum::<usize>();

    Ok(Position::new(
        u32::try_from(source_position.line() - 1)
            .map_err(|_| PositionMappingError::PositionOverflow)?,
        u32::try_from(utf16_column).map_err(|_| PositionMappingError::PositionOverflow)?,
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
    let text = sources.source_text(source_id)?;
    let target_line =
        usize::try_from(position.line).map_err(|_| PositionMappingError::PositionOverflow)?;
    let target_character =
        usize::try_from(position.character).map_err(|_| PositionMappingError::PositionOverflow)?;

    let mut line_start = 0;
    for _ in 0..target_line {
        let Some(newline) = text[line_start..].find('\n') else {
            return Ok(None);
        };
        line_start += newline + 1;
    }
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |newline| line_start + newline);
    let content_end = if text.as_bytes().get(line_end.wrapping_sub(1)) == Some(&b'\r') {
        line_end - 1
    } else {
        line_end
    };

    let mut utf16_column = 0;
    for (relative, character) in text[line_start..content_end].char_indices() {
        if utf16_column == target_character {
            return Ok(Some(line_start + relative));
        }
        utf16_column += character.len_utf16();
        if utf16_column > target_character {
            return Err(PositionMappingError::InsideSurrogatePair);
        }
    }
    Ok((utf16_column == target_character).then_some(content_end))
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
