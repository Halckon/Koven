//! 普通 validated unit 的封闭借用交接；不存储第二份 index 或重新决定语义。

use std::fmt;

use crate::{
    name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames, index_compilation_unit},
    source::SourceMap,
    type_checking::{CompilationUnitTypes, TypeEnvironment, ValidatedCompilationUnitTypes},
};

use super::{CompilationUnitOwnership, ValidatedCompilationUnitOwnership};

/// 已核对完整来源身份链的普通 owned compilation unit 只读借用。
///
/// 调用方继续拥有源码、AST、临时 inputs 和全部阶段产物；view 不能比其中任一借用存活更久。
/// 字段不公开，只有 [`owned_compilation_unit_view`] 能构造；常量与 recovery 产物不接受此交接。
pub struct OwnedCompilationUnitView<'view, 'parsed: 'view> {
    sources: &'view SourceMap,
    inputs: &'view [SourceUnitInput<'parsed>],
    names: &'view ValidatedCompilationUnitNames,
    _environment: &'view TypeEnvironment,
    typed: &'view ValidatedCompilationUnitTypes,
    owned: &'view ValidatedCompilationUnitOwnership,
}

impl<'view, 'parsed: 'view> OwnedCompilationUnitView<'view, 'parsed> {
    /// 返回被借用的源码集合。
    #[must_use]
    pub const fn sources(&self) -> &'view SourceMap {
        self.sources
    }

    /// 返回调用方的源码输入切片；不复制 AST。
    #[must_use]
    pub const fn inputs(&self) -> &'view [SourceUnitInput<'parsed>] {
        self.inputs
    }

    /// 返回已决定的名称身份和规范 index。
    #[must_use]
    pub const fn names(&self) -> &'view ValidatedCompilationUnitNames {
        self.names
    }

    /// 返回已决定的类型和调用事实。
    #[must_use]
    pub const fn types(&self) -> &'view CompilationUnitTypes {
        self.typed.types()
    }

    /// 返回已发布的所有权事实。
    #[must_use]
    pub const fn ownership(&self) -> &'view CompilationUnitOwnership {
        self.owned.ownership()
    }
}

/// 普通或常量专用 owned-unit 交接的来源或分析身份错误。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnedCompilationUnitViewError {
    /// 源码与 inputs 不能建立合法的 compilation-unit index。
    MismatchedSource,
    /// 合法 inputs 与名称、环境、类型或所有权产物的身份不匹配。
    MismatchedAnalysis,
}

impl fmt::Display for OwnedCompilationUnitViewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MismatchedSource => "owned compilation-unit view has mismatched source inputs",
            Self::MismatchedAnalysis => {
                "owned compilation-unit view has mismatched analysis identity"
            }
        })
    }
}

impl std::error::Error for OwnedCompilationUnitViewError {}

/// 以一次 index 重建核对普通 unit 的完整来源身份链，并封闭为只读借用。
///
/// 顺序为 source/input index、names index、签名保存的 inputs/names/environment 身份，最后核对
/// ownership 的 typed-body owner；合法 clone 与同 typed 的重新 ownership 检查保留兼容性。
pub fn owned_compilation_unit_view<'view, 'parsed: 'view>(
    sources: &'view SourceMap,
    inputs: &'view [SourceUnitInput<'parsed>],
    names: &'view ValidatedCompilationUnitNames,
    environment: &'view TypeEnvironment,
    typed: &'view ValidatedCompilationUnitTypes,
    owned: &'view ValidatedCompilationUnitOwnership,
) -> Result<OwnedCompilationUnitView<'view, 'parsed>, OwnedCompilationUnitViewError> {
    let rebuilt = index_compilation_unit(sources, inputs)
        .map_err(|_| OwnedCompilationUnitViewError::MismatchedSource)?;
    if &rebuilt != names.names().index()
        || !typed
            .types()
            .signatures()
            .is_compatible_with_index(&rebuilt, names, environment)
        || !owned.ownership().is_compatible_with(typed)
    {
        return Err(OwnedCompilationUnitViewError::MismatchedAnalysis);
    }
    Ok(OwnedCompilationUnitView {
        sources,
        inputs,
        names,
        _environment: environment,
        typed,
        owned,
    })
}
