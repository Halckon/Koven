//! SPEC-0077 / SPEC-0097 / SPEC-0109 的非换行 trivia 等价与公开产物不变量。

use std::mem::{Discriminant, discriminant};

use lang_frontend::{
    lexer::LexemeKind,
    parser::{Expression, Item, ParsedFile, Statement, TypeRef, parse_file},
    source::{SourceId, Span},
};

#[path = "support/frontend_output_assertions.rs"]
mod frontend_output_assertions;
#[path = "support/lexer_matrix_assertions.rs"]
mod lexer_matrix_assertions;

use frontend_output_assertions::{validate_ast, validate_diagnostics, validate_lexed};
use lexer_matrix_assertions::lex_source_twice;

#[derive(Clone, Copy)]
struct GrammarCase {
    name: &'static str,
    tokens: &'static [&'static str],
}

const GRAMMAR_CASES: &[GrammarCase] = &[
    GrammarCase {
        name: "file header",
        tokens: &[
            "package", "demo", ".", "core", ";", "import", "lib", ".", "*", ";", "val", "answer",
            "=", "42",
        ],
    },
    GrammarCase {
        name: "variables and constant",
        tokens: &[
            "public", "val", "answer", ":", "Int", "=", "42", ";", "var", "counter", ":", "Int",
            "=", "0", ";", "const", "val", "limit", ":", "Int", "=", "10",
        ],
    },
    GrammarCase {
        name: "generic function",
        tokens: &[
            "fun", "<", "T", ":", "Copyable", ">", "id", "(", "borrow", "item", ":", "T", ")", ":",
            "T", "=", "item",
        ],
    },
    GrammarCase {
        name: "function type",
        tokens: &[
            "val", "handler", ":", "move", "(", "borrow", "Int", ",", "inout", "String", ")", "->",
            "Unit", "=", "target",
        ],
    },
    GrammarCase {
        name: "typed named mode call",
        tokens: &[
            "val", "result", "=", "service", ".", "send", "<", "Int", ">", "(", "name", "=",
            "input", ",", "&", "target", ")",
        ],
    },
    GrammarCase {
        name: "move lambda",
        tokens: &[
            "val", "combine", "=", "move", "{", "left", ",", "right", "->", "left", "+", "right",
            "}",
        ],
    },
    GrammarCase {
        name: "local destructuring",
        tokens: &[
            "fun", "usePair", "(", ")", "{", "val", "(", "first", ",", "second", ")", "=", "pair",
            "}",
        ],
    },
    GrammarCase {
        name: "if expression",
        tokens: &[
            "val", "choice", "=", "if", "(", "ready", ")", "yes", "else", "no",
        ],
    },
    GrammarCase {
        name: "when expression",
        tokens: &[
            "val", "choice", "=", "when", "(", "input", ")", "{", "is", "Type", "->", "yes", ";",
            "!is", "Other", "->", "no", ";", "else", "->", "fallback", "}",
        ],
    },
    GrammarCase {
        name: "loop family",
        tokens: &[
            "fun", "loops", "(", ")", "{", "while", "(", "ready", ")", "{", "continue", "}", "for",
            "(", "item", "in", "items", ")", "{", "break", "}", "loop", "{", "return", "}", "}",
        ],
    },
    GrammarCase {
        name: "postfix chain",
        tokens: &["val", "result", "=", "source", "?.", "member", "!!", "?"],
    },
    GrammarCase {
        name: "ordinary class",
        tokens: &[
            "public",
            "class",
            "Box",
            "<",
            "T",
            ":",
            "Copyable",
            ">",
            "(",
            "private",
            "val",
            "item",
            ":",
            "T",
            ",",
            "var",
            "count",
            ":",
            "Int",
            ")",
            ":",
            "Printable",
            "{",
            "override",
            "fun",
            "show",
            "(",
            ")",
            ":",
            "Unit",
            "{",
            "}",
            "}",
        ],
    },
    GrammarCase {
        name: "interface companion",
        tokens: &[
            "interface",
            "Printable",
            "{",
            "fun",
            "show",
            "(",
            ")",
            ":",
            "Unit",
            ";",
            "companion",
            "object",
            "{",
            "const",
            "val",
            "NAME",
            ":",
            "String",
            "=",
            r#""printable""#,
            "}",
            "}",
        ],
    },
    GrammarCase {
        name: "enum variants and member",
        tokens: &[
            "enum", "class", "Result", "<", "T", ">", "{", "Ok", "(", "payload", ":", "T", ")",
            ",", "Error", "(", "message", ":", "String", ")", ";", "fun", "isOk", "(", ")", ":",
            "Boolean", "=", "true", "}",
        ],
    },
    GrammarCase {
        name: "named object",
        tokens: &[
            "object", "Config", "{", "const", "val", "VERSION", ":", "Int", "=", "1", ";", "fun",
            "load", "(", ")", ":", "Unit", "{", "}", "}",
        ],
    },
    GrammarCase {
        name: "interface delegation",
        tokens: &[
            "class",
            "Screen",
            "(",
            "val",
            "renderer",
            ":",
            "Renderer",
            ")",
            ":",
            "Draw",
            "by",
            "renderer",
            ",",
            "Resettable",
            "{",
            "}",
        ],
    },
    GrammarCase {
        name: "operator hierarchy",
        tokens: &[
            "val", "result", "=", "target", "=", "a", "?:", "b", "||", "c", "&&", "d", "==", "e",
            "+", "f", "*", "g",
        ],
    },
    GrammarCase {
        name: "cast contains and type test",
        tokens: &[
            "val",
            "cast",
            "=",
            "input",
            "as?",
            "Type",
            "?",
            ";",
            "val",
            "contained",
            "=",
            "item",
            "!in",
            "items",
            ";",
            "val",
            "tested",
            "=",
            "item",
            "!is",
            "Type",
        ],
    },
    GrammarCase {
        name: "index call and reference",
        tokens: &[
            "val", "result", "=", "array", "[", "index", "]", ".", "member", "(", "argument", ")",
            "::", "ref",
        ],
    },
    GrammarCase {
        name: "super member",
        tokens: &[
            "val", "result", "=", "super", "<", "Logger", ">", ".", "log", "(", "message", ")",
        ],
    },
];

