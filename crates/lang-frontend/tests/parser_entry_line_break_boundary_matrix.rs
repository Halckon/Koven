//! SPEC-0092 / SPEC-0100 / SPEC-0112 的独立入口换行边界与公开产物不变量。

use std::mem::{Discriminant, discriminant};

use lang_frontend::{
    lexer::LexemeKind,
    parser::{
        Expression, Item, Statement, SyntaxAst, TypeRef, parse_block, parse_declaration,
        parse_expression,
    },
};

#[path = "support/frontend_output_assertions.rs"]
mod frontend_output_assertions;
#[path = "support/lexer_matrix_assertions.rs"]
mod lexer_matrix_assertions;
#[path = "support/parser_line_break_carriers.rs"]
mod parser_line_break_carriers;

use frontend_output_assertions::{validate_ast, validate_diagnostics, validate_lexed};
use lexer_matrix_assertions::lex_source_twice;
use parser_line_break_carriers::{
    Carrier, NON_BREAK_TRIVIA, STRUCTURAL_BREAKS, validate_carrier_lexemes,
};

#[derive(Clone, Copy, Debug)]
enum EntryKind {
    Expression,
    Declaration,
    Block,
}

#[derive(Clone, Copy)]
struct BoundaryCase {
    name: &'static str,
    kind: EntryKind,
    prefix: &'static str,
    suffix: &'static str,
    non_break_codes: &'static [&'static str],
}

const BOUNDARY_CASES: &[BoundaryCase] = &[
    BoundaryCase {
        name: "expression when entries",
        kind: EntryKind::Expression,
        prefix: "when { ready -> one",
        suffix: "other -> two }",
        non_break_codes: &["L0065"],
    },
    BoundaryCase {
        name: "declaration class members",
        kind: EntryKind::Declaration,
        prefix: "class C { fun a() {}",
        suffix: "fun b() {} }",
        non_break_codes: &["L0072"],
    },
    BoundaryCase {
        name: "block bare return",
        kind: EntryKind::Block,
        prefix: "{ return",
        suffix: "result }",
        non_break_codes: &[],
    },
];

#[derive(Clone, Copy)]
struct InvariantCase {
    name: &'static str,
    kind: EntryKind,
    prefix: &'static str,
    suffix: &'static str,
    codes: &'static [&'static str],
}

const INVARIANT_CASES: &[InvariantCase] = &[
    InvariantCase {
        name: "expression infix continues",
        kind: EntryKind::Expression,
        prefix: "a",
        suffix: "+ b",
        codes: &[],
    },
    InvariantCase {
        name: "declaration enum variants still require comma",
        kind: EntryKind::Declaration,
        prefix: "enum class E { A",
        suffix: "B }",
        codes: &["L0074"],
    },
    InvariantCase {
        name: "block unfinished operator continues",
        kind: EntryKind::Block,
        prefix: "{ a +",
        suffix: "b }",
        codes: &[],
    },
];

#[derive(Debug, PartialEq, Eq)]
struct SyntaxShape {
    root: usize,
    items: Vec<Discriminant<Item>>,
    statements: Vec<Discriminant<Statement>>,
    expressions: Vec<Discriminant<Expression>>,
    type_refs: Vec<Discriminant<TypeRef>>,
    classifier_members: Vec<usize>,
    when_entries: Vec<usize>,
    return_values: Vec<bool>,
}

#[derive(Debug, PartialEq, Eq)]
struct ParseFingerprint {
    significant_kinds: Vec<LexemeKind>,
    diagnostic_codes: Vec<String>,
    diagnostic_messages: Vec<String>,
    shape: SyntaxShape,
}

fn syntax_shape(ast: &SyntaxAst, root: usize) -> SyntaxShape {
    SyntaxShape {
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
        classifier_members: ast
            .items()
            .iter()
            .filter_map(|(_, node)| match node.payload() {
                Item::Classifier(classifier) => {
                    classifier.body.as_ref().map(|body| body.members.len())
                }
                _ => None,
            })
            .collect(),
        when_entries: ast
            .expressions()
            .iter()
            .filter_map(|(_, node)| match node.payload() {
                Expression::When { entries, .. } => Some(entries.len()),
                _ => None,
            })
            .collect(),
        return_values: ast
            .expressions()
            .iter()
            .filter_map(|(_, node)| match node.payload() {
                Expression::Return { value, .. } => Some(value.is_some()),
                _ => None,
            })
            .collect(),
    }
}

