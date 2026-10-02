use crate::{
    name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, OwnershipCheckingError, check_compilation_unit_ownership,
    },
    source::SourceMap,
    type_checking::{CompilationUnitTypes, TypeEnvironment, ValidatedCompilationUnitTypes},
};

/// 基础类型能力验证后的分流结果；按值拥有事实，不借用输入或证明额外身份。
///
/// 成功分支保存 validated typed 与原始 ownership（可含诊断或 deferred）；
/// NotBasic 分支原样保存 validation 返回的 boxed typed recovery，不尝试常量能力。
pub struct BasicOwnershipOutcome {
    result: Result<
        (ValidatedCompilationUnitTypes, CompilationUnitOwnership),
        Box<CompilationUnitTypes>,
    >,
}

impl BasicOwnershipOutcome {
    /// 取回按值阶段产物；Err 仅表示 NotBasic 分流，不是 ownership 内部错误。
    pub fn into_result(
        self,
    ) -> Result<(ValidatedCompilationUnitTypes, CompilationUnitOwnership), Box<CompilationUnitTypes>>
    {
        self.result
    }
}

/// 只推进 typed 基础能力验证与普通 ownership，不执行宿主诊断 gate 或 const fallback。
///
/// 必须先消费 typed validation：NotBasic 不核验其他输入的身份，直接交回原 Box；
/// basic 成功后由原 checker 核验输入身份并产生 raw ownership。外层 Err 保留其内部错误。
/// 调用方自行聚合诊断、选择 const、验证 ownership 或选择 entry；本函数不复制 typed。
pub fn analyze_basic_unit_ownership(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: CompilationUnitTypes,
) -> Result<BasicOwnershipOutcome, OwnershipCheckingError> {
    let result = match typed.validate() {
        Ok(typed) => {
            let owned =
                check_compilation_unit_ownership(sources, inputs, names, environment, &typed)?;
            Ok((typed, owned))
        }
        Err(typed) => Err(typed),
    };
    Ok(BasicOwnershipOutcome { result })
}
