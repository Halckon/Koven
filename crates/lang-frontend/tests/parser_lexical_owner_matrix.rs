//! SPEC-0075 的 lexical owner 到代表性语法位置恢复矩阵契约。

use lang_frontend::{
    lexer::{LexedFile, LexemeKind, lex},
    parser::{Item, NameMarker, ParsedFile, parse_file},
    source::{SourceId, SourceMap},
};

#[derive(Clone, Copy)]
struct Placement {
    name: &'static str,
    prefix: &'static str,
    suffix: &'static str,
}

#[derive(Clone, Copy)]
struct Owner {
    name: &'static str,
    text: &'static str,
    lexical_codes: &'static [&'static str],
}

const PLACEMENTS: &[Placement] = &[
    Placement {
        name: "top-level declaration",
        prefix: "",
        suffix: "",
    },
    Placement {
        name: "after modifier",
        prefix: "public ",
        suffix: "",
    },
    Placement {
        name: "variable name",
        prefix: "val ",
        suffix: " = 0",
    },
    Placement {
        name: "variable type",
        prefix: "val x: ",
        suffix: " = 0",
    },
    Placement {
        name: "initializer",
        prefix: "val x = ",
        suffix: "",
    },
    Placement {
        name: "function name",
        prefix: "fun ",
        suffix: "() {}",
    },
    Placement {
        name: "parameter name",
        prefix: "fun f(",
        suffix: ": T) {}",
    },
    Placement {
        name: "parameter type",
        prefix: "fun f(x: ",
        suffix: ") {}",
    },
    Placement {
        name: "return type",
        prefix: "fun f(): ",
        suffix: " {}",
    },
    Placement {
        name: "class name",
        prefix: "class ",
        suffix: " {}",
    },
    Placement {
        name: "field name",
        prefix: "class C(val ",
        suffix: ": T) {}",
    },
    Placement {
        name: "field type",
        prefix: "class C(val x: ",
        suffix: ") {}",
    },
    Placement {
        name: "supertype",
        prefix: "class C : ",
        suffix: " {}",
    },
    Placement {
        name: "member",
        prefix: "class C {\n",
        suffix: "\nfun ok() {}\n}",
    },
    Placement {
        name: "enum variant",
        prefix: "enum class E {\n",
        suffix: ", Ok\n}",
    },
    Placement {
        name: "call argument",
        prefix: "val x = f(",
        suffix: ", next)",
    },
];

const RECOVERABLE_OWNERS: &[Owner] = &[
    Owner {
        name: "complete string",
        text: r#""text""#,
        lexical_codes: &[],
    },
    Owner {
        name: "complete interpolation",
        text: r#""${value}""#,
        lexical_codes: &[],
    },
    Owner {
        name: "closed string with invalid escape",
        text: r#""a\qz""#,
        lexical_codes: &["L0006"],
    },
    Owner {
        name: "newline-terminated string",
        text: "\"abc\n",
        lexical_codes: &["L0004"],
    },
];

const TERMINAL_OWNERS: &[Owner] = &[
    Owner {
        name: "unterminated string",
        text: r#""abc"#,
        lexical_codes: &["L0004"],
    },
    Owner {
        name: "unterminated interpolation",
        text: r#""${value"#,
        lexical_codes: &["L0005"],
    },
    Owner {
        name: "terminal escape",
        text: "\"abc\\",
        lexical_codes: &["L0006"],
    },
    Owner {
        name: "unterminated char",
        text: "'",
        lexical_codes: &["L0007"],
    },
    Owner {
        name: "unterminated block comment",
        text: "/*",
        lexical_codes: &["L0003"],
    },
];

fn lex_case(text: &str) -> (SourceMap, SourceId, LexedFile) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-lexical-owner-matrix.ko", text)
        .expect("matrix source name must be unique");
    let lexed = lex(&sources, source_id).expect("matrix source must lex internally");
    (sources, source_id, lexed)
}

fn assert_lexeme_coverage(source_id: SourceId, source_len: usize, lexed: &LexedFile) {
    let mut next_offset = 0;
    let mut eof_count = 0;
    for lexeme in lexed.lexemes() {
        let span = lexeme.span();
        assert_eq!(span.source_id(), source_id);
        assert_eq!(span.start(), next_offset);
        if lexeme.kind() == LexemeKind::Eof {
            eof_count += 1;
            assert_eq!((span.start(), span.end()), (source_len, source_len));
        } else {
            assert!(span.end() > span.start());
            next_offset = span.end();
        }
    }
    assert_eq!(next_offset, source_len);
    assert_eq!(eof_count, 1);
}

fn lexical_codes(lexed: &LexedFile) -> Vec<String> {
    lexed
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

fn assert_after_survives(sources: &SourceMap, parsed: &ParsedFile, context: &str) {
    let found = parsed.roots().iter().any(|root| {
        let Ok(node) = parsed.ast().items().get(*root) else {
            return false;
        };
        let Item::Variable {
            name: NameMarker::Present(span),
            ..
        } = node.payload()
        else {
            return false;
        };
        sources.slice(*span).is_ok_and(|name| name == "after")
    });
    assert!(
        found,
        "sentinel declaration did not survive {context}: diagnostics={:?}",
        parsed.diagnostics()
    );
}

fn parse_twice(sources: &SourceMap, lexed: &LexedFile, context: &str) -> ParsedFile {
    let first = parse_file(sources, lexed)
        .unwrap_or_else(|error| panic!("first parse failed for {context}: {error}"));
    let second = parse_file(sources, lexed)
        .unwrap_or_else(|error| panic!("second parse failed for {context}: {error}"));
    assert_eq!(
        format!("{first:?}"),
        format!("{second:?}"),
        "public parser output changed for {context}"
    );
    first
}

#[test]
fn recoverable_owners_preserve_the_following_top_level_declaration() {
    assert_eq!(PLACEMENTS.len(), 16);
    assert_eq!(RECOVERABLE_OWNERS.len(), 4);

    let mut executed = 0;
    for placement in PLACEMENTS {
        for owner in RECOVERABLE_OWNERS {
            let context = format!("{} at {}", owner.name, placement.name);
            let text = format!(
                "{}{}{}\nval after = 1",
                placement.prefix, owner.text, placement.suffix
            );
            let (sources, source_id, lexed) = lex_case(&text);
            assert_lexeme_coverage(source_id, text.len(), &lexed);
            assert_eq!(
                lexical_codes(&lexed),
                owner.lexical_codes,
                "unexpected lexer diagnostics for {context}"
            );

            let parsed = parse_twice(&sources, &lexed, &context);
            assert_after_survives(&sources, &parsed, &context);
            executed += 1;
        }
    }
    assert_eq!(executed, 64);
}

#[test]
fn terminal_owners_are_total_at_every_representative_placement() {
    assert_eq!(PLACEMENTS.len(), 16);
    assert_eq!(TERMINAL_OWNERS.len(), 5);

    let mut executed = 0;
    for placement in PLACEMENTS {
        for owner in TERMINAL_OWNERS {
            let context = format!("{} at {}", owner.name, placement.name);
            let text = format!("{}{}", placement.prefix, owner.text);
            let (sources, source_id, lexed) = lex_case(&text);
            assert_lexeme_coverage(source_id, text.len(), &lexed);
            assert_eq!(
                lexical_codes(&lexed),
                owner.lexical_codes,
                "unexpected lexer diagnostics for {context}"
            );

            parse_twice(&sources, &lexed, &context);
            executed += 1;
        }
    }
    assert_eq!(executed, 80);
}
