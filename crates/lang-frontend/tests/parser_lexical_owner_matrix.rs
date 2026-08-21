//! SPEC-0075 / SPEC-0095 / SPEC-0107 的 lexical owner 位置恢复与公开产物契约。

use lang_frontend::{
    lexer::LexedFile,
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
};

#[path = "support/frontend_output_assertions.rs"]
mod frontend_output_assertions;
#[path = "support/lexer_matrix_assertions.rs"]
mod lexer_matrix_assertions;
#[path = "support/parser_mutation_assertions.rs"]
mod parser_mutation_assertions;

use frontend_output_assertions::{validate_ast, validate_diagnostics, validate_lexed};
use lexer_matrix_assertions::lex_source_twice;
use parser_mutation_assertions::assert_last_root_source;

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

fn lex_case(text: &str, context: &str) -> (SourceMap, SourceId, LexedFile) {
    lex_source_twice(
        "parser-lexical-owner-matrix.ko",
        text,
        context,
        validate_lexed,
    )
}

fn lexical_codes(lexed: &LexedFile) -> Vec<String> {
    lexed
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

fn validate_parsed(source_id: SourceId, source_len: usize, parsed: &ParsedFile, context: &str) {
    validate_ast(source_id, source_len, parsed.ast());
    validate_diagnostics(source_id, source_len, parsed.diagnostics());
    for root in parsed.roots() {
        parsed
            .ast()
            .items()
            .get(*root)
            .unwrap_or_else(|error| panic!("invalid file root for {context}: {error}"));
    }
}

fn parse_twice(
    sources: &SourceMap,
    source_id: SourceId,
    source_len: usize,
    lexed: &LexedFile,
    context: &str,
) -> (ParsedFile, ParsedFile) {
    let first = parse_file(sources, lexed)
        .unwrap_or_else(|error| panic!("first parse failed for {context}: {error}"));
    let second = parse_file(sources, lexed)
        .unwrap_or_else(|error| panic!("second parse failed for {context}: {error}"));
    validate_parsed(source_id, source_len, &first, context);
    validate_parsed(source_id, source_len, &second, context);
    assert_eq!(
        format!("{first:?}"),
        format!("{second:?}"),
        "public parser output changed for {context}"
    );
    (first, second)
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
            let (sources, source_id, lexed) = lex_case(&text, &context);
            assert_eq!(
                lexical_codes(&lexed),
                owner.lexical_codes,
                "unexpected lexer diagnostics for {context}"
            );

            let (first, second) = parse_twice(&sources, source_id, text.len(), &lexed, &context);
            assert_last_root_source(&sources, &first, "val after = 1", &context);
            assert_last_root_source(&sources, &second, "val after = 1", &context);
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
            let (sources, source_id, lexed) = lex_case(&text, &context);
            assert_eq!(
                lexical_codes(&lexed),
                owner.lexical_codes,
                "unexpected lexer diagnostics for {context}"
            );

            parse_twice(&sources, source_id, text.len(), &lexed, &context);
            executed += 1;
        }
    }
    assert_eq!(executed, 80);
}
