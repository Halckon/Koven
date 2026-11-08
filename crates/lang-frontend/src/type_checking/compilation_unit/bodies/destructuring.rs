//! Typed facts for a local value-class destructuring statement.

use crate::{name_resolution::UnitSymbolId, type_checking::DestructuringMode};

use super::{UnitStatementId, UnitTypeId};

/// compilation unit 中一个已类型化的结构化解构分量。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitDestructuringComponent {
    symbol: UnitSymbolId,
    ty: UnitTypeId,
}

impl UnitDestructuringComponent {
    pub(crate) const fn new(symbol: UnitSymbolId, ty: UnitTypeId) -> Self {
        Self { symbol, ty }
    }

    /// 返回接收分量的 source-qualified binding symbol。
    #[must_use]
    pub const fn symbol(self) -> UnitSymbolId {
        self.symbol
    }

    /// 返回替换实际泛型实参后的分量类型。
    #[must_use]
    pub const fn ty(self) -> UnitTypeId {
        self.ty
    }
}

/// compilation unit 中一次精确、有效的局部 value-class 结构化解构。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitDestructuringDescriptor {
    statement: UnitStatementId,
    source_type: UnitTypeId,
    mode: DestructuringMode,
    components: Vec<UnitDestructuringComponent>,
    reachable: bool,
}

impl UnitDestructuringDescriptor {
    pub(crate) fn new(
        statement: UnitStatementId,
        source_type: UnitTypeId,
        mode: DestructuringMode,
        components: Vec<UnitDestructuringComponent>,
        reachable: bool,
    ) -> Self {
        Self {
            statement,
            source_type,
            mode,
            components,
            reachable,
        }
    }

    /// 返回带 source-unit 限定的 statement identity。
    #[must_use]
    pub const fn statement(&self) -> UnitStatementId {
        self.statement
    }

    /// 返回 initializer 的规范化源类型。
    #[must_use]
    pub const fn source_type(&self) -> UnitTypeId {
        self.source_type
    }

    /// 返回 Copy 或 Consume 原子模式。
    #[must_use]
    pub const fn mode(&self) -> DestructuringMode {
        self.mode
    }

    /// Returns whether the callable can reach this split after evaluating its initializer.
    /// This uses the frontend's conservative flow facts. Unreachable splits still have type facts.
    #[must_use]
    pub const fn is_reachable(&self) -> bool {
        self.reachable
    }

    /// 返回主构造器字段顺序的 binding / component type。
    #[must_use]
    pub fn components(&self) -> &[UnitDestructuringComponent] {
        &self.components
    }
}