const TRIVIA_VARIANTS: &[&str] = &["\t", "/*c*/", " \t/*c*/ "];

#[derive(Debug, PartialEq, Eq)]
struct SyntaxShape {
    package_segments: Option<usize>,
    imports: Vec<(usize, bool, bool)>,
    roots: Vec<usize>,
    items: Vec<Discriminant<Item>>,
    statements: Vec<Discriminant<Statement>>,
    expressions: Vec<Discriminant<Expression>>,
    type_refs: Vec<Discriminant<TypeRef>>,
}

fn render(tokens: &[&str], default_gap: &str, replacement: Option<(usize, &str)>) -> String {
    let mut source = String::new();
    for (index, token) in tokens.iter().enumerate() {
        if index > 0 {
            source.push_str(
                replacement
                    .filter(|(gap, _)| *gap == index - 1)
                    .map_or(default_gap, |(_, trivia)| trivia),
            );
        }
        source.push_str(token);
    }
    source
}

fn significant_kinds(lexemes: &[lang_frontend::lexer::Lexeme]) -> Vec<LexemeKind> {
    lexemes
        .iter()
        .filter_map(|lexeme| match lexeme.kind() {
            LexemeKind::Trivia(_) => None,
            kind => Some(kind),
        })
        .collect()
}

fn syntax_shape(parsed: &ParsedFile) -> SyntaxShape {
    let ast = parsed.ast();
    SyntaxShape {
        package_segments: parsed.package().map(|package| package.segments.len()),
        imports: parsed
            .imports()
            .iter()
            .map(|import| {
                (
                    import.segments.len(),
                    import.wildcard_span.is_some(),
                    import.alias.is_some(),
                )
            })
            .collect(),
        roots: parsed.roots().iter().map(|root| root.index()).collect(),
        items: ast
            .items()
            .iter()
            .map(|(_, node)| discriminant(node.payload()))
            .collect(),
        statements: ast
            .statements()
            .iter()
            .map(|(_, node)| discriminant(node.payload()))
            .collect(),
        expressions: ast
            .expressions()
            .iter()
            .map(|(_, node)| discriminant(node.payload()))
            .collect(),
        type_refs: ast
            .type_refs()
            .iter()
            .map(|(_, node)| discriminant(node.payload()))
            .collect(),
    }
}

fn validate_span(source_id: SourceId, source_len: usize, span: Span) {
    assert_eq!(span.source_id(), source_id);
    assert!(span.start() <= span.end());
    assert!(span.end() <= source_len);
}

