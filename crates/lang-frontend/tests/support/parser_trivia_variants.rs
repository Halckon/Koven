//! SPEC-0101 的非换行 trivia 插入分段契约。

use lang_frontend::{
    lexer::{LexedFile, LexemeKind, TriviaKind},
    source::SourceMap,
};

#[derive(Clone, Copy)]
pub(crate) struct TriviaVariant {
    pub(crate) text: &'static str,
    fragments: &'static [(TriviaKind, &'static str)],
}

pub(crate) const TRIVIA_VARIANTS: &[TriviaVariant] = &[
    TriviaVariant {
        text: "\t",
        fragments: &[(TriviaKind::Whitespace, "\t")],
    },
    TriviaVariant {
        text: "/*c*/",
        fragments: &[(TriviaKind::BlockComment, "/*c*/")],
    },
    TriviaVariant {
        text: " \t/*c*/ ",
        fragments: &[
            (TriviaKind::Whitespace, " \t"),
            (TriviaKind::BlockComment, "/*c*/"),
            (TriviaKind::Whitespace, " "),
        ],
    },
];

pub(crate) fn validate_inserted_trivia(
    sources: &SourceMap,
    lexed: &LexedFile,
    start: usize,
    variant: TriviaVariant,
    context: &str,
) {
    let end = start + variant.text.len();
    let actual = lexed
        .lexemes()
        .iter()
        .filter_map(|lexeme| {
            let span = lexeme.span();
            let overlap_start = span.start().max(start);
            let overlap_end = span.end().min(end);
            if overlap_start >= overlap_end {
                return None;
            }
            let LexemeKind::Trivia(kind) = lexeme.kind() else {
                panic!(
                    "inserted trivia produced non-trivia {:?} for {context}",
                    lexeme.kind()
                );
            };
            let source = sources
                .source_text(span.source_id())
                .unwrap_or_else(|error| {
                    panic!("invalid trivia source identity for {context}: {error}")
                });
            let text = &source[overlap_start..overlap_end];
            Some((kind, text))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual, variant.fragments,
        "inserted trivia lexeme drift for {context}"
    );
}
