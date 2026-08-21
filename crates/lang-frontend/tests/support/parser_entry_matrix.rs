//! 独立 Parser 入口矩阵共享的合法 corpus、公开产物与无 Span 结构断言。

use std::mem::{Discriminant, discriminant};

use lang_frontend::{
    lexer::LexedFile,
    parser::{
        Expression, Item, Statement, SyntaxAst, TypeRef, parse_block, parse_declaration,
        parse_expression,
    },
    source::{SourceId, SourceMap},
};

#[path = "frontend_output_assertions.rs"]
mod frontend_output_assertions;

pub(crate) use frontend_output_assertions::validate_lexed;
use frontend_output_assertions::{validate_ast, validate_diagnostics};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EntryKind {
    Expression,
    Declaration,
    Block,
}

impl EntryKind {
    pub(crate) const fn count_index(self) -> usize {
        match self {
            Self::Expression => 0,
            Self::Declaration => 1,
            Self::Block => 2,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct EntryCase {
    pub(crate) name: &'static str,
    pub(crate) kind: EntryKind,
    pub(crate) source: &'static str,
}

pub(crate) const ENTRY_CASES: &[EntryCase] = &[
    EntryCase {
        name: "expression typed named mode call",
        kind: EntryKind::Expression,
        source: "service.send<Int>(name = borrow input, &target)",
    },
    EntryCase {
        name: "expression nested control flow",
        kind: EntryKind::Expression,
        source: "if (ready) yes else when (input) { is Type -> yes; else -> no }",
    },
    EntryCase {
        name: "expression unicode move lambda",
        kind: EntryKind::Expression,
        source: r#"move { left, right -> "前${left + right}后" }"#,
    },
    EntryCase {
        name: "expression operator hierarchy",
        kind: EntryKind::Expression,
        source: "target = a ?: b || c && d == e + f * g",
    },
    EntryCase {
        name: "declaration generic function",
        kind: EntryKind::Declaration,
        source: "fun <T: Copyable> id(borrow item: T): T = item",
    },
    EntryCase {
        name: "declaration ordinary class",
        kind: EntryKind::Declaration,
        source: "public class Box<T: Copyable>(private val item: T, var count: Int): Printable { override fun show(): Unit {} }",
    },
    EntryCase {
        name: "declaration enum variants",
        kind: EntryKind::Declaration,
        source: "enum class Result<T> { Ok(payload: T), Error(message: String); fun isOk(): Boolean = true }",
    },
    EntryCase {
        name: "declaration interface companion",
        kind: EntryKind::Declaration,
        source: "interface Printable { fun show(): Unit; companion object { const val NAME: String = \"printable\" } }",
    },
    EntryCase {
        name: "block local destructuring",
        kind: EntryKind::Block,
        source: "{ val (first, second) = pair return first + second }",
    },
    EntryCase {
        name: "block conditional return",
        kind: EntryKind::Block,
        source: "{\nif (ready) { yes } else { no }\nreturn\n}",
    },
    EntryCase {
        name: "block loop family",
        kind: EntryKind::Block,
        source: "{\nfor (item in items) { call(item) }\nloop { break }\n}",
    },
    EntryCase {
        name: "block unicode lexical owners",
        kind: EntryKind::Block,
        source: r#"{ val text = "前${call('界', "内${x}")}后" return text }"#,
    },
];

#[derive(Debug, PartialEq, Eq)]
struct EntryFingerprint {
    debug: String,
    diagnostic_count: usize,
    shape: EntrySyntaxShape,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct EntrySyntaxShape {
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

fn parse_entry_once(
    case: EntryCase,
    sources: &SourceMap,
    source_id: SourceId,
    source_len: usize,
    lexed: &LexedFile,
    context: &str,
) -> EntryFingerprint {
    macro_rules! fingerprint {
        ($parsed:ident, $table:ident) => {{
            assert_eq!($parsed.source_id(), source_id);
            validate_ast(source_id, source_len, $parsed.ast());
            validate_diagnostics(source_id, source_len, $parsed.diagnostics());
            $parsed
                .ast()
                .$table()
                .get($parsed.root())
                .unwrap_or_else(|error| panic!("root failed for {context}: {error}"));
            EntryFingerprint {
                debug: format!("{:?}", $parsed),
                diagnostic_count: $parsed.diagnostics().len(),
                shape: entry_syntax_shape($parsed.ast(), $parsed.root().index()),
            }
        }};
    }

    match case.kind {
        EntryKind::Expression => {
            let parsed = parse_expression(sources, lexed)
                .unwrap_or_else(|error| panic!("expression parse failed for {context}: {error}"));
            fingerprint!(parsed, expressions)
        }
        EntryKind::Declaration => {
            let parsed = parse_declaration(sources, lexed)
                .unwrap_or_else(|error| panic!("declaration parse failed for {context}: {error}"));
            fingerprint!(parsed, items)
        }
        EntryKind::Block => {
            let parsed = parse_block(sources, lexed)
                .unwrap_or_else(|error| panic!("block parse failed for {context}: {error}"));
            fingerprint!(parsed, statements)
        }
    }
}

pub(crate) fn parse_entry_twice(
    case: EntryCase,
    sources: &SourceMap,
    source_id: SourceId,
    lexed: &LexedFile,
    context: &str,
) -> (usize, EntrySyntaxShape) {
    let source_len = sources
        .source_text(source_id)
        .expect("matrix source identity must resolve")
        .len();
    validate_lexed(source_id, source_len, lexed);
    let first = parse_entry_once(case, sources, source_id, source_len, lexed, context);
    let repeated = parse_entry_once(case, sources, source_id, source_len, lexed, context);
    assert_eq!(first, repeated, "non-deterministic parse for {context}");
    (first.diagnostic_count, first.shape)
}
