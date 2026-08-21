//! SPEC-0091 的独立 Parser 入口非换行 trivia 等价矩阵。

use std::mem::{Discriminant, discriminant};

use lang_frontend::{
    lexer::{LexemeKind, lex},
    parser::{
        Expression, Item, Statement, SyntaxAst, TypeRef, parse_block, parse_declaration,
        parse_expression,
    },
    source::SourceMap,
};

#[path = "support/parser_entry_matrix.rs"]
mod parser_entry_matrix;
#[path = "support/parser_mutation_gaps.rs"]
mod parser_mutation_gaps;
#[path = "support/parser_mutation_tokens.rs"]
mod parser_mutation_tokens;

use parser_entry_matrix::{ENTRY_CASES, EntryCase, EntryKind, parse_entry_twice};
use parser_mutation_gaps::{Gap, token_gaps};
use parser_mutation_tokens::original_token_slots;

const TRIVIA_VARIANTS: &[&str] = &["\t", "/*c*/", " \t/*c*/ "];

#[derive(Debug, PartialEq, Eq)]
struct EntrySyntaxShape {
    root: usize,
    items: Vec<Discriminant<Item>>,
    statements: Vec<Discriminant<Statement>>,
    expressions: Vec<Discriminant<Expression>>,
    type_refs: Vec<Discriminant<TypeRef>>,
}

fn entry_syntax_shape(ast: &SyntaxAst, root: usize) -> EntrySyntaxShape {
    EntrySyntaxShape {
        root,
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

fn significant_kinds(lexemes: &[lang_frontend::lexer::Lexeme]) -> Vec<LexemeKind> {
    lexemes
        .iter()
        .filter_map(|lexeme| match lexeme.kind() {
            LexemeKind::Trivia(_) => None,
            kind => Some(kind),
        })
        .collect()
}

fn insert_trivia(source: &str, offset: usize, trivia: &str) -> String {
    let mut mutated = String::with_capacity(source.len() + trivia.len());
    mutated.push_str(&source[..offset]);
    mutated.push_str(trivia);
    mutated.push_str(&source[offset..]);
    mutated
}

fn insert_trivia_at_all_gaps(source: &str, gaps: &[Gap], trivia: &str) -> String {
    let mut mutated = source.to_owned();
    for gap in gaps.iter().rev() {
        mutated.insert_str(gap.offset, trivia);
    }
    mutated
}

fn parse_clean(
    case: EntryCase,
    source: &str,
    context: &str,
) -> (Vec<LexemeKind>, EntrySyntaxShape) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-entry-trivia-invariance.ko", source)
        .expect("matrix source name must be unique");
    let lexed = lex(&sources, source_id).expect("matrix source must lex internally");
    assert!(
        lexed.diagnostics().is_empty(),
        "Lexer diagnostics for {context}: {:?}\nsource={source:?}",
        lexed.diagnostics()
    );
    let kinds = significant_kinds(lexed.lexemes());
    assert_eq!(
        parse_entry_twice(case, &sources, source_id, &lexed, context),
        0,
        "entry must parse cleanly for {context}"
    );
    let shape = match case.kind {
        EntryKind::Expression => {
            let parsed = parse_expression(&sources, &lexed)
                .unwrap_or_else(|error| panic!("expression parse failed for {context}: {error}"));
            entry_syntax_shape(parsed.ast(), parsed.root().index())
        }
        EntryKind::Declaration => {
            let parsed = parse_declaration(&sources, &lexed)
                .unwrap_or_else(|error| panic!("declaration parse failed for {context}: {error}"));
            entry_syntax_shape(parsed.ast(), parsed.root().index())
        }
        EntryKind::Block => {
            let parsed = parse_block(&sources, &lexed)
                .unwrap_or_else(|error| panic!("block parse failed for {context}: {error}"));
            entry_syntax_shape(parsed.ast(), parsed.root().index())
        }
    };
    (kinds, shape)
}

#[test]
fn non_newline_trivia_preserves_significant_tokens_and_syntax_shape_for_every_entry() {
    assert_eq!(ENTRY_CASES.len(), 12);
    assert_eq!(TRIVIA_VARIANTS.len(), 3);
    let mut case_counts = [0; 3];
    let mut token_counts = [0; 3];
    let mut code_gap_counts = [0; 3];
    let mut mutation_counts = [0; 3];

    for case in ENTRY_CASES {
        let index = case.kind.count_index();
        case_counts[index] += 1;
        let mut baseline_sources = SourceMap::new();
        let baseline_source_id = baseline_sources
            .add_source("parser-entry-trivia-baseline.ko", case.source)
            .expect("baseline source name must be unique");
        let baseline_lexed =
            lex(&baseline_sources, baseline_source_id).expect("baseline must lex internally");
        let slots = original_token_slots(&baseline_lexed, case.source.len());
        token_counts[index] += slots.len();
        let gaps = token_gaps(slots.iter().map(|slot| (slot.kind, slot.span.end())));
        let code_gaps = gaps
            .into_iter()
            .filter(|gap| gap.code_mode)
            .collect::<Vec<_>>();
        code_gap_counts[index] += code_gaps.len();

        let (baseline_kinds, baseline_shape) = parse_clean(*case, case.source, case.name);
        for gap in &code_gaps {
            for trivia in TRIVIA_VARIANTS {
                let context = format!("{} insert {trivia:?} at code gap {}", case.name, gap.offset);
                let mutated = insert_trivia(case.source, gap.offset, trivia);
                let (kinds, shape) = parse_clean(*case, &mutated, &context);
                assert_eq!(kinds, baseline_kinds, "token drift for {context}");
                assert_eq!(shape, baseline_shape, "syntax drift for {context}");
                mutation_counts[index] += 1;
            }
        }

        for trivia in TRIVIA_VARIANTS {
            let context = format!("{} insert {trivia:?} at all code gaps", case.name);
            let mutated = insert_trivia_at_all_gaps(case.source, &code_gaps, trivia);
            let (kinds, shape) = parse_clean(*case, &mutated, &context);
            assert_eq!(kinds, baseline_kinds, "token drift for {context}");
            assert_eq!(shape, baseline_shape, "syntax drift for {context}");
            mutation_counts[index] += 1;
        }
    }

    assert_eq!(case_counts, [4, 4, 4]);
    assert_eq!(token_counts, [66, 104, 70]);
    assert_eq!(code_gap_counts, [66, 106, 67]);
    assert_eq!(mutation_counts, [210, 330, 213]);
    assert_eq!(mutation_counts.iter().sum::<usize>(), 753);
}
