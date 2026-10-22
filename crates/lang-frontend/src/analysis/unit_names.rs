use std::{error::Error, fmt};

use crate::{
    lexer::{LexerInternalError, lex},
    name_resolution::{
        CompilationUnitInputError, CompilationUnitNameError, CompilationUnitNames, NameEnvironment,
        SourceUnitInput, SourceUnitInputKey, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    parser::{ParsedFile, ParserInternalError, parse_file},
    source::{SourceId, SourceMap},
};

/// 一份显式源码的稳定 key 与原 SourceMap 身份；不含宿主 IO 信息。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitSourceDescriptor {
    key: UnitSourceDescriptorKey,
    source_id: SourceId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum UnitSourceDescriptorKey {
    Rooted {
        root_identity: String,
        logical_path: String,
    },
    MemoryDocument,
}

impl UnitSourceDescriptor {
    /// 保存 rooted 输入；路径和重复项由既有 index 入口校验。
    #[must_use]
    pub fn new(
        root_identity: impl Into<String>,
        logical_path: impl Into<String>,
        source_id: SourceId,
    ) -> Self {
        Self {
            key: UnitSourceDescriptorKey::Rooted {
                root_identity: root_identity.into(),
                logical_path: logical_path.into(),
            },
            source_id,
        }
    }

    /// 保存独立内存文档的实际来源，不从 locator 或 package 生成路径。
    #[must_use]
    pub const fn memory_document(source_id: SourceId) -> Self {
        Self {
            key: UnitSourceDescriptorKey::MemoryDocument,
            source_id,
        }
    }

    /// 返回保留输入种类的只读原始 key。
    #[must_use]
    pub fn input_key(&self) -> SourceUnitInputKey<'_> {
        match &self.key {
            UnitSourceDescriptorKey::Rooted {
                root_identity,
                logical_path,
            } => SourceUnitInputKey::Rooted {
                root_identity,
                logical_path,
            },
            UnitSourceDescriptorKey::MemoryDocument => SourceUnitInputKey::MemoryDocument,
        }
    }

    /// 投影同一 descriptor 与真实 ParsedFile，不重建或登记 source identity。
    #[must_use]
    pub fn as_input<'a>(&'a self, parsed: &'a ParsedFile) -> SourceUnitInput<'a> {
        match self.input_key() {
            SourceUnitInputKey::Rooted {
                root_identity,
                logical_path,
            } => SourceUnitInput::new(root_identity, logical_path, self.source_id, parsed),
            SourceUnitInputKey::MemoryDocument => {
                SourceUnitInput::memory_document(self.source_id, parsed)
            }
        }
    }

    /// 仅 rooted 输入有稳定 root identity。
    #[must_use]
    pub fn root_identity(&self) -> Option<&str> {
        match &self.key {
            UnitSourceDescriptorKey::Rooted { root_identity, .. } => Some(root_identity),
            UnitSourceDescriptorKey::MemoryDocument => None,
        }
    }

    /// 仅 rooted 输入有调用方提供的 root-relative 逻辑路径。
    #[must_use]
    pub fn logical_path(&self) -> Option<&str> {
        match &self.key {
            UnitSourceDescriptorKey::Rooted { logical_path, .. } => Some(logical_path),
            UnitSourceDescriptorKey::MemoryDocument => None,
        }
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
        .map(|(unit, parsed)| unit.as_input(parsed))
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
    let parsed_sources = descriptors
        .into_iter()
        .map(|unit| {
            let lexed = lex(&sources, unit.source_id()).map_err(UnitNameAnalysisError::Lexer)?;
            let parsed = parse_file(&sources, &lexed).map_err(UnitNameAnalysisError::Parser)?;
            Ok((unit, parsed))
        })
        .collect::<Result<Vec<_>, UnitNameAnalysisError>>()?;
    analyze_parsed_unit_names(sources, parsed_sources, environment)
}

/// 续接既有解析产物；配对与原 SourceMap 一起移动，不重新 lex/parse 或登记 SourceId。
///
/// 原 index 继续验证 descriptor、ParsedFile 和 SourceMap 身份；普通诊断保留在 recovery
/// snapshot，不授权 validated 名称能力。调用方负责为每份显式 source 提供真实解析产物。
pub fn analyze_parsed_unit_names(
    sources: SourceMap,
    mut parsed_sources: Vec<(UnitSourceDescriptor, ParsedFile)>,
    environment: NameEnvironment,
) -> Result<UnitNameSnapshot, UnitNameAnalysisError> {
    let source_inputs = parsed_sources
        .iter()
        .map(|(unit, parsed)| unit.as_input(parsed))
        .collect::<Vec<_>>();
    let index =
        index_compilation_unit(&sources, &source_inputs).map_err(UnitNameAnalysisError::Input)?;
    let names = resolve_compilation_unit_names(&sources, &source_inputs, &index, &environment)
        .map_err(UnitNameAnalysisError::Name)?;
    // 移动完整配对而非 AST clone，公开连续 ParsedFile 切片与 index 共用规范顺序。
    parsed_sources.sort_by(|(left, _), (right, _)| left.input_key().cmp(&right.input_key()));
    let (descriptors, parsed_files) = parsed_sources.into_iter().unzip();
    let names = names.validate();
    Ok(UnitNameSnapshot {
        sources,
        descriptors,
        parsed_files,
        environment,
        names,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ast::AstNode, diagnostic::Severity, type_checking::standard_environments};

    fn assert_nodes<'a, Id: fmt::Debug + Eq, Payload: fmt::Debug + Eq + 'a>(
        actual: impl ExactSizeIterator<Item = (Id, &'a AstNode<Payload>)>,
        expected: impl ExactSizeIterator<Item = (Id, &'a AstNode<Payload>)>,
    ) {
        // ExactSizeIterator 的长度先相等，zip 不会静默截断任一 AST table。
        assert_eq!(actual.len(), expected.len());
        for ((actual_id, actual), (expected_id, expected)) in actual.zip(expected) {
            assert_eq!(actual_id, expected_id);
            assert_eq!(actual.span(), expected.span());
            assert_eq!(actual.payload(), expected.payload());
        }
    }

    fn paired(
        sources: SourceMap,
        descriptors: Vec<UnitSourceDescriptor>,
        environment: NameEnvironment,
    ) -> UnitNameSnapshot {
        // 独立真实手工前缀是 oracle，不调用待测续接或生产 inputs helper。
        let expected_parsed = descriptors
            .iter()
            .map(|unit| {
                let lexed = lex(&sources, unit.source_id()).expect("manual lex");
                parse_file(&sources, &lexed).expect("manual parse")
            })
            .collect::<Vec<_>>();
        let manual_inputs = descriptors
            .iter()
            .zip(&expected_parsed)
            .map(|(unit, parsed)| {
                SourceUnitInput::new(
                    unit.root_identity().expect("rooted source"),
                    unit.logical_path().expect("rooted source"),
                    unit.source_id(),
                    parsed,
                )
            })
            .collect::<Vec<_>>();
        let index = index_compilation_unit(&sources, &manual_inputs).expect("manual index");
        let expected =
            resolve_compilation_unit_names(&sources, &manual_inputs, &index, &environment)
                .expect("manual names");
        let expected_valid = !expected
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.severity() == Severity::Error);
        let expected_environment = environment.clone();
        let original_sources = descriptors
            .iter()
            .map(|unit| {
                let id = unit.source_id();
                (
                    id,
                    sources.source_name(id).unwrap().to_owned(),
                    sources.source_text(id).unwrap().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        // 待测输入另经真实 lex/parse 产生，随后只按值移动，不 clone AST。
        let parsed_sources = descriptors
            .into_iter()
            .map(|unit| {
                let lexed = lex(&sources, unit.source_id()).expect("input lex");
                let parsed = parse_file(&sources, &lexed).expect("input parse");
                (unit, parsed)
            })
            .collect::<Vec<_>>();
        let original_roots = parsed_sources
            .iter()
            .filter(|(_, parsed)| !parsed.roots().is_empty())
            .map(|(unit, parsed)| (unit.source_id(), parsed.roots().as_ptr()))
            .collect::<Vec<_>>();
        let snapshot = analyze_parsed_unit_names(sources, parsed_sources, environment)
            .expect("preparsed prefix");
        // 派生等值覆盖全部 names/index facts、内部 hints 及有序诊断的每个字段。
        assert_eq!(snapshot.names(), &expected);
        assert_eq!(snapshot.environment(), &expected_environment);
        assert!(std::sync::Arc::ptr_eq(
            &snapshot.environment().owner(),
            &expected_environment.owner(),
        ));
        assert_eq!(snapshot.validated_names().is_some(), expected_valid);
        let inputs = snapshot.inputs();
        let count = expected_parsed.len();
        assert_eq!(inputs.len(), count);
        assert_eq!(snapshot.descriptors().len(), count);
        assert_eq!(snapshot.parsed_files().len(), count);
        assert_eq!(snapshot.names().index().source_units().len(), count);
        for (position, input) in inputs.iter().enumerate() {
            let descriptor = &snapshot.descriptors()[position];
            let unit = &snapshot.names().index().source_units()[position];
            let actual = &snapshot.parsed_files()[position];
            assert_eq!(descriptor.source_id(), unit.source_id());
            assert_eq!(
                descriptor.root_identity().expect("rooted source"),
                unit.key().root().expect("rooted source").as_str()
            );
            assert_eq!(
                descriptor.logical_path().expect("rooted source"),
                unit.key().logical_path().expect("rooted source").as_str()
            );
            assert_eq!(input.source_id(), descriptor.source_id());
            assert_eq!(
                input.root_identity().expect("rooted source"),
                descriptor.root_identity().expect("rooted source")
            );
            assert_eq!(
                input.logical_path().expect("rooted source"),
                descriptor.logical_path().expect("rooted source")
            );
            assert_eq!(actual.source_id(), unit.source_id());
            assert!(std::ptr::eq(input.parsed(), actual));
            let old = expected_parsed
                .iter()
                .find(|old| old.source_id() == unit.source_id())
                .unwrap();
            assert_eq!(actual.package(), old.package());
            assert_eq!(actual.imports(), old.imports());
            assert_eq!(actual.roots(), old.roots());
            assert_eq!(actual.diagnostics(), old.diagnostics());
            assert_nodes(actual.ast().items().iter(), old.ast().items().iter());
            assert_nodes(
                actual.ast().statements().iter(),
                old.ast().statements().iter(),
            );
            assert_nodes(
                actual.ast().expressions().iter(),
                old.ast().expressions().iter(),
            );
            assert_nodes(
                actual.ast().type_refs().iter(),
                old.ast().type_refs().iter(),
            );
        }
        for (id, name, text) in original_sources {
            assert_eq!(snapshot.sources().source_name(id).unwrap(), name);
            assert_eq!(snapshot.sources().source_text(id).unwrap(), text);
        }
        // 非空 roots 指针只佐证向量被移动；不解引用旧指针，不代替以上语义等值。
        for (id, roots) in original_roots {
            let parsed = snapshot
                .parsed_files()
                .iter()
                .find(|parsed| parsed.source_id() == id)
                .unwrap();
            assert_eq!(parsed.roots().as_ptr(), roots);
        }
        snapshot
    }

    #[test]
    fn preparsed_sources_preserve_canonical_names_and_ast() {
        let empty = paired(SourceMap::new(), vec![], NameEnvironment::new());
        assert!(empty.validated_names().is_some());
        assert!(empty.inputs().is_empty());
        for order in [vec![0], vec![1, 0], vec![0, 1]] {
            let mut sources = SourceMap::new();
            let excluded = sources
                .add_source("excluded", "#\nval ignored = absent")
                .unwrap();
            let library = sources
                .add_source(
                    "z-display",
                    "package lib\npublic fun make(value: Int): Int { val result = value\nreturn result }",
                )
                .unwrap();
            let app = sources
                .add_source(
                    "a-display",
                    "package app\nimport lib.make as alias\nfun use(): Int = alias(7)",
                )
                .unwrap();
            let fixtures = [
                ("a-root", "lib/api.ko", library),
                ("z-root", "app/main.ko", app),
            ];
            let descriptors = order
                .into_iter()
                .map(|position| {
                    let (root, path, id) = fixtures[position];
                    UnitSourceDescriptor::new(root, path, id)
                })
                .collect();
            let (environment, _) = standard_environments();
            let snapshot = paired(sources, descriptors, environment);
            assert!(snapshot.names().diagnostics().is_empty());
            assert!(snapshot.validated_names().is_some());
            assert_eq!(snapshot.descriptors()[0].source_id(), library);
            assert_eq!(
                snapshot.sources().source_text(excluded).unwrap(),
                "#\nval ignored = absent"
            );
        }
    }

    #[test]
    fn preparsed_sources_preserve_mixed_diagnostics_once() {
        let mut sources = SourceMap::new();
        let text =
            "// 界é\n#\nval first = absent\nval duplicate = 1\nval duplicate = 2\nval syntax =\n";
        let source = sources.add_source("z-display", text).unwrap();
        let later = sources
            .add_source("a-display", "fun typeBad(): Int = true")
            .unwrap();
        let (environment, _) = standard_environments();
        let snapshot = paired(
            sources,
            vec![
                UnitSourceDescriptor::new("z-root", "later.ko", later),
                UnitSourceDescriptor::new("a-root", "mixed.ko", source),
            ],
            environment,
        );
        assert!(snapshot.validated_names().is_none());
        let diagnostics = snapshot.names().diagnostics();
        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect::<Vec<_>>(),
            ["L0001", "L0080", "L0079", "L0009"]
        );
        let spans = [
            (text.find('#').unwrap(), 1),
            (text.find("absent").unwrap(), 6),
            (text.rfind("duplicate").unwrap(), 9),
            (text.len(), 0),
        ];
        assert_eq!(diagnostics.len(), spans.len());
        for (diagnostic, (start, len)) in diagnostics.iter().zip(spans) {
            let span = diagnostic.primary_span();
            assert_eq!(span.source_id(), source);
            assert_eq!((span.start(), span.end()), (start, start + len));
        }
        assert!(!diagnostics[2].details().is_empty());
    }
}
