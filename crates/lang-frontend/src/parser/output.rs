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
