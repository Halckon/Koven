use crate::{ast::ExpressionId, name_resolution::SymbolId};

use super::TypeId;

/// 聚合分量投影的静态来源。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggregateProjectionKind {
    /// 普通主构造器字段访问。
    Field,
    /// `value class` 自动提供的 `componentN()` 调用。
    StructuralComponent,
}

/// 一个已完成泛型替换的聚合分量投影。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AggregateProjectionDescriptor {
    expression: ExpressionId,
    receiver: ExpressionId,
    field: SymbolId,
    ty: TypeId,
    kind: AggregateProjectionKind,
}

impl AggregateProjectionDescriptor {
    pub(crate) const fn new(
        expression: ExpressionId,
        receiver: ExpressionId,
        field: SymbolId,
        ty: TypeId,
        kind: AggregateProjectionKind,
    ) -> Self {
        Self {
            expression,
            receiver,
            field,
            ty,
            kind,
        }
    }

    /// 返回字段访问或 call expression identity。
    #[must_use]
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }

    /// 返回聚合 receiver expression identity。
    #[must_use]
    pub const fn receiver(self) -> ExpressionId {
        self.receiver
    }

    /// 返回主构造器字段 symbol。
    #[must_use]
    pub const fn field(self) -> SymbolId {
        self.field
    }

    /// 返回替换实际泛型实参后的字段类型。
    #[must_use]
    pub const fn ty(self) -> TypeId {
        self.ty
    }

    /// 返回普通字段或自动结构分量类别。
    #[must_use]
    pub const fn kind(self) -> AggregateProjectionKind {
        self.kind
    }
}
