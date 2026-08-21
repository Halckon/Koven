//! SPEC-0152 的四公开 Parser 入口递归预算精确边界矩阵。

use lang_frontend::{
    parser::{ParserInternalError, parse_block, parse_declaration, parse_expression, parse_file},
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
