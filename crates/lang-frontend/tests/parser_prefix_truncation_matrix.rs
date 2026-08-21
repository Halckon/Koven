//! SPEC-0079 的合法完整语法逐 UTF-8 前缀 EOF 恢复矩阵。

use std::collections::BTreeSet;

use lang_frontend::{
    diagnostic::DiagnosticDetail,
    lexer::{LexedFile, LexemeKind, lex},
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap, Span},
};

#[derive(Clone, Copy)]
struct GrammarCase {
    name: &'static str,
    source: &'static str,
}

const GRAMMAR_CASES: &[GrammarCase] = &[
    GrammarCase {
        name: "file header",
        source: "package demo.core; import lib.*; val answer = 42",
    },
    GrammarCase {
        name: "variables and constant",
        source: "public val answer: Int = 42; var counter: Int = 0; const val limit: Int = 10",
    },
    GrammarCase {
        name: "generic function",
        source: "fun <T: Copyable> id(borrow item: T): T = item",
    },
    GrammarCase {
        name: "function type",
        source: "val handler: move (borrow Int, inout String) -> Unit = target",
    },
    GrammarCase {
        name: "typed named mode call",
        source: "val result = service.send<Int>(name = borrow input, &target)",
    },
    GrammarCase {
        name: "move lambda",
        source: "val combine = move { left, right -> left + right }",
    },
    GrammarCase {
        name: "local destructuring",
        source: "fun usePair() { val (first, second) = pair }",
    },
    GrammarCase {
        name: "if expression",
        source: "val choice = if (ready) yes else no",
    },
    GrammarCase {
        name: "when expression",
        source: "val choice = when (input) { is Type -> yes; !is Other -> no; else -> fallback }",
    },
    GrammarCase {
        name: "loop family",
        source: "fun loops() { while (ready) { continue } for (item in items) { break } loop { return } }",
    },
    GrammarCase {
        name: "postfix chain",
        source: "val result = source?.member!!?",
    },
    GrammarCase {
        name: "ordinary class",
        source: "public class Box<T: Copyable>(private val item: T, var count: Int): Printable { override fun show(): Unit {} }",
    },
    GrammarCase {
        name: "value class",
        source: "value class Meters(val amount: Int)",
    },
    GrammarCase {
        name: "interface companion",
        source: "interface Printable { fun show(): Unit; companion object { const val NAME: String = \"printable\" } }",
    },
    GrammarCase {
        name: "enum variants and member",
        source: "enum class Result<T> { Ok(payload: T), Error(message: String); fun isOk(): Boolean = true }",
    },
    GrammarCase {
        name: "named object",
        source: "object Config { const val VERSION: Int = 1; fun load(): Unit {} }",
    },
    GrammarCase {
        name: "interface delegation",
        source: "class Screen(val renderer: Renderer): Draw by renderer, Resettable {}",
    },
    GrammarCase {
        name: "operator hierarchy",
        source: "val result = target = a ?: b || c && d == e + f * g",
    },
    GrammarCase {
        name: "cast contains and type test",
        source: "val cast = input as? Type?; val contained = item !in items; val tested = item !is Type",
    },
    GrammarCase {
        name: "index call and reference",
        source: "val result = array[index].member(argument)::ref",
    },
    GrammarCase {
        name: "super member",
        source: "val result = super<Logger>.log(message)",
    },
    GrammarCase {
        name: "unicode lexical owners",
        source: r#"val text = "前${call('界', "内${x}")}后" /*尾*/"#,
    },
];

fn validate_span(source_id: SourceId, source_len: usize, span: Span) {
    assert_eq!(span.source_id(), source_id);
    assert!(span.start() <= span.end());
    assert!(span.end() <= source_len);
}