fn validate_parsed_file(
    source_id: SourceId,
    source_len: usize,
    parsed: &ParsedFile,
    context: &str,
) {
    assert_eq!(parsed.source_id(), source_id);
    validate_ast(source_id, source_len, parsed.ast());
    validate_diagnostics(source_id, source_len, parsed.diagnostics());
    for root in parsed.roots() {
        parsed
            .ast()
            .items()
            .get(*root)
            .unwrap_or_else(|error| panic!("invalid file root for {context}: {error}"));
    }
    if let Some(package) = parsed.package() {
        validate_span(source_id, source_len, package.span);
        validate_span(source_id, source_len, package.keyword_span);
        for segment in &package.segments {
            validate_span(source_id, source_len, segment.span);
        }
    }
    for import in parsed.imports() {
        validate_span(source_id, source_len, import.span);
        validate_span(source_id, source_len, import.keyword_span);
        for segment in &import.segments {
            validate_span(source_id, source_len, segment.span);
        }
        if let Some(span) = import.wildcard_span {
            validate_span(source_id, source_len, span);
        }
        if let Some(alias) = import.alias {
            validate_span(source_id, source_len, alias.as_span);
            validate_span(source_id, source_len, alias.name_span);
        }
    }
}

fn parse_clean(source: &str, context: &str) -> (Vec<LexemeKind>, SyntaxShape) {
    let (sources, source_id, lexed) = lex_source_twice(
        "parser-trivia-invariance.ko",
        source,
        context,
        validate_lexed,
    );
    assert!(
        lexed.diagnostics().is_empty(),
        "Lexer diagnostics for {context}: {:?}",
        lexed.diagnostics()
    );
    let kinds = significant_kinds(lexed.lexemes());
    let first = parse_file(&sources, &lexed)
        .unwrap_or_else(|error| panic!("first parse failed for {context}: {error}"));
    let repeated = parse_file(&sources, &lexed)
        .unwrap_or_else(|error| panic!("repeated parse failed for {context}: {error}"));
    for parsed in [&first, &repeated] {
        validate_parsed_file(source_id, source.len(), parsed, context);
        assert!(
            parsed.diagnostics().is_empty(),
            "Parser diagnostics for {context}: {:?}\nsource={source:?}",
            parsed.diagnostics()
        );
    }
    assert_eq!(
        syntax_shape(&first),
        syntax_shape(&repeated),
        "non-deterministic syntax shape for {context}"
    );
    assert_eq!(
        format!("{first:?}"),
        format!("{repeated:?}"),
        "non-deterministic parse for {context}"
    );
    (kinds, syntax_shape(&first))
}

#[test]
fn non_newline_trivia_preserves_significant_tokens_and_syntax_shape() {
    assert_eq!(GRAMMAR_CASES.len(), 20);
    assert_eq!(TRIVIA_VARIANTS.len(), 3);

    let mut executed = 0;
    for case in GRAMMAR_CASES {
        let baseline_source = render(case.tokens, " ", None);
        let (baseline_kinds, baseline_shape) = parse_clean(&baseline_source, case.name);
        executed += 1;

        for (gap, _) in case.tokens.windows(2).enumerate() {
            for trivia in TRIVIA_VARIANTS {
                let context = format!("{} gap {gap} with {trivia:?}", case.name);
                let source = render(case.tokens, " ", Some((gap, trivia)));
                let (kinds, shape) = parse_clean(&source, &context);
                assert_eq!(kinds, baseline_kinds, "token drift for {context}");
                assert_eq!(shape, baseline_shape, "syntax drift for {context}");
                executed += 1;
            }
        }

        for trivia in TRIVIA_VARIANTS {
            let context = format!("{} all gaps with {trivia:?}", case.name);
            let source = render(case.tokens, trivia, None);
            let (kinds, shape) = parse_clean(&source, &context);
            assert_eq!(kinds, baseline_kinds, "token drift for {context}");
            assert_eq!(shape, baseline_shape, "syntax drift for {context}");
            executed += 1;

            let context = format!("{} file edges with {trivia:?}", case.name);
            let source = format!("{trivia}{baseline_source}{trivia}");
            let (kinds, shape) = parse_clean(&source, &context);
            assert_eq!(kinds, baseline_kinds, "token drift for {context}");
            assert_eq!(shape, baseline_shape, "syntax drift for {context}");
            executed += 1;
        }
    }

    assert_eq!(executed, 1_172);
}
