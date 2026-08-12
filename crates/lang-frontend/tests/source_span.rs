//! SPEC-0002 的公开 source / span 契约测试。

use lang_frontend::source::{SourceError, SourceId, SourceMap, SourcePosition};

fn add_source(sources: &mut SourceMap, name: &str, text: &str) -> SourceId {
    sources
        .add_source(name, text)
        .expect("test source names are unique")
}

fn coordinates(position: SourcePosition) -> (usize, usize) {
    (position.line(), position.column())
}

#[test]
fn ascii_slice_preserves_half_open_multiline_range() {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "ascii.ko", "ab\ncd");
    let span = sources
        .span(source_id, 1, 4)
        .expect("1..4 is a valid ASCII range");

    assert_eq!(sources.source_name(source_id), Ok("ascii.ko"));
    assert_eq!(sources.source_text(source_id), Ok("ab\ncd"));
    assert_eq!(sources.slice(span), Ok("b\nc"));
    assert_eq!(span.source_id(), source_id);
    assert_eq!((span.start(), span.end(), span.len()), (1, 4, 3));
    assert!(!span.is_empty());
    assert_eq!(
        coordinates(
            sources
                .position(source_id, span.start())
                .expect("span start is a valid position")
        ),
        (1, 2)
    );
    assert_eq!(
        coordinates(
            sources
                .position(source_id, span.end())
                .expect("span end is a valid position")
        ),
        (2, 2)
    );
}

#[test]
fn utf8_columns_count_scalars_and_tab_once() {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "unicode.ko", "a界\tβ");
    let scalar_positions = [
        (0, (1, 1)),
        (1, (1, 2)),
        (4, (1, 3)),
        (5, (1, 4)),
        (7, (1, 5)),
    ];

    for (offset, expected) in scalar_positions {
        let actual = sources
            .position(source_id, offset)
            .map(coordinates)
            .expect("table only contains UTF-8 character boundaries");
        assert_eq!(actual, expected, "unexpected position at byte {offset}");
    }

    let character = sources
        .span(source_id, 1, 4)
        .expect("the three-byte character is a valid range");
    assert_eq!(sources.slice(character), Ok("界"));
}

#[test]
fn empty_source_and_eof_empty_ranges_are_valid() {
    let mut sources = SourceMap::new();
    let empty_id = add_source(&mut sources, "empty.ko", "");
    let text_id = add_source(&mut sources, "text.ko", "ok");
    let trailing_newline_id = add_source(&mut sources, "newline.ko", "ok\n");

    let empty = sources
        .span(empty_id, 0, 0)
        .expect("empty source has a valid EOF range");
    let text_eof = sources
        .span(text_id, 2, 2)
        .expect("EOF is a valid empty range");
    let newline_eof = sources
        .span(trailing_newline_id, 3, 3)
        .expect("EOF after a newline is a valid empty range");

    assert!(empty.is_empty());
    assert_eq!(sources.slice(empty), Ok(""));
    assert_eq!(sources.slice(text_eof), Ok(""));
    assert_eq!(sources.slice(newline_eof), Ok(""));
    assert_eq!(sources.position(empty_id, 0).map(coordinates), Ok((1, 1)));
    assert_eq!(sources.position(text_id, 2).map(coordinates), Ok((1, 3)));
    assert_eq!(
        sources.position(trailing_newline_id, 3).map(coordinates),
        Ok((2, 1))
    );
}

#[test]
fn lf_and_crlf_offsets_have_stable_positions() {
    let mut sources = SourceMap::new();
    let lf_id = add_source(&mut sources, "lf.ko", "a\nβ\n");
    let crlf_id = add_source(&mut sources, "crlf.ko", "a\r\nβ\r\n");
    let lf_positions = [
        (0, (1, 1)),
        (1, (1, 2)),
        (2, (2, 1)),
        (4, (2, 2)),
        (5, (3, 1)),
    ];
    let crlf_positions = [
        (0, (1, 1)),
        (1, (1, 2)),
        (2, (1, 3)),
        (3, (2, 1)),
        (5, (2, 2)),
        (6, (2, 3)),
        (7, (3, 1)),
    ];

    for (offset, expected) in lf_positions {
        assert_eq!(
            sources.position(lf_id, offset).map(coordinates),
            Ok(expected),
            "unexpected LF position at byte {offset}"
        );
    }
    for (offset, expected) in crlf_positions {
        assert_eq!(
            sources.position(crlf_id, offset).map(coordinates),
            Ok(expected),
            "unexpected CRLF position at byte {offset}"
        );
    }
}