fn parse_twice(
    kind: EntryKind,
    source: &str,
    carrier_start: usize,
    carrier: Carrier,
    context: &str,
) -> ParseFingerprint {
    let (sources, source_id, lexed) = lex_source_twice(
        "parser-entry-line-break-matrix.ko",
        source,
        context,
        validate_lexed,
    );
    assert!(
        lexed.diagnostics().is_empty(),
        "Lexer diagnostics for {context}: {:?}",
        lexed.diagnostics()
    );
    validate_carrier_lexemes(&sources, &lexed, carrier_start, carrier, context);
    let significant_kinds = lexed
        .lexemes()
        .iter()
        .filter_map(|lexeme| match lexeme.kind() {
            LexemeKind::Trivia(_) => None,
            kind => Some(kind),
        })
        .collect();

    macro_rules! fingerprint {
        ($parse:ident, $table:ident) => {{
            let first = $parse(&sources, &lexed)
                .unwrap_or_else(|error| panic!("first parse failed for {context}: {error}"));
            let repeated = $parse(&sources, &lexed)
                .unwrap_or_else(|error| panic!("repeated parse failed for {context}: {error}"));
            for parsed in [&first, &repeated] {
                assert_eq!(parsed.source_id(), source_id);
                validate_ast(source_id, source.len(), parsed.ast());
                validate_diagnostics(source_id, source.len(), parsed.diagnostics());
                parsed
                    .ast()
                    .$table()
                    .get(parsed.root())
                    .unwrap_or_else(|error| panic!("root failed for {context}: {error}"));
            }
            assert_eq!(
                syntax_shape(first.ast(), first.root().index()),
                syntax_shape(repeated.ast(), repeated.root().index()),
                "non-deterministic syntax shape for {context}"
            );
            assert_eq!(
                format!("{first:?}"),
                format!("{repeated:?}"),
                "non-deterministic parse for {context}"
            );
            ParseFingerprint {
                significant_kinds,
                diagnostic_codes: first
                    .diagnostics()
                    .iter()
                    .map(|diagnostic| diagnostic.code().to_string())
                    .collect(),
                diagnostic_messages: first
                    .diagnostics()
                    .iter()
                    .map(|diagnostic| diagnostic.message().to_owned())
                    .collect(),
                shape: syntax_shape(first.ast(), first.root().index()),
            }
        }};
    }

    match kind {
        EntryKind::Expression => fingerprint!(parse_expression, expressions),
        EntryKind::Declaration => fingerprint!(parse_declaration, items),
        EntryKind::Block => fingerprint!(parse_block, statements),
    }
}

fn source(prefix: &str, carrier: &str, suffix: &str) -> String {
    format!("{prefix}{carrier}{suffix}")
}

#[test]
fn line_break_carriers_form_the_expected_boundary_for_each_entry() {
    assert_eq!(STRUCTURAL_BREAKS.len(), 6);
    assert_eq!(NON_BREAK_TRIVIA.len(), 4);
    assert_eq!(BOUNDARY_CASES.len(), 3);
    assert!(
        STRUCTURAL_BREAKS
            .iter()
            .all(|carrier| carrier.text.contains('\n'))
    );
    assert!(
        NON_BREAK_TRIVIA
            .iter()
            .all(|carrier| !carrier.text.contains('\n'))
    );

    let mut executed = 0;
    for case in BOUNDARY_CASES {
        let mut structural_baseline = None;
        for carrier in STRUCTURAL_BREAKS {
            let context = format!("{} structural {:?}", case.name, carrier.text);
            let parsed = parse_twice(
                case.kind,
                &source(case.prefix, carrier.text, case.suffix),
                case.prefix.len(),
                *carrier,
                &context,
            );
            assert!(
                parsed.diagnostic_codes.is_empty(),
                "unexpected structural diagnostics for {context}: {:?}",
                parsed.diagnostic_codes
            );
            if let Some(baseline) = &structural_baseline {
                assert_eq!(&parsed, baseline, "structural carrier drift for {context}");
            } else {
                structural_baseline = Some(parsed);
            }
            executed += 1;
        }

        let mut non_break_baseline = None;
        for carrier in NON_BREAK_TRIVIA {
            let context = format!("{} non-break {:?}", case.name, carrier.text);
            let parsed = parse_twice(
                case.kind,
                &source(case.prefix, carrier.text, case.suffix),
                case.prefix.len(),
                *carrier,
                &context,
            );
            assert_eq!(
                parsed.diagnostic_codes, case.non_break_codes,
                "unexpected non-break diagnostics for {context}"
            );
            if let Some(baseline) = &non_break_baseline {
                assert_eq!(&parsed, baseline, "non-break carrier drift for {context}");
            } else {
                non_break_baseline = Some(parsed);
            }
            executed += 1;
        }

        if matches!(case.kind, EntryKind::Block) {
            assert_eq!(
                structural_baseline
                    .as_ref()
                    .expect("structural baseline")
                    .shape
                    .return_values,
                [false]
            );
            assert_eq!(
                non_break_baseline
                    .as_ref()
                    .expect("non-break baseline")
                    .shape
                    .return_values,
                [true]
            );
        }
        assert_ne!(
            structural_baseline, non_break_baseline,
            "{} must observe the structural boundary distinction",
            case.name
        );
    }
    assert_eq!(executed, 30);
}

#[test]
fn newline_does_not_replace_required_tokens_or_split_infix_expressions() {
    assert_eq!(INVARIANT_CASES.len(), 3);
    let carriers = STRUCTURAL_BREAKS.iter().chain(NON_BREAK_TRIVIA);
    let mut executed = 0;

    for case in INVARIANT_CASES {
        let mut baseline = None;
        for carrier in carriers.clone() {
            let context = format!("{} with {:?}", case.name, carrier.text);
            let parsed = parse_twice(
                case.kind,
                &source(case.prefix, carrier.text, case.suffix),
                case.prefix.len(),
                *carrier,
                &context,
            );
            assert_eq!(
                parsed.diagnostic_codes, case.codes,
                "unexpected diagnostics for {context}"
            );
            if let Some(expected) = &baseline {
                assert_eq!(&parsed, expected, "carrier drift for {context}");
            } else {
                baseline = Some(parsed);
            }
            executed += 1;
        }
    }
    assert_eq!(executed, 30);
}