fn validate_diagnostics(
    source_id: SourceId,
    source_len: usize,
    diagnostics: &[lang_frontend::diagnostic::Diagnostic],
) {
    for diagnostic in diagnostics {
        validate_span(source_id, source_len, diagnostic.primary_span());
        for detail in diagnostic.details() {
            if let DiagnosticDetail::Label(label) = detail {
                validate_span(source_id, source_len, label.span());
            }
        }
    }
}

fn validate_lexemes(source_id: SourceId, source_len: usize, lexed: &LexedFile) {
    let mut covered = 0;
    let mut eof_count = 0;

    for (index, lexeme) in lexed.lexemes().iter().enumerate() {
        let span = lexeme.span();
        validate_span(source_id, source_len, span);
        if lexeme.kind() == LexemeKind::Eof {
            eof_count += 1;
            assert_eq!(index + 1, lexed.lexemes().len());
            assert_eq!((span.start(), span.end()), (source_len, source_len));
            continue;
        }

        assert_eq!(span.start(), covered);
        assert!(span.end() > span.start());
        covered = span.end();
    }

    assert_eq!(covered, source_len);
    assert_eq!(eof_count, 1);
    validate_diagnostics(source_id, source_len, lexed.diagnostics());
}

fn validate_parsed(source_id: SourceId, source_len: usize, parsed: &ParsedFile) {
    validate_diagnostics(source_id, source_len, parsed.diagnostics());
    let ast = parsed.ast();
    for (_, node) in ast.items().iter() {
        validate_span(source_id, source_len, node.span());
    }
    for (_, node) in ast.statements().iter() {
        validate_span(source_id, source_len, node.span());
    }
    for (_, node) in ast.expressions().iter() {
        validate_span(source_id, source_len, node.span());
    }
    for (_, node) in ast.type_refs().iter() {
        validate_span(source_id, source_len, node.span());
    }
}

fn parse_prefix(source: &str, context: &str) -> ParsedFile {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-prefix-truncation.ko", source)
        .expect("matrix source name must be unique");
    let lexed = lex(&sources, source_id).expect("matrix prefix must lex internally");
    validate_lexemes(source_id, source.len(), &lexed);

    let first = parse_file(&sources, &lexed)
        .unwrap_or_else(|error| panic!("first parse failed for {context}: {error}"));
    let repeated = parse_file(&sources, &lexed)
        .unwrap_or_else(|error| panic!("repeated parse failed for {context}: {error}"));
    assert_eq!(
        format!("{first:?}"),
        format!("{repeated:?}"),
        "non-deterministic parse for {context}"
    );
    validate_parsed(source_id, source.len(), &first);
    first
}

fn prefix_ends(source: &str) -> Vec<usize> {
    let mut ends = Vec::with_capacity(source.chars().count() + 1);
    ends.push(0);
    ends.extend(
        source
            .char_indices()
            .map(|(start, character)| start + character.len_utf8()),
    );
    ends
}

#[test]
fn every_utf8_prefix_of_representative_complete_files_is_recoverable() {
    assert_eq!(GRAMMAR_CASES.len(), 22);
    assert_eq!(
        GRAMMAR_CASES
            .iter()
            .map(|case| case.source)
            .collect::<BTreeSet<_>>()
            .len(),
        GRAMMAR_CASES.len()
    );

    let mut executed = 0;
    for case in GRAMMAR_CASES {
        let complete = parse_prefix(case.source, case.name);
        assert!(
            complete.diagnostics().is_empty(),
            "complete source must parse cleanly for {}: {:?}",
            case.name,
            complete.diagnostics()
        );

        let ends = prefix_ends(case.source);
        assert_eq!(ends.len(), case.source.chars().count() + 1);
        assert_eq!(ends.first(), Some(&0));
        assert_eq!(ends.last(), Some(&case.source.len()));
        assert!(ends.windows(2).all(|window| window[0] < window[1]));
        for end in ends {
            let context = format!("{} prefix byte {end}", case.name);
            parse_prefix(&case.source[..end], &context);
            executed += 1;
        }
    }

    assert_eq!(executed, 1_373);
}
