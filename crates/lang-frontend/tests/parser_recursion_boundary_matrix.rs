//! SPEC-0152 / SPEC-0159 的四公开 Parser 入口递归预算精确边界矩阵。

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::LexedFile,
    parser::{
        Expression, ExpressionAst, Item, ParserInternalError, Statement, parse_block,
        parse_declaration, parse_expression, parse_file,
    },
    source::{SourceId, SourceMap},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

use parser_test_assertions::{
    assert_parser_error_twice, lex_parser_source_twice, parse_block_twice, parse_declaration_twice,
    parse_expression_twice, parse_file_twice,
};

const RECURSION_LIMIT: usize = 1_024;

#[derive(Clone, Copy)]
enum OwnerShape {
    Closed,
    Terminal,
}

impl OwnerShape {
    const fn label(self) -> &'static str {
        match self {
            Self::Closed => "closed lexical owner",
            Self::Terminal => "terminal lexical owner",
        }
    }

    fn source(self, depth: usize) -> String {
        let mut source = format!("{}x", "\"${".repeat(depth));
        if matches!(self, Self::Closed) {
            source.push_str(&"}\"".repeat(depth));
        }
        source
    }
}

#[derive(Clone, Copy)]
enum OwnerEntry {
    Expression,
    Declaration,
    Block,
    File,
}

impl OwnerEntry {
    const fn label(self) -> &'static str {
        match self {
            Self::Expression => "expression",
            Self::Declaration => "declaration",
            Self::Block => "block",
            Self::File => "file",
        }
    }

    const fn boundary(self) -> (usize, usize) {
        match self {
            Self::Expression | Self::Declaration | Self::File => (511, 512),
            Self::Block => (510, 511),
        }
    }

    fn source(self, owner: &str, terminal: bool) -> (String, usize) {
        let (prefix, suffix) = match self {
            Self::Expression => ("", ""),
            Self::Declaration | Self::File => ("val result = ", ""),
            Self::Block if terminal => ("{ ", ""),
            Self::Block => ("{ ", " }"),
        };
        (format!("{prefix}{owner}{suffix}"), prefix.len())
    }
}

#[derive(Clone, Copy)]
enum ExpressionShape {
    Prefix,
    Assignment,
    Elvis,
    Group,
    GenericType,
    FunctionType,
}

impl ExpressionShape {
    const fn label(self) -> &'static str {
        match self {
            Self::Prefix => "prefix",
            Self::Assignment => "assignment",
            Self::Elvis => "elvis",
            Self::Group => "group",
            Self::GenericType => "generic type",
            Self::FunctionType => "function type",
        }
    }

    const fn boundary(self) -> (usize, usize) {
        match self {
            Self::Prefix | Self::Group => (511, 512),
            Self::Assignment | Self::Elvis | Self::GenericType | Self::FunctionType => {
                (1_022, 1_023)
            }
        }
    }

    fn source(self, depth: usize) -> String {
        match self {
            Self::Prefix => {
                // Alternation prevents adjacent `!` from becoming postfix `!!` tokens.
                let operators = (0..depth)
                    .map(|index| if index % 2 == 0 { '!' } else { '-' })
                    .collect::<String>();
                format!("{operators}x")
            }
            Self::Assignment => format!("{}z", "a=".repeat(depth)),
            Self::Elvis => format!("{}z", "a?:".repeat(depth)),
            Self::Group => format!("{}x{}", "(".repeat(depth), ")".repeat(depth)),
            Self::GenericType => {
                format!("x as {}T{}", "A<".repeat(depth), ">".repeat(depth))
            }
            Self::FunctionType => format!("x as {}T", "() -> ".repeat(depth)),
        }
    }
}

fn add_source(source: String) -> (SourceMap, SourceId) {
    let mut sources = SourceMap::new();
    let source_id = sources
        .add_source("recursion-boundary.ko", source)
        .expect("boundary source name must be unique");
    (sources, source_id)
}

fn assert_expression_boundary(shape: ExpressionShape) {
    let (accepted_depth, rejected_depth) = shape.boundary();
    assert_eq!(accepted_depth + 1, rejected_depth);

    let (sources, source_id) = add_source(shape.source(accepted_depth));
    let parsed = parse_expression_twice(&sources, source_id, shape.label());
    assert!(
        parsed.diagnostics().is_empty(),
        "accepted {} boundary produced diagnostics: {:?}",
        shape.label(),
        parsed.diagnostics()
    );

    let (sources, source_id) = add_source(shape.source(rejected_depth));
    let lexed = lex_parser_source_twice(&sources, source_id, shape.label());
    assert_parser_error_twice(
        &sources,
        &lexed,
        ParserInternalError::NestingLimitExceeded {
            limit: RECURSION_LIMIT,
        },
        shape.label(),
        parse_expression,
    );
}

