use std::{error::Error, fmt};

use crate::{
    lexer::{LexerInternalError, lex},
    name_resolution::{
        CompilationUnitInputError, CompilationUnitNameError, CompilationUnitNames, NameEnvironment,
        SourceUnitInput, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    parser::{ParsedFile, ParserInternalError, parse_file},
    source::{SourceId, SourceMap},
};

/// 一份显式内存源码的稳定 key 与原 SourceMap 身份；不含宿主 IO 信息。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitSourceDescriptor {
    root_identity: String,
    logical_path: String,
    source_id: SourceId,
}

impl UnitSourceDescriptor {
    /// 保存调用方的输入；路径和重复项由名称前缀中的既有 index 入口校验。
    #[must_use]
    pub fn new(
        root_identity: impl Into<String>,
        logical_path: impl Into<String>,
        source_id: SourceId,
    ) -> Self {
        Self {
            root_identity: root_identity.into(),
            logical_path: logical_path.into(),
            source_id,
        }
    }

    /// 返回稳定 root identity。
    #[must_use]
    pub fn root_identity(&self) -> &str {
        &self.root_identity
    }

    /// 返回调用方提供的 root-relative 逻辑路径。
    #[must_use]
    pub fn logical_path(&self) -> &str {
        &self.logical_path
    }

    /// 返回原 SourceMap 中的源码身份。
    #[must_use]
    pub const fn source_id(&self) -> SourceId {
        self.source_id
    }
}

/// 纯名称前缀的内部或输入失败；普通源码诊断保留在成功返回的 snapshot 中。
#[derive(Debug)]
pub enum UnitNameAnalysisError {
    /// Lexer 内部失败或 SourceId 不属于传入的 SourceMap。
    Lexer(LexerInternalError),
    /// Parser 内部失败。
    Parser(ParserInternalError),
    /// compilation-unit 输入不能建立规范 index。
    Input(CompilationUnitInputError),
    /// 名称解析内部失败。
    Name(CompilationUnitNameError),
}

impl fmt::Display for UnitNameAnalysisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lexer(error) => error.fmt(formatter),
            Self::Parser(error) => error.fmt(formatter),
            Self::Input(error) => error.fmt(formatter),
            Self::Name(error) => error.fmt(formatter),
        }
    }
}

impl Error for UnitNameAnalysisError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::Lexer(error) => error,
            Self::Parser(error) => error,
            Self::Input(error) => error,
            Self::Name(error) => error,
        })
    }
}

/// 共同拥有同源文本、语法、环境与名称事实的不可变前缀产物。
///
/// 不保存指向自身的借用；临时输入由调用方持有，后续阶段只借用所需字段。
pub struct UnitNameSnapshot {
    sources: SourceMap,
    descriptors: Vec<UnitSourceDescriptor>,
    parsed_files: Vec<ParsedFile>,
    environment: NameEnvironment,
    names: Result<ValidatedCompilationUnitNames, Box<CompilationUnitNames>>,
}

impl UnitNameSnapshot {
    /// 返回移动进工厂的原 SourceMap，所有 SourceId/Span 身份保持不变。
    #[must_use]
    pub const fn sources(&self) -> &SourceMap {
        &self.sources
    }

    /// 返回 canonical source-key 顺序的显式源码描述。
    #[must_use]
    pub fn descriptors(&self) -> &[UnitSourceDescriptor] {
        &self.descriptors
    }

    /// 返回与 descriptors 一一配对、按 canonical SourceUnitId 排列的完整解析产物。
    #[must_use]
    pub fn parsed_files(&self) -> &[ParsedFile] {
        &self.parsed_files
    }

    /// 返回产生当前名称事实的原名称环境。
    #[must_use]
    pub const fn environment(&self) -> &NameEnvironment {
        &self.environment
    }

    /// 返回完整名称 recovery facts 与既有聚合诊断。
    #[must_use]
    pub fn names(&self) -> &CompilationUnitNames {
        match &self.names {
            Ok(names) => names.names(),
            Err(names) => names,
        }
    }

    /// 仅当既有名称 validation 成功时返回对应能力；不替代宿主的非空诊断 gate。
    #[must_use]
    pub const fn validated_names(&self) -> Option<&ValidatedCompilationUnitNames> {
        match &self.names {
            Ok(names) => Some(names),
            Err(_) => None,
        }
    }

    /// 投影临时只读输入；调用方须持有此 Vec 直到借用其 slice 的后续 view 使用结束。
    #[must_use]
    pub fn inputs(&self) -> Vec<SourceUnitInput<'_>> {
        inputs(&self.descriptors, &self.parsed_files)
    }
}

fn inputs<'a>(
    descriptors: &'a [UnitSourceDescriptor],
    parsed_files: &'a [ParsedFile],
) -> Vec<SourceUnitInput<'a>> {
    descriptors
        .iter()
        .zip(parsed_files)
        .map(|(unit, parsed)| {
            SourceUnitInput::new(
                unit.root_identity(),
                unit.logical_path(),
                unit.source_id(),
                parsed,
            )
        })
        .collect()
}

/// 运行显式内存 source set 的 lex→parse→index→names，不执行宿主 IO 或后续类型检查。
///
/// 逐 descriptor 按输入顺序完成 lex/parse，再进入 index；因此 foreign SourceId 的
/// `Lexer(Source(_))` 先于 index 才能发现的非法 logical path。到达 index 后保留其原错误选择。
/// 普通用户诊断不中断此前缀；返回的 recovery snapshot 不会伪装成 validated 名称能力。
pub fn analyze_unit_names(
    sources: SourceMap,
    descriptors: Vec<UnitSourceDescriptor>,
    environment: NameEnvironment,
) -> Result<UnitNameSnapshot, UnitNameAnalysisError> {
    let parsed_files = descriptors
        .iter()
        .map(|unit| {
            let lexed = lex(&sources, unit.source_id()).map_err(UnitNameAnalysisError::Lexer)?;
            parse_file(&sources, &lexed).map_err(UnitNameAnalysisError::Parser)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let source_inputs = inputs(&descriptors, &parsed_files);
    let index =
        index_compilation_unit(&sources, &source_inputs).map_err(UnitNameAnalysisError::Input)?;
    let names = resolve_compilation_unit_names(&sources, &source_inputs, &index, &environment)
        .map_err(UnitNameAnalysisError::Name)?;
    // 移动完整配对而非 AST clone，公开连续 ParsedFile 切片与 index 共用规范顺序。
    let mut paired = descriptors
        .into_iter()
        .zip(parsed_files)
        .collect::<Vec<_>>();
    paired.sort_by(|(left, _), (right, _)| {
        (left.root_identity(), left.logical_path())
            .cmp(&(right.root_identity(), right.logical_path()))
    });
    let (descriptors, parsed_files) = paired.into_iter().unzip();
    let names = names.validate();
    Ok(UnitNameSnapshot {
        sources,
        descriptors,
        parsed_files,
        environment,
        names,
    })
}
