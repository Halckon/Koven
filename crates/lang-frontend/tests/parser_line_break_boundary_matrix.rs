//! SPEC-0078 / SPEC-0098 / SPEC-0100 的结构性换行边界与公开产物不变量。

use std::mem::{Discriminant, discriminant};

use lang_frontend::{
    lexer::{LexemeKind, lex},
    parser::{Expression, Item, ParsedFile, Statement, TypeRef, parse_file},
    source::{SourceId, SourceMap, Span},
};

#[path = "support/frontend_output_assertions.rs"]
mod frontend_output_assertions;
#[path = "support/parser_line_break_carriers.rs"]
mod parser_line_break_carriers;

use frontend_output_assertions::{validate_ast, validate_diagnostics, validate_lexed};
use parser_line_break_carriers::{
    Carrier, NON_BREAK_TRIVIA, STRUCTURAL_BREAKS, validate_carrier_lexemes,
};

#[derive(Clone, Copy)]
struct BoundaryCase {
    name: &'static str,
    prefix: &'static str,
    suffix: &'static str,
    non_break_codes: &'static [&'static str],
}

const BOUNDARY_CASES: &[BoundaryCase] = &[
    BoundaryCase {
        name: "package to import",
        prefix: "package a",
        suffix: "import b\nval x=1",
        non_break_codes: &["L0053"],
    },
    BoundaryCase {
        name: "import to declaration",
        prefix: "import a",
        suffix: "val x=1",
        non_break_codes: &["L0053"],
    },
    BoundaryCase {
        name: "top-level declarations",
        prefix: "val a=1",
        suffix: "fun f(){}",
        non_break_codes: &["L0047"],
    },
    BoundaryCase {
        name: "class members",
        prefix: "class C { fun a() {}",
        suffix: "fun b() {} }",
        non_break_codes: &["L0072"],
    },
    BoundaryCase {
        name: "when entries",
        prefix: "val x = when { ready -> one",
        suffix: "other -> two }",
        non_break_codes: &["L0065"],
    },
    BoundaryCase {
        name: "bare return",
        prefix: "fun f() { return",
        suffix: "result }",
        non_break_codes: &[],
    },
];

#[derive(Clone, Copy)]
struct InvariantCase {
    name: &'static str,
    prefix: &'static str,
    suffix: &'static str,
    codes: &'static [&'static str],
}

const INVARIANT_CASES: &[InvariantCase] = &[
    InvariantCase {
        name: "enum variants still require comma",
        prefix: "enum class E { A",
        suffix: "B }",
        codes: &["L0074"],
    },
    InvariantCase {
        name: "infix expression continues",
        prefix: "val x = a",
        suffix: "+ b",
        codes: &[],
    },
];

#[derive(Debug, PartialEq, Eq)]
struct SyntaxShape {
    package_segments: Option<usize>,
    import_count: usize,
    roots: Vec<usize>,
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

fn syntax_shape(parsed: &ParsedFile) -> SyntaxShape {
    let ast = parsed.ast();
    SyntaxShape {
        package_segments: parsed.package().map(|package| package.segments.len()),
        import_count: parsed.imports().len(),
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
    source: &str,
    carrier_start: usize,
    carrier: Carrier,
    context: &str,
) -> ParseFingerprint {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("parser-line-break-matrix.ko", source)
        .expect("matrix source name must be unique");
    let lexed = lex(&sources, source_id).expect("matrix source must lex internally");
    validate_lexed(source_id, source.len(), &lexed);
    validate_carrier_lexemes(&sources, &lexed, carrier_start, carrier, context);
    assert!(
        lexed.diagnostics().is_empty(),
        "Lexer diagnostics for {context}: {:?}",
        lexed.diagnostics()
    );
    let significant_kinds = lexed
        .lexemes()
        .iter()
        .filter_map(|lexeme| match lexeme.kind() {
            LexemeKind::Trivia(_) => None,
            kind => Some(kind),
        })
        .collect();

    let first = parse_file(&sources, &lexed)
        .unwrap_or_else(|error| panic!("first parse failed for {context}: {error}"));
    let repeated = parse_file(&sources, &lexed)
        .unwrap_or_else(|error| panic!("repeated parse failed for {context}: {error}"));
    for parsed in [&first, &repeated] {
        validate_parsed_file(source_id, source.len(), parsed, context);
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
        shape: syntax_shape(&first),
    }
}

fn source(prefix: &str, carrier: &str, suffix: &str) -> String {
    format!("{prefix}{carrier}{suffix}")
}

#[test]
fn line_break_carriers_share_boundaries_and_non_break_trivia_does_not() {
    assert_eq!(STRUCTURAL_BREAKS.len(), 6);
    assert_eq!(NON_BREAK_TRIVIA.len(), 4);
    assert_eq!(BOUNDARY_CASES.len(), 6);
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

        if case.name == "bare return" {
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
    assert_eq!(executed, 60);
}

#[test]
fn newline_does_not_replace_enum_comma_or_split_an_infix_expression() {
    assert_eq!(INVARIANT_CASES.len(), 2);
    let carriers = STRUCTURAL_BREAKS.iter().chain(NON_BREAK_TRIVIA);
    let mut executed = 0;

    for case in INVARIANT_CASES {
        let mut baseline = None;
        for carrier in carriers.clone() {
            let context = format!("{} with {:?}", case.name, carrier.text);
            let parsed = parse_twice(
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
    assert_eq!(executed, 20);
}