fn assert_owner_diagnostics(
    diagnostics: &[Diagnostic],
    shape: OwnerShape,
    owner_start: usize,
    depth: usize,
    source_len: usize,
    context: &str,
) {
    match shape {
        OwnerShape::Closed => assert!(
            diagnostics.is_empty(),
            "closed lexical owner diagnostics for {context}: {diagnostics:?}"
        ),
        OwnerShape::Terminal => {
            assert_eq!(diagnostics.len(), 1, "terminal diagnostics for {context}");
            let diagnostic = &diagnostics[0];
            assert_eq!(
                diagnostic.code().to_string(),
                "L0005",
                "terminal code for {context}"
            );
            assert_eq!(
                (
                    diagnostic.primary_span().start(),
                    diagnostic.primary_span().end()
                ),
                (owner_start + depth * 3 - 2, source_len),
                "terminal span for {context}"
            );
        }
    }
}

fn assert_string_count(ast: &ExpressionAst, depth: usize) {
    assert_eq!(
        ast.expressions()
            .iter()
            .filter(|(_, node)| matches!(node.payload(), Expression::String { .. }))
            .count(),
        depth
    );
}

fn assert_expression_is_string(ast: &ExpressionAst, expression: lang_frontend::ast::ExpressionId) {
    assert!(matches!(
        ast.expressions()
            .get(expression)
            .expect("lexical owner root expression")
            .payload(),
        Expression::String { .. }
    ));
}

fn assert_accepted_owner(
    entry: OwnerEntry,
    shape: OwnerShape,
    sources: &SourceMap,
    source_id: SourceId,
    owner_start: usize,
    depth: usize,
    context: &str,
) {
    let source_len = sources
        .source_text(source_id)
        .unwrap_or_else(|error| panic!("lexical owner source lookup failed for {context}: {error}"))
        .len();
    match entry {
        OwnerEntry::Expression => {
            let parsed = parse_expression_twice(sources, source_id, context);
            assert_owner_diagnostics(
                parsed.diagnostics(),
                shape,
                owner_start,
                depth,
                source_len,
                context,
            );
            assert_expression_is_string(parsed.ast(), parsed.root());
            assert_string_count(parsed.ast(), depth);
        }
        OwnerEntry::Declaration => {
            let parsed = parse_declaration_twice(sources, source_id, context);
            assert_owner_diagnostics(
                parsed.diagnostics(),
                shape,
                owner_start,
                depth,
                source_len,
                context,
            );
            let Item::Variable { initializer, .. } = parsed
                .ast()
                .items()
                .get(parsed.root())
                .expect("lexical owner declaration root")
                .payload()
            else {
                panic!("lexical owner declaration must remain a variable")
            };
            assert_expression_is_string(parsed.ast(), *initializer);
            assert_string_count(parsed.ast(), depth);
        }
        OwnerEntry::Block => {
            let parsed = parse_block_twice(sources, source_id, context);
            assert_owner_diagnostics(
                parsed.diagnostics(),
                shape,
                owner_start,
                depth,
                source_len,
                context,
            );
            let Statement::Block { elements } = parsed
                .ast()
                .statements()
                .get(parsed.root())
                .expect("lexical owner block root")
                .payload()
            else {
                panic!("lexical owner block must retain its root")
            };
            assert_eq!(elements.len(), 1);
            let Statement::Expression { expression } = parsed
                .ast()
                .statements()
                .get(elements[0])
                .expect("lexical owner block element")
                .payload()
            else {
                panic!("lexical owner block must retain its expression")
            };
            assert_expression_is_string(parsed.ast(), *expression);
            assert_string_count(parsed.ast(), depth);
        }
        OwnerEntry::File => {
            let parsed = parse_file_twice(sources, source_id, context);
            assert_owner_diagnostics(
                parsed.diagnostics(),
                shape,
                owner_start,
                depth,
                source_len,
                context,
            );
            assert_eq!(parsed.roots().len(), 1);
            let Item::Variable { initializer, .. } = parsed
                .ast()
                .items()
                .get(parsed.roots()[0])
                .expect("lexical owner file root")
                .payload()
            else {
                panic!("lexical owner file root must remain a variable")
            };
            assert_expression_is_string(parsed.ast(), *initializer);
            assert_string_count(parsed.ast(), depth);
        }
    }
}

