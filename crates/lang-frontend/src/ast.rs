//! 保留源码范围的索引式 AST 存储骨架。

use std::{error::Error, fmt};

use crate::source::{SourceId, Span};

/// AST 存储中的节点类别。
///
/// 该枚举只标识四张物理 table，用于内部错误上下文；它不是 Koven 语法 kind。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeCategory {
    /// 顶层或成员声明。
    Item,
    /// 语句。
    Statement,
    /// 表达式。
    Expression,
    /// 类型引用。
    TypeRef,
}

/// AST 节点插入或索引访问失败。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AstError {
    /// 节点范围不属于 AST file 关联的源码。
    MismatchedSource {
        /// 被插入节点的类别。
        category: NodeCategory,
        /// AST file 关联的源码。
        expected: SourceId,
        /// 节点范围实际关联的源码。
        actual: SourceId,
    },
    /// typed ID 的下标在对应 table 之外。
    InvalidNodeId {
        /// 被查询 table 的节点类别。
        category: NodeCategory,
        /// typed ID 携带的下标。
        index: usize,
        /// table 当前节点数量。
        len: usize,
    },
}

impl fmt::Display for AstError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MismatchedSource {
                category,
                expected,
                actual,
            } => write!(
                formatter,
                "{category:?} span belongs to {actual:?}, but the AST file uses {expected:?}"
            ),
            Self::InvalidNodeId {
                category,
                index,
                len,
            } => write!(
                formatter,
                "{category:?} ID {index} is outside a table of length {len}"
            ),
        }
    }
}

impl Error for AstError {}

/// 带受检源码范围的 AST 节点 envelope。
pub struct AstNode<T> {
    span: Span,
    payload: T,
}

impl<T> fmt::Debug for AstNode<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Payload 由后续语法阶段定义，可能包含路径、地址或无序集合；稳定表示只暴露骨架。
        formatter
            .debug_struct("AstNode")
            .field("span", &self.span)
            .finish_non_exhaustive()
    }
}

impl<T> AstNode<T> {
    /// 返回节点覆盖的源码范围。
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// 返回节点 payload 的共享引用。
    #[must_use]
    pub const fn payload(&self) -> &T {
        &self.payload
    }
}

macro_rules! define_typed_table {
    ($id:ident, $table:ident, $category:expr, $category_name:literal) => {
        #[doc = concat!($category_name, "节点在所属 table 中的 typed ID。")]
        ///
        /// ID 只编码类别与下标，不携带 AST file identity。同类 ID 的跨 file 误用只有在下标
        /// 越界时才能被检测。
        #[derive(Clone, Copy, PartialEq, Eq)]
        pub struct $id(usize);

        impl fmt::Debug for $id {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_tuple(stringify!($id))
                    .field(&self.0)
                    .finish()
            }
        }

        #[doc = concat!("按插入顺序拥有", $category_name, "节点的 typed table。")]
        #[derive(Debug)]
        pub struct $table<T> {
            nodes: Vec<AstNode<T>>,
        }

        impl<T> $table<T> {
            fn new() -> Self {
                Self { nodes: Vec::new() }
            }

            fn push(&mut self, node: AstNode<T>) -> $id {
                let id = $id(self.nodes.len());
                self.nodes.push(node);
                id
            }

            #[doc = concat!("按 typed ID 读取", $category_name, "节点。")]
            ///
            /// # Errors
            ///
            /// ID 下标超出当前 table 时返回 [`AstError::InvalidNodeId`]。
            pub fn get(&self, id: $id) -> Result<&AstNode<T>, AstError> {
                self.nodes.get(id.0).ok_or(AstError::InvalidNodeId {
                    category: $category,
                    index: id.0,
                    len: self.nodes.len(),
                })
            }

            #[doc = concat!("按确定的插入顺序遍历", $category_name, "节点及其 typed ID。")]
            pub fn iter(&self) -> impl ExactSizeIterator<Item = ($id, &AstNode<T>)> {
                self.nodes
                    .iter()
                    .enumerate()
                    .map(|(index, node)| ($id(index), node))
            }

            /// 返回 table 中的节点数量。
            #[must_use]
            pub fn len(&self) -> usize {
                self.nodes.len()
            }

            /// 返回 table 是否为空。
            #[must_use]
            pub fn is_empty(&self) -> bool {
                self.nodes.is_empty()
            }
        }
    };
}

define_typed_table!(ItemId, ItemTable, NodeCategory::Item, "item ");
define_typed_table!(
    StatementId,
    StatementTable,
    NodeCategory::Statement,
    "statement "
);
define_typed_table!(
    ExpressionId,
    ExpressionTable,
    NodeCategory::Expression,
    "expression "
);
define_typed_table!(
    TypeRefId,
    TypeRefTable,
    NodeCategory::TypeRef,
    "type-reference "
);