#[test]
fn invalid_source_and_ranges_return_specific_errors() {
    let mut first_map = SourceMap::new();
    let source_id = add_source(&mut first_map, "first.ko", "abc");
    let span = first_map
        .span(source_id, 0, 1)
        .expect("range is valid in its owning map");
    let mut second_map = SourceMap::default();
    let second_id = add_source(&mut second_map, "second.ko", "xyz");

    assert_ne!(
        source_id, second_id,
        "IDs from different maps must not collide"
    );

    assert_eq!(
        second_map.source_name(source_id),
        Err(SourceError::InvalidSourceId { source_id })
    );
    assert_eq!(
        second_map.source_text(source_id),
        Err(SourceError::InvalidSourceId { source_id })
    );
    assert_eq!(
        second_map.position(source_id, 0),
        Err(SourceError::InvalidSourceId { source_id })
    );
    assert_eq!(
        second_map.span(source_id, 0, 1),
        Err(SourceError::InvalidSourceId { source_id })
    );
    assert_eq!(
        second_map.slice(span),
        Err(SourceError::InvalidSourceId { source_id })
    );
    assert_eq!(
        first_map.span(source_id, 2, 1),
        Err(SourceError::ReversedSpan {
            source_id,
            start: 2,
            end: 1,
        })
    );
    assert_eq!(
        first_map.span(source_id, 0, 4),
        Err(SourceError::SpanOutOfBounds {
            source_id,
            start: 0,
            end: 4,
            source_len: 3,
        })
    );
    assert_eq!(
        first_map.position(source_id, 4),
        Err(SourceError::OffsetOutOfBounds {
            source_id,
            offset: 4,
            source_len: 3,
        })
    );
}

#[test]
fn utf8_interior_offsets_are_rejected_without_panicking() {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "boundary.ko", "a界");

    assert_eq!(
        sources.position(source_id, 2),
        Err(SourceError::NotCharBoundary {
            source_id,
            offset: 2,
        })
    );
    assert_eq!(
        sources.span(source_id, 2, 4),
        Err(SourceError::NotCharBoundary {
            source_id,
            offset: 2,
        })
    );
    assert_eq!(
        sources.span(source_id, 1, 3),
        Err(SourceError::NotCharBoundary {
            source_id,
            offset: 3,
        })
    );
}

#[test]
fn repeated_position_queries_are_deterministic() {
    let mut sources = SourceMap::new();
    let source_id = add_source(&mut sources, "repeat.ko", "α\nvalue");
    let expected = sources
        .position(source_id, 4)
        .expect("offset 4 starts the second scalar on line two");
    add_source(&mut sources, "other.ko", "unrelated");

    for _ in 0..16 {
        assert_eq!(sources.position(source_id, 4), Ok(expected));
    }
}

#[test]
fn source_ids_remain_stable_when_more_sources_are_added() {
    let mut sources = SourceMap::new();
    let first_id = add_source(&mut sources, "first.ko", "first");
    let second_id = add_source(&mut sources, "second.ko", "second");

    assert_ne!(first_id, second_id);
    assert_eq!(sources.source_name(first_id), Ok("first.ko"));
    assert_eq!(sources.source_text(first_id), Ok("first"));
    assert_eq!(sources.source_name(second_id), Ok("second.ko"));
}

#[test]
fn duplicate_source_names_are_rejected_without_mutating_the_first_source() {
    let mut sources = SourceMap::new();
    let first_id = add_source(&mut sources, "same.ko", "first");

    assert_eq!(
        sources.add_source("same.ko", "replacement"),
        Err(SourceError::DuplicateSourceName {
            name: String::from("same.ko"),
        })
    );
    assert_eq!(sources.source_text(first_id), Ok("first"));
}