fn assert_rejected_owner(entry: OwnerEntry, sources: &SourceMap, lexed: &LexedFile, context: &str) {
    let expected = ParserInternalError::NestingLimitExceeded {
        limit: RECURSION_LIMIT,
    };
    match entry {
        OwnerEntry::Expression => {
            assert_parser_error_twice(sources, lexed, expected, context, parse_expression)
        }
        OwnerEntry::Declaration => {
            assert_parser_error_twice(sources, lexed, expected, context, parse_declaration)
        }
        OwnerEntry::Block => {
            assert_parser_error_twice(sources, lexed, expected, context, parse_block)
        }
        OwnerEntry::File => {
            assert_parser_error_twice(sources, lexed, expected, context, parse_file)
        }
    }
}

#[test]
fn expression_recursion_shapes_lock_their_last_accepted_and_first_rejected_depths() {
    for shape in [
        ExpressionShape::Prefix,
        ExpressionShape::Assignment,
        ExpressionShape::Elvis,
        ExpressionShape::Group,
        ExpressionShape::GenericType,
        ExpressionShape::FunctionType,
    ] {
        assert_expression_boundary(shape);
    }
}

#[test]
fn declaration_block_and_file_entries_lock_exact_recursion_boundaries() {
    let declaration_source =
        |depth| format!("val x: {}T{} = 1", "A<".repeat(depth), ">".repeat(depth));
    let (sources, source_id) = add_source(declaration_source(1_023));
    let parsed = parse_declaration_twice(&sources, source_id, "declaration type accepted boundary");
    assert!(parsed.diagnostics().is_empty());
    let (sources, source_id) = add_source(declaration_source(1_024));
    let lexed = lex_parser_source_twice(&sources, source_id, "declaration type rejected boundary");
    assert_parser_error_twice(
        &sources,
        &lexed,
        ParserInternalError::NestingLimitExceeded {
            limit: RECURSION_LIMIT,
        },
        "declaration type rejected boundary",
        parse_declaration,
    );

    let block_source = |depth| format!("{}{}", "{".repeat(depth), "}".repeat(depth));
    let (sources, source_id) = add_source(block_source(1_024));
    let parsed = parse_block_twice(&sources, source_id, "block accepted boundary");
    assert!(parsed.diagnostics().is_empty());
    let (sources, source_id) = add_source(block_source(1_025));
    let lexed = lex_parser_source_twice(&sources, source_id, "block rejected boundary");
    assert_parser_error_twice(
        &sources,
        &lexed,
        ParserInternalError::NestingLimitExceeded {
            limit: RECURSION_LIMIT,
        },
        "block rejected boundary",
        parse_block,
    );

    let file_source = |depth| format!("fun boundary() {}{}", "{".repeat(depth), "}".repeat(depth));
    let (sources, source_id) = add_source(file_source(1_024));
    let parsed = parse_file_twice(&sources, source_id, "file block accepted boundary");
    assert!(parsed.diagnostics().is_empty());
    let (sources, source_id) = add_source(file_source(1_025));
    let lexed = lex_parser_source_twice(&sources, source_id, "file block rejected boundary");
    assert_parser_error_twice(
        &sources,
        &lexed,
        ParserInternalError::NestingLimitExceeded {
            limit: RECURSION_LIMIT,
        },
        "file block rejected boundary",
        parse_file,
    );
}

#[test]
fn lexical_owners_lock_exact_recursion_boundaries_for_every_entry() {
    let mut accepted = 0;
    let mut rejected = 0;

    for entry in [
        OwnerEntry::Expression,
        OwnerEntry::Declaration,
        OwnerEntry::Block,
        OwnerEntry::File,
    ] {
        let (accepted_depth, rejected_depth) = entry.boundary();
        assert_eq!(accepted_depth + 1, rejected_depth);

        for shape in [OwnerShape::Closed, OwnerShape::Terminal] {
            let terminal = matches!(shape, OwnerShape::Terminal);
            let context = format!("{} {} accepted boundary", entry.label(), shape.label());
            let owner = shape.source(accepted_depth);
            let (source, owner_start) = entry.source(&owner, terminal);
            let (sources, source_id) = add_source(source);
            assert_accepted_owner(
                entry,
                shape,
                &sources,
                source_id,
                owner_start,
                accepted_depth,
                &context,
            );
            accepted += 1;

            let context = format!("{} {} rejected boundary", entry.label(), shape.label());
            let owner = shape.source(rejected_depth);
            let (source, owner_start) = entry.source(&owner, terminal);
            let source_len = source.len();
            let (sources, source_id) = add_source(source);
            let lexed = lex_parser_source_twice(&sources, source_id, &context);
            assert_owner_diagnostics(
                lexed.diagnostics(),
                shape,
                owner_start,
                rejected_depth,
                source_len,
                &context,
            );
            assert_rejected_owner(entry, &sources, &lexed, &context);
            rejected += 1;
        }
    }

    assert_eq!((accepted, rejected), (8, 8));
}