/// 一份源码的索引式 AST 存储。
///
/// Phase 0 不定义具体 Koven 语法节点；四个 payload 类型由使用者提供。每次插入都检查节点
/// `Span` 的 [`SourceId`] 与 file 关联的 source 一致。
///
/// 以下六个编译失败示例逐对证明四类 ID 不能混用：
///
/// ```compile_fail
/// use lang_frontend::ast::{ItemId, StatementId};
/// fn cannot_mix(id: ItemId) { let _: StatementId = id; }
/// ```
///
/// ```compile_fail
/// use lang_frontend::ast::{ExpressionId, ItemId};
/// fn cannot_mix(id: ItemId) { let _: ExpressionId = id; }
/// ```
///
/// ```compile_fail
/// use lang_frontend::ast::{ItemId, TypeRefId};
/// fn cannot_mix(id: ItemId) { let _: TypeRefId = id; }
/// ```
///
/// ```compile_fail
/// use lang_frontend::ast::{ExpressionId, StatementId};
/// fn cannot_mix(id: StatementId) { let _: ExpressionId = id; }
/// ```
///
/// ```compile_fail
/// use lang_frontend::ast::{StatementId, TypeRefId};
/// fn cannot_mix(id: StatementId) { let _: TypeRefId = id; }
/// ```
///
/// ```compile_fail
/// use lang_frontend::ast::{ExpressionId, TypeRefId};
/// fn cannot_mix(id: ExpressionId) { let _: TypeRefId = id; }
/// ```
#[derive(Debug)]
pub struct AstFile<Item, Statement, Expression, TypeRef> {
    source_id: SourceId,
    items: ItemTable<Item>,
    statements: StatementTable<Statement>,
    expressions: ExpressionTable<Expression>,
    type_refs: TypeRefTable<TypeRef>,
}

impl<Item, Statement, Expression, TypeRef> AstFile<Item, Statement, Expression, TypeRef> {
    /// 创建关联单一源码、初始为空的 AST file。
    #[must_use]
    pub fn new(source_id: SourceId) -> Self {
        Self {
            source_id,
            items: ItemTable::new(),
            statements: StatementTable::new(),
            expressions: ExpressionTable::new(),
            type_refs: TypeRefTable::new(),
        }
    }

    /// 返回 AST file 关联的源码 ID。
    #[must_use]
    pub const fn source_id(&self) -> SourceId {
        self.source_id
    }

    /// 插入 item 节点。
    ///
    /// # Errors
    ///
    /// `span.source_id()` 与 file source 不一致时返回 [`AstError::MismatchedSource`]。
    pub fn add_item(&mut self, span: Span, payload: Item) -> Result<ItemId, AstError> {
        validate_source(self.source_id, span, NodeCategory::Item)?;
        Ok(self.items.push(AstNode { span, payload }))
    }

    /// 插入 statement 节点。
    ///
    /// # Errors
    ///
    /// `span.source_id()` 与 file source 不一致时返回 [`AstError::MismatchedSource`]。
    pub fn add_statement(
        &mut self,
        span: Span,
        payload: Statement,
    ) -> Result<StatementId, AstError> {
        validate_source(self.source_id, span, NodeCategory::Statement)?;
        Ok(self.statements.push(AstNode { span, payload }))
    }

    /// 插入 expression 节点。
    ///
    /// # Errors
    ///
    /// `span.source_id()` 与 file source 不一致时返回 [`AstError::MismatchedSource`]。
    pub fn add_expression(
        &mut self,
        span: Span,
        payload: Expression,
    ) -> Result<ExpressionId, AstError> {
        validate_source(self.source_id, span, NodeCategory::Expression)?;
        Ok(self.expressions.push(AstNode { span, payload }))
    }

    /// 插入 type-reference 节点。
    ///
    /// # Errors
    ///
    /// `span.source_id()` 与 file source 不一致时返回 [`AstError::MismatchedSource`]。
    pub fn add_type_ref(&mut self, span: Span, payload: TypeRef) -> Result<TypeRefId, AstError> {
        validate_source(self.source_id, span, NodeCategory::TypeRef)?;
        Ok(self.type_refs.push(AstNode { span, payload }))
    }

    /// 返回只读 item table。
    #[must_use]
    pub const fn items(&self) -> &ItemTable<Item> {
        &self.items
    }

    /// 返回只读 statement table。
    #[must_use]
    pub const fn statements(&self) -> &StatementTable<Statement> {
        &self.statements
    }

    /// 返回只读 expression table。
    #[must_use]
    pub const fn expressions(&self) -> &ExpressionTable<Expression> {
        &self.expressions
    }

    /// 返回只读 type-reference table。
    #[must_use]
    pub const fn type_refs(&self) -> &TypeRefTable<TypeRef> {
        &self.type_refs
    }
}

fn validate_source(expected: SourceId, span: Span, category: NodeCategory) -> Result<(), AstError> {
    let actual = span.source_id();
    if actual != expected {
        return Err(AstError::MismatchedSource {
            category,
            expected,
            actual,
        });
    }

    Ok(())
}
