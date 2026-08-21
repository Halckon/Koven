//! SPEC-0100 的结构性换行矩阵共享 carrier 词法分段契约。

use lang_frontend::{
    lexer::{LexedFile, LexemeKind, TriviaKind},
    source::SourceMap,
};

#[derive(Clone, Copy)]
pub(crate) struct Carrier {
    pub(crate) text: &'static str,
    trivia: &'static [(TriviaKind, &'static str)],
}

pub(crate) const STRUCTURAL_BREAKS: &[Carrier] = &[
    Carrier {
        text: "\n",
        trivia: &[(TriviaKind::Newline, "\n")],
    },
    Carrier {
        text: "\r\n",
        trivia: &[(TriviaKind::Newline, "\r\n")],
    },
    Carrier {
        text: "//c\n",
        trivia: &[
            (TriviaKind::LineComment, "//c"),
            (TriviaKind::Newline, "\n"),
        ],
    },
    Carrier {
        text: "//c\r\n",
        trivia: &[
            (TriviaKind::LineComment, "//c"),
            (TriviaKind::Newline, "\r\n"),
        ],
    },
    Carrier {
        text: "/*c\nc*/",
        trivia: &[(TriviaKind::BlockComment, "/*c\nc*/")],
    },
    Carrier {
        text: "/*c\r\nc*/",
        trivia: &[(TriviaKind::BlockComment, "/*c\r\nc*/")],
    },
];

pub(crate) const NON_BREAK_TRIVIA: &[Carrier] = &[
    Carrier {
        text: " ",
        trivia: &[(TriviaKind::Whitespace, " ")],
    },
    Carrier {
        text: "\t",
        trivia: &[(TriviaKind::Whitespace, "\t")],
    },
    Carrier {
        text: "/*c*/",
        trivia: &[(TriviaKind::BlockComment, "/*c*/")],
    },
    Carrier {
        text: "/*c\rc*/",
        trivia: &[(TriviaKind::BlockComment, "/*c\rc*/")],
    },
];

pub(crate) fn validate_carrier_lexemes(
    sources: &SourceMap,
    lexed: &LexedFile,
    carrier_start: usize,
    carrier: Carrier,
    context: &str,
) {
    let carrier_end = carrier_start + carrier.text.len();
    let actual = lexed
        .lexemes()
        .iter()
        .filter(|lexeme| {
            let span = lexeme.span();
            span.start() >= carrier_start && span.end() <= carrier_end && span.start() < span.end()
        })
        .map(|lexeme| {
            let LexemeKind::Trivia(kind) = lexeme.kind() else {
                panic!(
                    "carrier produced non-trivia {:?} for {context}",
                    lexeme.kind()
                );
            };
            let text = sources
                .slice(lexeme.span())
                .unwrap_or_else(|error| panic!("invalid carrier span for {context}: {error}"));
            (kind, text)
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, carrier.trivia, "carrier lexeme drift for {context}");
}
