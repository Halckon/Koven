//! SPEC-0083 的合法完整语法 token gap 词法 poison 插入矩阵。

use lang_frontend::{
    lexer::{TokenKind, lex},
    source::SourceMap,
};

#[path = "support/frontend_matrix_assertions.rs"]
mod frontend_matrix_assertions;
#[path = "support/parser_grammar_corpus.rs"]
mod parser_grammar_corpus;
#[path = "support/parser_lexical_poisons.rs"]
mod parser_lexical_poisons;
#[path = "support/parser_mutation_assertions.rs"]
mod parser_mutation_assertions;
#[path = "support/parser_mutation_tokens.rs"]
mod parser_mutation_tokens;

use frontend_matrix_assertions::{parse_file_twice, validate_lexed};
use parser_grammar_corpus::GRAMMAR_CASES;
use parser_lexical_poisons::LEXICAL_POISONS;
use parser_mutation_assertions::assert_last_root_source;
use parser_mutation_tokens::{MutationSlot, original_token_slots};

const SENTINEL: &str = "val sentinel = 0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LexicalMode {
    Code,
    String,
}

#[derive(Clone, Copy, Debug)]
struct Gap {
    offset: usize,
    code_mode: bool,
}

fn token_gaps(slots: &[MutationSlot]) -> Vec<Gap> {
    let mut modes = vec![LexicalMode::Code];
    let mut gaps = vec![Gap {
        offset: 0,
        code_mode: true,
    }];

    for slot in slots {
        match slot.kind {
            TokenKind::StringStart => {
                assert_eq!(modes.last(), Some(&LexicalMode::Code));
                modes.push(LexicalMode::String);
            }
            TokenKind::InterpolationStart => {
                assert_eq!(modes.last(), Some(&LexicalMode::String));
                modes.push(LexicalMode::Code);
            }
            TokenKind::InterpolationEnd => {
                assert_eq!(modes.pop(), Some(LexicalMode::Code));
                assert_eq!(modes.last(), Some(&LexicalMode::String));
            }
            TokenKind::StringEnd => {
                assert_eq!(modes.pop(), Some(LexicalMode::String));
            }
            _ => {}
        }
        gaps.push(Gap {
            offset: slot.span.end(),
            code_mode: modes.last() == Some(&LexicalMode::Code),
        });
    }

    assert_eq!(modes, [LexicalMode::Code]);
    gaps
}

fn baseline_and_gaps(case_source: &str, context: &str) -> (String, Vec<Gap>, usize) {
    let source = format!("{case_source}\n{SENTINEL}");
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-poison-insertion-baseline.ko", &source)
        .expect("baseline source name must be unique");
    let lexed = lex(&sources, source_id).expect("baseline source must lex internally");
    validate_lexed(source_id, source.len(), &lexed);
    assert!(
        lexed.diagnostics().is_empty(),
        "baseline must lex cleanly for {context}: {:?}",
        lexed.diagnostics()
    );
    let parsed = parse_file_twice(&sources, source_id, source.len(), &lexed, context);
    assert!(
        parsed.diagnostics().is_empty(),
        "baseline must parse cleanly for {context}: {:?}",
        parsed.diagnostics()
    );
    assert_last_root_source(&sources, &parsed, SENTINEL, context);

    let slots = original_token_slots(&lexed, case_source.len());
    let gaps = token_gaps(&slots);
    (source, gaps, slots.len())
}

fn insert_poison(source: &str, offset: usize, poison: &str) -> (String, (usize, usize)) {
    let poison_start = offset + 1;
    let poison_end = poison_start + poison.len();
    let mut mutated = String::with_capacity(source.len() + poison.len() + 2);
    mutated.push_str(&source[..offset]);
    mutated.push(' ');
    mutated.push_str(poison);
    mutated.push(' ');
    mutated.push_str(&source[offset..]);
    (mutated, (poison_start, poison_end))
}

#[test]
fn inserting_lexer_poison_at_each_token_gap_preserves_the_complete_grammar() {
    assert_eq!(GRAMMAR_CASES.len(), 22);
    assert_eq!(LEXICAL_POISONS.len(), 2);

    let mut token_count = 0;
    let mut gap_count = 0;
    let mut code_mode_gaps = 0;
    let mut string_mode_gaps = 0;
    let mut executed = 0;

    for case in GRAMMAR_CASES {
        let (source, gaps, case_token_count) = baseline_and_gaps(case.source, case.name);
        token_count += case_token_count;
        gap_count += gaps.len();

        for gap in gaps {
            if gap.code_mode {
                code_mode_gaps += 1;
            } else {
                string_mode_gaps += 1;
            }

            for poison in LEXICAL_POISONS {
                let context = format!(
                    "{} insert {} at gap {} ({})",
                    case.name,
                    poison.name,
                    gap.offset,
                    if gap.code_mode { "code" } else { "string" }
                );
                let (mutated, poison_range) = insert_poison(&source, gap.offset, poison.text);
                let mut sources = SourceMap::new();
                let source_id = sources
                    .add_source("parser-poison-insertion.ko", &mutated)
                    .expect("mutation source name must be unique");
                let poison_span = sources
                    .span(source_id, poison_range.0, poison_range.1)
                    .expect("inserted poison span must fit the mutation source");
                let lexed = lex(&sources, source_id).expect("mutation must lex internally");
                validate_lexed(source_id, mutated.len(), &lexed);

                if gap.code_mode {
                    assert_eq!(
                        lexed
                            .diagnostics()
                            .iter()
                            .map(|diagnostic| {
                                (diagnostic.code().to_string(), diagnostic.primary_span())
                            })
                            .collect::<Vec<_>>(),
                        [(poison.code.to_owned(), poison_span)],
                        "target lexical root drift for {context}: {:?}",
                        lexed.diagnostics()
                    );
                } else {
                    assert!(
                        lexed.diagnostics().is_empty(),
                        "string text insertion must lex cleanly for {context}: {:?}",
                        lexed.diagnostics()
                    );
                }

                let parsed = parse_file_twice(&sources, source_id, mutated.len(), &lexed, &context);
                if !gap.code_mode {
                    assert!(
                        parsed.diagnostics().is_empty(),
                        "string text insertion must parse cleanly for {context}: {:?}",
                        parsed.diagnostics()
                    );
                }
                assert_last_root_source(&sources, &parsed, SENTINEL, &context);
                executed += 1;
            }
        }
    }

    assert_eq!(
        (
            token_count,
            gap_count,
            code_mode_gaps,
            string_mode_gaps,
            executed,
        ),
        (396, 418, 409, 9, 836)
    );
}
