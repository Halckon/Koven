use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    diagnostic::Diagnostic,
    source::{SourceId, Span},
};

use super::{ExpressionAst, SyntaxAst};

/// 拥有完整文件 AST、有序顶层声明与两阶段有序诊断的解析产物。
#[derive(Debug)]
pub struct ParsedFile {
    pub(crate) ast: SyntaxAst,
    pub(crate) package: Option<PackageDirective>,
    // 只记录初始 package 名称及紧随它的分隔符；没有初始 package 时为 true。
    pub(super) initial_package_header_complete: bool,
    pub(crate) imports: Vec<ImportDirective>,
    pub(crate) roots: Vec<ItemId>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl ParsedFile {
    /// 返回产物关联的源码身份。
    #[must_use]
    pub const fn source_id(&self) -> SourceId {
        self.ast.source_id()
    }

    /// 返回只读具体 AST。
    #[must_use]
    pub const fn ast(&self) -> &SyntaxAst {
        &self.ast
    }

    /// 返回可选的文件 package directive。
    #[must_use]
    pub const fn package(&self) -> Option<&PackageDirective> {
        self.package.as_ref()
    }

    /// 初始 package 名称和紧随它的分隔符是否完整；无初始 package 时为 true。
    #[must_use]
    pub(crate) const fn initial_package_header_is_complete(&self) -> bool {
        self.initial_package_header_complete
    }

    /// 返回按源码顺序排列的 import directives。
    #[must_use]
    pub fn imports(&self) -> &[ImportDirective] {
        &self.imports
    }

    /// 返回按源码顺序排列的顶层声明。
    #[must_use]
    pub fn roots(&self) -> &[ItemId] {
        &self.roots
    }

    /// 返回 Lexer 与 Parser 诊断的确定性合并全序。
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// 点分限定名中的一个真实标识符 segment。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QualifiedNameSegment {
    /// segment 的真实源码范围。
    pub span: Span,
}

/// 文件首部可选的 `package` directive。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageDirective {
    /// 从 `package` 到最后实际消费 segment 的范围。
    pub span: Span,
    /// 真实 `package` token。
    pub keyword_span: Span,
    /// 源码顺序的点分名称 segment。
    pub segments: Vec<QualifiedNameSegment>,
}

/// exact import 的可选别名。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportAlias {
    /// 真实 `as` token。
    pub as_span: Span,
    /// 真实别名；缺失恢复时为空范围。
    pub name_span: Span,
}

/// 文件首部一个源码有序的 `import` directive。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportDirective {
    /// 从 `import` 到 alias、wildcard 或最后 segment 的范围。
    pub span: Span,
    /// 真实 `import` token。
    pub keyword_span: Span,
    /// 源码顺序的点分目标 segment。
    pub segments: Vec<QualifiedNameSegment>,
    /// 末尾 `*`；exact import 为 `None`。
    pub wildcard_span: Option<Span>,
    /// exact import 的可选 `as` alias。
    pub alias: Option<ImportAlias>,
}

