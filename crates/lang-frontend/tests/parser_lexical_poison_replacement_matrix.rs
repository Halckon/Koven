//! SPEC-0081 的合法完整语法逐显著 token 词法 poison 替换矩阵。

use lang_frontend::{
    lexer::{TokenKind, lex},
    source::{SourceMap, Span},
};

#[path = "support/frontend_matrix_assertions.rs"]
mod frontend_matrix_assertions;
#[path = "support/parser_grammar_corpus.rs"]
mod parser_grammar_corpus;
#[path = "support/parser_mutation_assertions.rs"]
mod parser_mutation_assertions;
#[path = "support/parser_mutation_tokens.rs"]
mod parser_mutation_tokens;

use frontend_matrix_assertions::{parse_file_twice, validate_lexed};
use parser_grammar_corpus::GRAMMAR_CASES;
use parser_mutation_assertions::assert_last_root_source;
use parser_mutation_tokens::{MutationSlot, original_token_slots, token_affects_owner};

const SENTINEL: &str = "val sentinel = 0";

#[derive(Clone, Copy)]
struct Poison {
    name: &'static str,
    text: &'static str,
    code: &'static str,
}

const POISONS: &[Poison] = &[
    Poison {
        name: "invalid character",
        text: "#",
        code: "L0001",
    },
    Poison {
        name: "future reserved word",
        text: "async",
        code: "L0002",
    },
];

fn token_is_lexical_mode_segment(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::StringStart
            | TokenKind::StringText
            | TokenKind::StringEnd
            | TokenKind::InterpolationStart
            | TokenKind::InterpolationEnd
    )
}

fn baseline_and_slots(case_source: &str, context: &str) -> (String, Vec<MutationSlot>) {
    let source = format!("{case_source}\n{SENTINEL}");
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-poison-replacement-baseline.ko", &source)
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
    (source, slots)
}

fn replace_with_poison(source: &str, span: Span, poison: &str) -> String {
    let mut mutated = String::with_capacity(source.len() + poison.len() + 2);
    mutated.push_str(&source[..span.start()]);
    mutated.push(' ');
    mutated.push_str(poison);
    mutated.push(' ');
    mutated.push_str(&source[span.end()..]);
    mutated
}

#[test]
fn replacing_each_significant_token_with_lexer_poison_is_total_and_recoverable() {
    assert_eq!(GRAMMAR_CASES.len(), 22);
    assert_eq!(POISONS.len(), 2);

    let mut executed = 0;
    let mut owner_mutations = 0;
    let mut recoverable_mutations = 0;
    let mut lexical_mode_mutations = 0;
    let mut target_code_mutations = 0;

    for case in GRAMMAR_CASES {
        let (source, slots) = baseline_and_slots(case.source, case.name);
        assert!(!slots.is_empty(), "empty mutation slots for {}", case.name);

        for slot in slots {
            for poison in POISONS {
                let context = format!(
                    "{} replace {:?} at {}..{} with {}",
                    case.name,
                    slot.kind,
                    slot.span.start(),
                    slot.span.end(),
                    poison.name
                );
                let mutated = replace_with_poison(&source, slot.span, poison.text);
                let mut sources = SourceMap::new();
                let source_id = sources
                    .add_source("parser-poison-replacement.ko", &mutated)
                    .expect("mutation source name must be unique");
                let lexed = lex(&sources, source_id).expect("mutation must lex internally");
                validate_lexed(source_id, mutated.len(), &lexed);

                if token_is_lexical_mode_segment(slot.kind) {
                    lexical_mode_mutations += 1;
                } else {
                    assert_eq!(
                        lexed
                            .diagnostics()
                            .iter()
                            .filter(|diagnostic| diagnostic.code().to_string() == poison.code)
                            .count(),
                        1,
                        "target lexical root count drift for {context}: {:?}",
                        lexed.diagnostics()
                    );
                    target_code_mutations += 1;
                }

                let parsed = parse_file_twice(&sources, source_id, mutated.len(), &lexed, &context);
                if token_affects_owner(slot.kind) {
                    owner_mutations += 1;
                } else {
                    assert_last_root_source(&sources, &parsed, SENTINEL, &context);
                    recoverable_mutations += 1;
                }
                executed += 1;
            }
        }
    }

    assert!(owner_mutations > 0);
    assert!(recoverable_mutations > 0);
    assert!(lexical_mode_mutations > 0);
    assert!(target_code_mutations > 0);
    assert_eq!(
        (
            executed,
            owner_mutations,
            recoverable_mutations,
            lexical_mode_mutations,
            target_code_mutations,
        ),
        (792, 192, 600, 28, 764)
    );
}
