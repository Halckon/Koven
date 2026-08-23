use crate::name_resolution::SymbolId;

use super::TypeId;

/// callable 参数的类型级模式。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ParameterMode {
    /// Owned delivery，源码声明侧拼写为 `own`。
    Value,
    /// Shared borrow，源码声明侧无 marker 或显式 `borrow`。
    Borrow,
    /// Exclusive inout borrow.
    Inout,
}

/// 函数类型中的规范化参数。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionParameterType {
    /// Parameter passing mode.
    pub mode: ParameterMode,
    /// Parameter type identity.
    pub ty: TypeId,
}

/// 已采用 callable contract 的源码参数绑定。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParameterBindingDescriptor {
    symbol: SymbolId,
    mode: ParameterMode,
}

impl ParameterBindingDescriptor {
    pub(crate) const fn new(symbol: SymbolId, mode: ParameterMode) -> Self {
        Self { symbol, mode }
    }

    /// 返回参数绑定的稳定源码 symbol。
    #[must_use]
    pub const fn symbol(self) -> SymbolId {
        self.symbol
    }

    /// 返回最终采用的 Value/Borrow/Inout 契约。
    #[must_use]
    pub const fn mode(self) -> ParameterMode {
        self.mode
    }
}