/// 拥有具体 AST、根节点与两阶段有序诊断的解析产物。
#[derive(Debug)]
pub struct ParsedExpression {
    pub(crate) ast: ExpressionAst,
    pub(crate) root: ExpressionId,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl ParsedExpression {
    /// 返回产物关联的源码身份。
    #[must_use]
    pub const fn source_id(&self) -> SourceId {
        self.ast.source_id()
    }

    /// 返回只读具体 AST。
    #[must_use]
    pub const fn ast(&self) -> &ExpressionAst {
        &self.ast
    }

    /// 返回独立表达式根节点 ID。
    #[must_use]
    pub const fn root(&self) -> ExpressionId {
        self.root
    }

    /// 返回 Lexer 与 Parser 诊断的确定性合并全序。
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// 拥有具体 AST、根声明与两阶段有序诊断的解析产物。
#[derive(Debug)]
pub struct ParsedDeclaration {
    pub(crate) ast: SyntaxAst,
    pub(crate) root: ItemId,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl ParsedDeclaration {
    /// 返回产物关联的源码身份。
    #[must_use]
    pub const fn source_id(&self) -> SourceId {
        self.ast.source_id()
    }

    /// 返回只读具体 AST。
    #[must_use]
    pub const fn ast(&self) -> &SyntaxAst {
        &self.ast
    }

    /// 返回独立声明根节点 ID。
    #[must_use]
    pub const fn root(&self) -> ItemId {
        self.root
    }

    /// 返回 Lexer 与 Parser 诊断的确定性合并全序。
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// 拥有具体 AST、唯一 block 根节点与两阶段有序诊断的解析产物。
#[derive(Debug)]
pub struct ParsedBlock {
    pub(crate) ast: SyntaxAst,
    pub(crate) root: StatementId,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl ParsedBlock {
    /// 返回产物关联的源码身份。
    #[must_use]
    pub const fn source_id(&self) -> SourceId {
        self.ast.source_id()
    }

    /// 返回只读具体 AST。
    #[must_use]
    pub const fn ast(&self) -> &SyntaxAst {
        &self.ast
    }

    /// 返回独立 block 根节点 ID。
    #[must_use]
    pub const fn root(&self) -> StatementId {
        self.root
    }

    /// 返回 Lexer 与 Parser 诊断的确定性合并全序。
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        diagnostic::{Severity, codes},
        lexer::lex,
        parser::{
            Expression, FunctionForm, IntegerLiteralKind, Item, LiteralKind, NameMarker, Statement,
            TypePathSegment, TypeRef, VariableKind, parse_file,
        },
        source::SourceMap,
    };

    // 逐项读取公开 table 并锁定全部 payload，不使用 AST Debug。
    macro_rules! assert_nodes {
        ($table:expr, $expected:expr) => {
            assert_eq!(
                $table
                    .iter()
                    .map(|(id, node)| (id.index(), node.span(), node.payload().clone()))
                    .collect::<Vec<_>>(),
                $expected,
            );
        };
    }

    type DiagnosticOracle = (&'static str, &'static str, usize, usize);

    fn parsed(text: &str) -> (SourceMap, ParsedFile) {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source("header.ko", text)
            .expect("unique source");
        let lexed = lex(&sources, source).expect("real source lexes");
        let parsed = parse_file(&sources, &lexed).expect("real lexed file parses");
        assert_eq!(parsed.source_id(), source);
        assert_eq!(parsed.ast().source_id(), source);
        (sources, parsed)
    }

    fn span(sources: &SourceMap, parsed: &ParsedFile, start: usize, end: usize) -> Span {
        sources
            .span(parsed.source_id(), start, end)
            .expect("fixture span")
    }

    fn package(
        sources: &SourceMap,
        parsed: &ParsedFile,
        end: usize,
        segments: &[(usize, usize)],
    ) -> PackageDirective {
        PackageDirective {
            span: span(sources, parsed, 0, end),
            keyword_span: span(sources, parsed, 0, 7),
            segments: segments
                .iter()
                .map(|&(start, end)| QualifiedNameSegment {
                    span: span(sources, parsed, start, end),
                })
                .collect(),
        }
    }

    fn exact_import(
        sources: &SourceMap,
        parsed: &ParsedFile,
        start: usize,
        end: usize,
        segments: &[(usize, usize)],
    ) -> ImportDirective {
        ImportDirective {
            span: span(sources, parsed, start, end),
            keyword_span: span(sources, parsed, start, start + 6),
            segments: segments
                .iter()
                .map(|&(start, end)| QualifiedNameSegment {
                    span: span(sources, parsed, start, end),
                })
                .collect(),
            wildcard_span: None,
            alias: None,
        }
    }

    fn assert_diagnostics(sources: &SourceMap, parsed: &ParsedFile, expected: &[DiagnosticOracle]) {
        let catalog = codes::catalog().expect("production code catalog");
        let expected: Vec<_> = expected
            .iter()
            .map(|&(code, message, start, end)| {
                Diagnostic::new(
                    sources,
                    Severity::Error,
                    catalog.resolve(code).expect("registered code"),
                    message,
                    span(sources, parsed, start, end),
                )
                .expect("fixed diagnostic oracle")
            })
            .collect();
        // Diagnostic 的完整 Eq 包括 severity/code/message/primary/有序 details。
        assert_eq!(parsed.diagnostics(), expected);
    }

    fn assert_variable_tail(
        sources: &SourceMap,
        parsed: &ParsedFile,
        start: usize,
        errors: &[(usize, usize)],
    ) {
        let s = |start, end| span(sources, parsed, start, end);
        let initializer = parsed.ast().expressions().iter().next().expect("literal").0;
        assert_nodes!(
            parsed.ast().expressions(),
            [(
                0,
                s(start + 6, start + 7),
                Expression::Literal(LiteralKind::Integer(IntegerLiteralKind::Unsuffixed)),
            )]
        );
        let mut items: Vec<_> = errors
            .iter()
            .enumerate()
            .map(|(index, &(start, end))| (index, s(start, end), Item::Error))
            .collect();
        items.push((
            errors.len(),
            s(start, start + 7),
            Item::Variable {
                kind: VariableKind::Val,
                name: NameMarker::Present(s(start + 4, start + 5)),
                colon_span: None,
                type_ref: None,
                equals_span: s(start + 5, start + 6),
                initializer,
            },
        ));
        assert_nodes!(parsed.ast().items(), items);
        assert_eq!(
            parsed
                .roots()
                .iter()
                .map(|id| id.index())
                .collect::<Vec<_>>(),
            (0..=errors.len()).collect::<Vec<_>>()
        );
        assert!(parsed.ast().statements().is_empty());
        assert!(parsed.ast().type_refs().is_empty());
    }

    #[test]
    fn initial_package_header_fact_preserves_parser_products() {
        let (sources, file) = parsed("val x=1");
        assert!(file.initial_package_header_complete);
        assert_eq!(file.package(), None);
        assert!(file.imports().is_empty());
        assert_diagnostics(&sources, &file, &[]);
        assert_variable_tail(&sources, &file, 0, &[]);

        for (text, tail) in [
            ("package a.b", None),
            ("package a.b\nval x=1", Some(12)),
            ("package a.b;val x=1", Some(12)),
        ] {
            let (sources, file) = parsed(text);
            assert!(file.initial_package_header_complete, "{text}");
            assert_eq!(
                file.package(),
                Some(&package(&sources, &file, 11, &[(8, 9), (10, 11)]))
            );
            assert!(file.imports().is_empty());
            assert_diagnostics(&sources, &file, &[]);
            if let Some(start) = tail {
                assert_variable_tail(&sources, &file, start, &[]);
            } else {
                assert!(file.roots().is_empty());
                assert!(file.ast().items().is_empty());
                assert!(file.ast().statements().is_empty());
                assert!(file.ast().expressions().is_empty());
                assert!(file.ast().type_refs().is_empty());
            }
        }

        struct Case {
            text: &'static str,
            package_end: usize,
            segments: &'static [(usize, usize)],
            tail: usize,
            diagnostics: &'static [DiagnosticOracle],
        }
        for case in [
            Case {
                text: "package\nval x=1",
                package_end: 7,
                segments: &[],
                tail: 8,
                diagnostics: &[("L0048", "expected package name", 8, 8)],
            },
            Case {
                text: "package a.\nval x=1",
                package_end: 10,
                segments: &[(8, 9)],
                tail: 11,
                diagnostics: &[("L0048", "expected package name", 11, 11)],
            },
            Case {
                text: "package a.$ junk\nval x=1",
                package_end: 16,
                segments: &[(8, 9)],
                tail: 17,
                diagnostics: &[("L0001", "unexpected character", 10, 11)],
            },
            Case {
                text: "package a.b val x=1",
                package_end: 11,
                segments: &[(8, 9), (10, 11)],
                tail: 12,
                diagnostics: &[("L0053", "expected file header separator", 12, 15)],
            },
            Case {
                text: "package val x=1",
                package_end: 7,
                segments: &[],
                tail: 8,
                diagnostics: &[
                    ("L0048", "expected package name", 8, 8),
                    ("L0053", "expected file header separator", 8, 11),
                ],
            },
        ] {
            let (sources, file) = parsed(case.text);
            assert!(!file.initial_package_header_complete, "{}", case.text);
            assert_eq!(
                file.package(),
                Some(&package(&sources, &file, case.package_end, case.segments))
            );
            assert!(file.imports().is_empty());
            assert_diagnostics(&sources, &file, case.diagnostics);
            assert_variable_tail(&sources, &file, case.tail, &[]);
        }

        let (sources, file) = parsed("package $ junk\nimport valid.name\nval x=1");
        assert!(!file.initial_package_header_complete);
        assert_eq!(file.package(), Some(&package(&sources, &file, 14, &[])));
        assert_eq!(
            file.imports(),
            [exact_import(&sources, &file, 15, 32, &[(22, 27), (28, 32)])]
        );
        assert_diagnostics(&sources, &file, &[("L0001", "unexpected character", 8, 9)]);
        assert_variable_tail(&sources, &file, 33, &[]);

        let (sources, file) = parsed("package a.b\nval x:Int=1");
        let s = |start, end| span(&sources, &file, start, end);
        assert!(file.initial_package_header_complete);
        assert_eq!(
            file.package(),
            Some(&package(&sources, &file, 11, &[(8, 9), (10, 11)]))
        );
        assert!(file.imports().is_empty());
        assert_diagnostics(&sources, &file, &[]);
        let initializer = file.ast().expressions().iter().next().expect("literal").0;
        let type_ref = file.ast().type_refs().iter().next().expect("Int").0;
        assert_nodes!(
            file.ast().expressions(),
            [(
                0,
                s(22, 23),
                Expression::Literal(LiteralKind::Integer(IntegerLiteralKind::Unsuffixed)),
            )]
        );
        assert_nodes!(
            file.ast().type_refs(),
            [(
                0,
                s(18, 21),
                TypeRef::Qualified {
                    segments: vec![TypePathSegment {
                        name_span: s(18, 21),
                        arguments: Vec::new(),
                    }],
                    nullable_span: None,
                },
            )]
        );
        assert_nodes!(
            file.ast().items(),
            [(
                0,
                s(12, 23),
                Item::Variable {
                    kind: VariableKind::Val,
                    name: NameMarker::Present(s(16, 17)),
                    colon_span: Some(s(17, 18)),
                    type_ref: Some(type_ref),
                    equals_span: s(21, 22),
                    initializer,
                },
            )]
        );
        assert_eq!(
            file.roots().iter().map(|id| id.index()).collect::<Vec<_>>(),
            [0]
        );
        assert!(file.ast().statements().is_empty());
    }

    #[test]
    fn later_errors_do_not_change_initial_package_header_fact() {
        for (text, code, message) in [
            ("package a.b\n#\nval x=1", "L0001", "unexpected character"),
            ("package a.b\n@\nval x=1", "L0017", "expected declaration"),
        ] {
            let (sources, file) = parsed(text);
            assert!(file.initial_package_header_complete, "{text}");
            assert_eq!(
                file.package(),
                Some(&package(&sources, &file, 11, &[(8, 9), (10, 11)]))
            );
            assert!(file.imports().is_empty());
            assert_diagnostics(&sources, &file, &[(code, message, 12, 13)]);
            assert_variable_tail(&sources, &file, 14, &[(12, 13)]);
        }

        let (sources, file) = parsed("package a.b\nimport q.X val x=1");
        assert!(file.initial_package_header_complete);
        assert_eq!(
            file.package(),
            Some(&package(&sources, &file, 11, &[(8, 9), (10, 11)]))
        );
        assert_eq!(
            file.imports(),
            [exact_import(&sources, &file, 12, 22, &[(19, 20), (21, 22)])]
        );
        assert_diagnostics(
            &sources,
            &file,
            &[("L0053", "expected file header separator", 23, 26)],
        );
        assert_variable_tail(&sources, &file, 23, &[]);

        let (sources, file) = parsed("package a.b\nimport\nval x=1");
        assert!(file.initial_package_header_complete);
        assert_eq!(
            file.package(),
            Some(&package(&sources, &file, 11, &[(8, 9), (10, 11)]))
        );
        assert_eq!(file.imports(), [exact_import(&sources, &file, 12, 18, &[])]);
        assert_diagnostics(
            &sources,
            &file,
            &[("L0049", "expected import target", 19, 19)],
        );
        assert_variable_tail(&sources, &file, 19, &[]);

        let (sources, file) = parsed("import q.X\npackage\nval x=1");
        assert!(file.initial_package_header_complete);
        assert_eq!(file.package(), None);
        assert_eq!(
            file.imports(),
            [exact_import(&sources, &file, 0, 10, &[(7, 8), (9, 10)])]
        );
        assert_diagnostics(
            &sources,
            &file,
            &[
                ("L0051", "misplaced package directive", 11, 18),
                ("L0048", "expected package name", 19, 19),
            ],
        );
        assert_variable_tail(&sources, &file, 19, &[(11, 18)]);

        let (sources, file) = parsed("package a.b\npackage\nfun main(){missing}");
        let s = |start, end| span(&sources, &file, start, end);
        assert!(file.initial_package_header_complete);
        assert_eq!(
            file.package(),
            Some(&package(&sources, &file, 11, &[(8, 9), (10, 11)]))
        );
        assert!(file.imports().is_empty());
        assert_diagnostics(
            &sources,
            &file,
            &[
                ("L0051", "misplaced package directive", 12, 19),
                ("L0048", "expected package name", 20, 20),
            ],
        );
        let expression = file.ast().expressions().iter().next().expect("missing").0;
        let element = file.ast().statements().iter().next().expect("element").0;
        let body = file.ast().statements().iter().nth(1).expect("block").0;
        assert_nodes!(file.ast().expressions(), [(0, s(31, 38), Expression::Name)]);
        assert_nodes!(
            file.ast().statements(),
            [
                (0, s(31, 38), Statement::Expression { expression }),
                (
                    1,
                    s(30, 39),
                    Statement::Block {
                        elements: vec![element]
                    }
                ),
            ]
        );
        assert_nodes!(
            file.ast().items(),
            [
                (0, s(12, 19), Item::Error),
                (
                    1,
                    s(20, 39),
                    Item::Function {
                        name: NameMarker::Present(s(24, 28)),
                        extension_receiver: None,
                        type_parameters: Vec::new(),
                        type_parameter_list_span: None,
                        parameters: Vec::new(),
                        form: FunctionForm::ImplicitUnitBlock(body),
                    },
                ),
            ]
        );
        assert_eq!(
            file.roots().iter().map(|id| id.index()).collect::<Vec<_>>(),
            [0, 1]
        );
        assert!(file.ast().type_refs().is_empty());
    }
}
