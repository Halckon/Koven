//! 最终消费资格：不修改 raw/recovery facts，也不替代 basic/constant capability。

mod adapters;
mod message;
mod walk;

use super::{
    CompilationUnitTypeError, CompilationUnitTypes, TypeCheckingError, TypeEnvironment, TypedFile,
};
use crate::{
    diagnostic::{Diagnostic, Severity, ordered_diagnostics},
    name_resolution::{
        NameResolution, SourceUnitInput, ValidatedCompilationUnitNames, ordered_unit_diagnostics,
    },
    parser::ParsedFile,
    source::{SourceMap, Span},
};
use std::{error::Error, fmt, sync::Arc};

/// 最终准入失败；用户诊断与内部事实/身份缺失保持不同类别。
#[derive(Debug)]
pub enum FinalTypeAdmissionError {
    /// 非空的、稳定排序的真实源码诊断。
    Diagnostics(Vec<Diagnostic>),
    /// 不得伪装为尚未实现语言能力的内部不变量失败。
    Internal(FinalTypeAdmissionInternalError),
}

/// 最终消费所需的输入身份或已选事实不满足内部合同。
#[derive(Debug)]
pub enum FinalTypeAdmissionInternalError {
    /// 输入不属于原始 typed 分析链。
    MismatchedAnalysisIdentity,
    /// 必需值缺少类型事实或 Error 缺少其原始诊断。
    MissingRequiredFact {
        /// 缺失事实所对应的最小源码范围。
        span: Span,
        /// 所需事实类别，不冒充 DeferredReason。
        kind: &'static str,
    },
    /// 原有单文件基础设施失败。
    Type(TypeCheckingError),
    /// 原有 compilation-unit 基础设施失败。
    Unit(CompilationUnitTypeError),
}

impl fmt::Display for FinalTypeAdmissionInternalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MismatchedAnalysisIdentity => {
                f.write_str("final type admission inputs do not match the original analysis")
            }
            Self::MissingRequiredFact { span, kind } => {
                write!(f, "final type admission is missing {kind} at {span:?}")
            }
            Self::Type(error) => error.fmt(f),
            Self::Unit(error) => error.fmt(f),
        }
    }
}
impl Error for FinalTypeAdmissionInternalError {}
impl fmt::Display for FinalTypeAdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Diagnostics(diagnostics) => write!(
                f,
                "final type admission rejected {} source diagnostic(s)",
                diagnostics.len()
            ),
            Self::Internal(error) => error.fmt(f),
        }
    }
}
impl Error for FinalTypeAdmissionError {}
impl From<TypeCheckingError> for FinalTypeAdmissionInternalError {
    fn from(error: TypeCheckingError) -> Self {
        Self::Type(error)
    }
}
impl From<FinalTypeAdmissionInternalError> for FinalTypeAdmissionError {
    fn from(error: FinalTypeAdmissionInternalError) -> Self {
        Self::Internal(error)
    }
}

/// 只能由最终检查创建、借用原始单文件 typed 身份的完成资格。
#[derive(Debug)]
pub struct FinalTypedFile<'a> {
    typed: &'a TypedFile,
}
impl FinalTypedFile<'_> {
    /// 返回取得资格的原产物，不能替换为另一次分析。
    #[must_use]
    pub const fn types(&self) -> &TypedFile {
        self.typed
    }
    /// 克隆保留分析身份，重新检查相同源码则不保留。
    #[must_use]
    pub fn is_same_analysis(&self, other: &TypedFile) -> bool {
        Arc::ptr_eq(self.typed.analysis_owner(), other.analysis_owner())
    }
}

/// 只能由最终检查创建、借用原始 unit body 分析身份的完成资格。
#[derive(Debug)]
pub struct FinalCompilationUnitTypes<'a> {
    typed: &'a CompilationUnitTypes,
}
impl FinalCompilationUnitTypes<'_> {
    /// 返回取得资格的原 body typed 产物。
    #[must_use]
    pub const fn types(&self) -> &CompilationUnitTypes {
        self.typed
    }
    /// 比较 body 分析身份，而不是仅比较共享 signature identity。
    #[must_use]
    pub fn is_same_analysis(&self, other: &CompilationUnitTypes) -> bool {
        self.typed.is_same_analysis(other)
    }
}

/// 在不改变原 facts 的前提下检查单文件 ownership 消费所需的最终类型 roots。
pub fn complete_file_types<'a>(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    environment: &TypeEnvironment,
    typed: &'a TypedFile,
) -> Result<FinalTypedFile<'a>, FinalTypeAdmissionError> {
    if sources.source_text(parsed.source_id()).is_err()
        || parsed.source_id() != typed.source_id()
        || !typed.is_compatible_with_names(names)
        || !Arc::ptr_eq(typed.environment_owner(), environment.owner())
    {
        return Err(FinalTypeAdmissionInternalError::MismatchedAnalysisIdentity.into());
    }
    let mut diagnostics = parsed
        .diagnostics()
        .iter()
        .chain(names.diagnostics())
        .chain(typed.diagnostics())
        .cloned()
        .collect::<Vec<_>>();
    walk::check(
        sources,
        &[(
            parsed,
            adapters::Single {
                typed,
                names,
                environment,
            },
        )],
        &[],
        &mut diagnostics,
    )?;
    let ordered = ordered_diagnostics(sources, &diagnostics)
        .map_err(TypeCheckingError::from)
        .map_err(FinalTypeAdmissionInternalError::from)?;
    let mut diagnostics = ordered.into_iter().cloned().collect::<Vec<_>>();
    diagnostics.dedup();
    if diagnostics.iter().any(|d| d.severity() == Severity::Error) {
        return Err(FinalTypeAdmissionError::Diagnostics(diagnostics));
    }
    Ok(FinalTypedFile { typed })
}

/// 同一最终资格门覆盖 basic 与 constant 路径；只检查显式选中的 source-set。
pub fn complete_compilation_unit_types<'a>(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &'a CompilationUnitTypes,
) -> Result<FinalCompilationUnitTypes<'a>, FinalTypeAdmissionError> {
    if !typed.is_compatible_with(sources, inputs, names, environment) {
        return Err(FinalTypeAdmissionInternalError::MismatchedAnalysisIdentity.into());
    }
    let mut diagnostics = typed.diagnostics().to_vec();
    let mut files = Vec::new();
    for source in names.names().index().source_units() {
        let parsed = inputs
            .iter()
            .find(|input| input.source_id() == source.source_id())
            .map(|input| input.parsed())
            .ok_or(FinalTypeAdmissionInternalError::MismatchedAnalysisIdentity)?;
        diagnostics.extend_from_slice(parsed.diagnostics());
        files.push((
            parsed,
            adapters::Unit {
                typed,
                names,
                environment,
                source: source.id(),
            },
        ));
    }
    let definition_order = adapters::initializer_order(&files)?;
    walk::check(sources, &files, &definition_order, &mut diagnostics)?;
    let ordered =
        ordered_unit_diagnostics(sources, names.names().index().source_units(), &diagnostics)
            .map_err(CompilationUnitTypeError::from)
            .map_err(FinalTypeAdmissionInternalError::Unit)?;
    let mut diagnostics = ordered.into_iter().cloned().collect::<Vec<_>>();
    diagnostics.dedup();
    if diagnostics.iter().any(|d| d.severity() == Severity::Error) {
        return Err(FinalTypeAdmissionError::Diagnostics(diagnostics));
    }
    Ok(FinalCompilationUnitTypes { typed })
}
