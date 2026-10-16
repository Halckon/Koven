//! Unit for 的精确 provider、值交付与 borrowed projection；不授予 native capability。
use super::{CompilationUnitTypes, UnitExpressionId, UnitStatementId, UnitTypeId, UnitTypeKind};
use crate::{
    name_resolution::UnitSymbolId,
    type_checking::{BuiltinType, IterationProvider, ParameterMode},
};

/// 当前轮 binding，所有 identity 均限定所属 source；交付模式由 provider 决定。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitSequentialIterationBinding {
    /// `_` 不声明 symbol。
    Discard,
    /// 绑定当前 element。
    Name(UnitSymbolId),
    /// 按主构造器字段顺序的 borrowed projection，包含 discard。
    Destructure(Vec<UnitSequentialIterationComponent>),
}

impl UnitSequentialIterationBinding {
    /// 返回源码顺序的具名 binding；discard 不产生 symbol。
    pub fn symbols(&self) -> impl Iterator<Item = UnitSymbolId> + '_ {
        let name = match self {
            Self::Name(symbol) => Some(*symbol),
            _ => None,
        };
        let components = match self {
            Self::Destructure(parts) => parts.as_slice(),
            _ => &[],
        };
        name.into_iter()
            .chain(components.iter().filter_map(|part| part.symbol))
    }
}

/// 字段声明身份与本地 binding 分离；类型已替换实际泛型参数。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitSequentialIterationComponent {
    pub(crate) field: UnitSymbolId,
    pub(crate) symbol: Option<UnitSymbolId>,
    pub(crate) ty: UnitTypeId,
}

impl UnitSequentialIterationComponent {
    /// 原始主构造器字段身份，可来自另一个 source。
    #[must_use]
    pub const fn field(&self) -> UnitSymbolId {
        self.field
    }
    /// 本轮 binding；discard 为 None。
    #[must_use]
    pub const fn symbol(&self) -> Option<UnitSymbolId> {
        self.symbol
    }
    /// 实例化后的分量类型。
    #[must_use]
    pub const fn ty(&self) -> UnitTypeId {
        self.ty
    }
}

/// 仅由完整 Phase 2 检查发布，source 表达式只求值一次。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitSequentialIterationDescriptor {
    pub(crate) statement: UnitStatementId,
    pub(crate) source: UnitExpressionId,
    pub(crate) source_type: UnitTypeId,
    pub(crate) provider: IterationProvider,
    pub(crate) element_type: UnitTypeId,
    pub(crate) binding: UnitSequentialIterationBinding,
}

impl UnitSequentialIterationDescriptor {
    /// source-qualified for identity。
    #[must_use]
    pub const fn statement(&self) -> UnitStatementId {
        self.statement
    }
    /// 唯一 source 求值 identity。
    #[must_use]
    pub const fn source(&self) -> UnitExpressionId {
        self.source
    }
    /// source 的精确 provider 类型。
    #[must_use]
    pub const fn source_type(&self) -> UnitTypeId {
        self.source_type
    }
    /// compiler-bound provider 身份。
    #[must_use]
    pub const fn provider(&self) -> IterationProvider {
        self.provider
    }
    /// 精确 element 类型。
    #[must_use]
    pub const fn element_type(&self) -> UnitTypeId {
        self.element_type
    }
    /// 本轮 binding 与投影。
    #[must_use]
    pub const fn binding(&self) -> &UnitSequentialIterationBinding {
        &self.binding
    }
    /// 整数 Range 交付 owned Copyable 值，顺序容器与 View 保持 shared Borrow。
    #[must_use]
    pub const fn delivery(&self) -> ParameterMode {
        self.provider.delivery()
    }

    pub(crate) fn numeric_range_is_valid(&self, typed: &CompilationUnitTypes) -> bool {
        let (endpoint, range) = match self.provider {
            IterationProvider::NumericRange(BuiltinType::Int) => {
                (BuiltinType::Int, BuiltinType::IntRange)
            }
            IterationProvider::NumericRange(BuiltinType::Long) => {
                (BuiltinType::Long, BuiltinType::LongRange)
            }
            _ => return false,
        };
        self.statement.source_unit() == self.source.source_unit()
            && typed.expression_type(self.source) == Some(self.source_type)
            && typed.types().get(self.source_type) == Some(&UnitTypeKind::Builtin(range))
            && typed.types().get(self.element_type) == Some(&UnitTypeKind::Builtin(endpoint))
            && match &self.binding {
                UnitSequentialIterationBinding::Discard => true,
                UnitSequentialIterationBinding::Name(symbol) => {
                    symbol.source_unit() == self.statement.source_unit()
                        && typed.symbol_type(*symbol) == Some(self.element_type)
                }
                UnitSequentialIterationBinding::Destructure(_) => false,
            }
    }
}

impl CompilationUnitTypes {
    pub(super) fn numeric_range_iterations_are_valid(&self) -> bool {
        self.sequential_iterations()
            .windows(2)
            .all(|plans| plans[0].statement() < plans[1].statement())
            && self
                .sequential_iterations()
                .iter()
                .all(|plan| match plan.provider() {
                    IterationProvider::NumericRange(_) => plan.numeric_range_is_valid(self),
                    IterationProvider::Sequential(_) | IterationProvider::RangeView => true,
                })
    }
}
