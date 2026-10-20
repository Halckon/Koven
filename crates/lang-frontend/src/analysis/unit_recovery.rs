//! 同一 Unit 引擎的诊断恢复编排；保留真实上游资格，不提供新的执行能力。

use std::{error::Error, fmt};

use crate::{
    ownership_checking::{
        CompilationUnitOwnership, OwnershipCheckingError,
        check_compilation_unit_ownership_recovering,
    },
    type_checking::{
        CompilationUnitTypeError, CompilationUnitTypes, TypeEnvironment,
        check_compilation_unit_types_recovering,
    },
};

use super::UnitNameSnapshot;

/// 用已拥有的真实名称前缀继续共同 type/ownership producer，不重新解析或登记来源。
/// 普通源码错误留在 raw 产物，内部身份失败仍返回 Err；公开 validate 保留独立严格资格。
pub fn analyze_unit_recovery(
    names: &UnitNameSnapshot,
    environment: &TypeEnvironment,
) -> Result<(CompilationUnitTypes, CompilationUnitOwnership), UnitRecoveryAnalysisError> {
    let inputs = names.inputs();
    let typed = check_compilation_unit_types_recovering(
        names.sources(),
        &inputs,
        names.names(),
        environment,
    )
    .map_err(UnitRecoveryAnalysisError::Type)?;
    let owned = check_compilation_unit_ownership_recovering(
        names.sources(),
        &inputs,
        names.names(),
        environment,
        &typed,
    )
    .map_err(UnitRecoveryAnalysisError::Ownership)?;
    Ok((typed, owned))
}

/// Unit 恢复链的内部失败；不是可吞掉的用户诊断。
#[derive(Debug)]
pub enum UnitRecoveryAnalysisError {
    /// 类型阶段身份或尚未接通的 body 结构失败。
    Type(CompilationUnitTypeError),
    /// 所有权阶段身份或必要事实失败。
    Ownership(OwnershipCheckingError),
}

impl fmt::Display for UnitRecoveryAnalysisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Type(error) => write!(formatter, "unit type analysis failed internally: {error}"),
            Self::Ownership(error) => write!(
                formatter,
                "unit ownership analysis failed internally: {error}"
            ),
        }
    }
}

impl Error for UnitRecoveryAnalysisError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::Type(error) => error,
            Self::Ownership(error) => error,
        })
    }
}
