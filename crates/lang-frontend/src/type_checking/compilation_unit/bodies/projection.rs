use crate::{
    name_resolution::{DeclarationId, UnitSymbolId},
    type_checking::{UnitExpressionId, UnitTypeId},
};

/// 聚合投影的显式表达式或隐式 `this` receiver。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitAggregateProjectionReceiver {
    /// 源码中存在独立 receiver expression。
    Expression(UnitExpressionId),
    /// 裸 enum payload 使用当前 classifier 的隐式 `this`。
    This(DeclarationId),
}

/// compilation unit 中聚合分量投影的静态来源。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitAggregateProjectionKind {
    /// 普通主构造器字段访问。
    Field,
    /// `value class` 自动提供的 `componentN()` 调用。
    StructuralComponent,
}

/// 一个已完成泛型替换、且所有源码身份均带 source-unit 限定的聚合分量投影。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitAggregateProjectionDescriptor {
    expression: UnitExpressionId,
    receiver: UnitAggregateProjectionReceiver,
    field: UnitSymbolId,
    ty: UnitTypeId,
    kind: UnitAggregateProjectionKind,
}

impl UnitAggregateProjectionDescriptor {
    pub(crate) const fn new(
        expression: UnitExpressionId,
        receiver: UnitAggregateProjectionReceiver,
        field: UnitSymbolId,
        ty: UnitTypeId,
        kind: UnitAggregateProjectionKind,
    ) -> Self {
        Self {
            expression,
            receiver,
            field,
            ty,
            kind,
        }
    }

    /// 返回字段访问或结构分量 call expression identity。
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回聚合 receiver expression identity。
    #[must_use]
    pub const fn receiver(self) -> UnitAggregateProjectionReceiver {
        self.receiver
    }

    /// 返回 source-qualified 主构造器 field/payload symbol。
    #[must_use]
    pub const fn field(self) -> UnitSymbolId {
        self.field
    }

    /// 返回替换实际泛型实参后的字段类型。
    #[must_use]
    pub const fn ty(self) -> UnitTypeId {
        self.ty
    }

    /// 返回普通字段或自动结构分量类别。
    #[must_use]
    pub const fn kind(self) -> UnitAggregateProjectionKind {
        self.kind
    }
}
